//! Build an anonymous proposal: turn a membership note + a tree snapshot into
//! the arguments the on-chain `propose` instruction expects.
//!
//! This is the agent's Property-A core (`docs/DESIGN.md` §6.4): it generates a
//! Groth16 proof (via `mp-proof`) that the note's commitment sits under the
//! tree root and that the revealed nullifier is `H(k, round_id)`, binding the
//! action — without revealing which member proposed.

use crate::policy::Action;
use ark_bn254::Bn254;
use ark_groth16::ProvingKey;
use ark_std::rand::{CryptoRng, RngCore};
use mp_crypto::{field, IncrementalMerkleTree, Note};
use mp_proof::circuit::ProposeCircuit;
use mp_proof::proving::prove;
use mp_proof::solana::proof_to_bytes;

/// The exact arguments for the on-chain `propose` instruction.
#[derive(Clone, Debug)]
pub struct ProposalArgs {
    pub round_id: u64,
    pub root: [u8; 32],
    pub nullifier: [u8; 32],
    pub action: [u8; 32],
    pub proof_a: [u8; 64],
    pub proof_b: [u8; 128],
    pub proof_c: [u8; 64],
}

/// Produce a proposal for `action` in `round_id`, proving membership of `note`
/// at `leaf_index` in `tree`.
pub fn build_proposal<R: RngCore + CryptoRng>(
    pk: &ProvingKey<Bn254>,
    note: &Note,
    tree: &IncrementalMerkleTree,
    leaf_index: usize,
    round_id: u64,
    action: &Action,
    rng: &mut R,
) -> anyhow::Result<ProposalArgs> {
    let path = tree
        .opening(leaf_index)
        .ok_or_else(|| anyhow::anyhow!("leaf {leaf_index} is not in the tree"))?;
    let root = tree.root();

    let round_id_fr = field::from_u64(round_id);
    let action_bytes = action.to_field_bytes();
    let action_fr = field::from_bytes_be(&action_bytes)
        .ok_or_else(|| anyhow::anyhow!("action does not encode a field element"))?;

    let circuit = ProposeCircuit::from_witness(note, &path, root, round_id_fr, action_fr);
    let proof = proof_to_bytes(&prove(pk, circuit, rng));

    Ok(ProposalArgs {
        round_id,
        root: field::to_bytes_be(&root),
        nullifier: field::to_bytes_be(&note.nullifier(round_id_fr)),
        action: action_bytes,
        proof_a: proof.a,
        proof_b: proof.b,
        proof_c: proof.c,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ActionKind;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use groth16_solana::groth16::{Groth16Verifier, Groth16Verifyingkey};
    use mp_crypto::field::from_u64;
    use mp_proof::proving::setup;
    use mp_proof::solana::vk_to_bytes;

    #[test]
    fn builds_a_proposal_that_the_onchain_verifier_accepts() {
        let height = 10;
        let mut rng = StdRng::seed_from_u64(7);
        let (pk, vk) = setup(height, &mut rng);

        // Build a tree with our note in it.
        let note = Note::new(from_u64(5), from_u64(6));
        let mut tree = IncrementalMerkleTree::new(height);
        tree.insert(from_u64(111)).unwrap();
        let idx = tree.insert(note.commitment()).unwrap();

        let action = Action {
            kind: ActionKind::Unstake,
            amount: 10_000_000_000,
        };
        let args = build_proposal(&pk, &note, &tree, idx, 42, &action, &mut rng).unwrap();

        // The action decodes back to the structured form.
        assert_eq!(Action::from_field_bytes(&args.action), Some(action));

        // Verify with the exact verifier that runs on-chain.
        let vkb = vk_to_bytes(&vk);
        let gvk = Groth16Verifyingkey {
            nr_pubinputs: vkb.ic.len(),
            vk_alpha_g1: vkb.alpha,
            vk_beta_g2: vkb.beta,
            vk_gamme_g2: vkb.gamma,
            vk_delta_g2: vkb.delta,
            vk_ic: &vkb.ic,
        };
        let round_id_be = field::to_bytes_be(&from_u64(args.round_id));
        let public = [args.root, args.nullifier, round_id_be, args.action];
        let mut verifier =
            Groth16Verifier::<4>::new(&args.proof_a, &args.proof_b, &args.proof_c, &public, &gvk)
                .unwrap();
        assert!(verifier.verify().is_ok(), "agent proposal must verify");
    }

    #[test]
    fn fails_for_a_leaf_not_in_the_tree() {
        let height = 8;
        let mut rng = StdRng::seed_from_u64(1);
        let (pk, _vk) = setup(height, &mut rng);
        let note = Note::new(from_u64(1), from_u64(2));
        let tree = IncrementalMerkleTree::new(height); // empty
        let action = Action {
            kind: ActionKind::Stake,
            amount: 1,
        };
        assert!(build_proposal(&pk, &note, &tree, 0, 1, &action, &mut rng).is_err());
    }
}
