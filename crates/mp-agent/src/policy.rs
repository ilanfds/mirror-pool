//! The agent's action policy — the safety core of `docs/DESIGN.md` §7.1.
//!
//! An agent only participates in rounds whose action is within a policy it has
//! accepted: a whitelisted action kind at a standardized denomination. This is
//! what lets open strangers safely mirror a proposer they don't trust — a
//! poison action simply isn't expressible within the policy envelope.

use serde::{Deserialize, Serialize};

/// A standardized, non-custodial action the crowd can rally around.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind {
    Stake,
    Unstake,
}

/// A concrete action: a kind at a fixed-denomination amount (lamports).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Action {
    pub kind: ActionKind,
    pub amount: u64,
}

impl Action {
    /// Encode as the 32-byte big-endian field element used for the on-chain
    /// `action` public input: `kind` at byte 23, `amount` big-endian in the low
    /// 8 bytes. The value stays far below the field modulus and is reversible.
    pub fn to_field_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[23] = self.kind as u8;
        out[24..].copy_from_slice(&self.amount.to_be_bytes());
        out
    }

    /// Inverse of [`Action::to_field_bytes`]. Returns `None` if the encoding is
    /// not a canonical action (unexpected non-zero bytes or unknown kind).
    pub fn from_field_bytes(bytes: &[u8; 32]) -> Option<Self> {
        if bytes[..23].iter().any(|&b| b != 0) {
            return None;
        }
        let kind = match bytes[23] {
            0 => ActionKind::Stake,
            1 => ActionKind::Unstake,
            _ => return None,
        };
        let amount = u64::from_be_bytes(bytes[24..].try_into().ok()?);
        Some(Self { kind, amount })
    }
}

/// The envelope of actions an agent will mirror.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Policy {
    /// Action kinds the agent is willing to perform.
    pub allowed_kinds: Vec<ActionKind>,
    /// Standardized denominations (lamports) the agent accepts.
    pub denominations: Vec<u64>,
    /// Cap on how many proposals the agent will inject per round.
    pub max_proposals_per_round: u32,
}

impl Policy {
    /// Whether the agent will participate in a round performing `action`.
    pub fn accepts(&self, action: &Action) -> bool {
        self.allowed_kinds.contains(&action.kind) && self.denominations.contains(&action.amount)
    }
}

impl Default for Policy {
    /// A conservative starter policy: stake/unstake at 1 / 10 / 100 SOL.
    fn default() -> Self {
        const SOL: u64 = 1_000_000_000;
        Self {
            allowed_kinds: vec![ActionKind::Stake, ActionKind::Unstake],
            denominations: vec![SOL, 10 * SOL, 100 * SOL],
            max_proposals_per_round: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL: u64 = 1_000_000_000;

    #[test]
    fn action_encoding_roundtrips() {
        for kind in [ActionKind::Stake, ActionKind::Unstake] {
            let a = Action {
                kind,
                amount: 100 * SOL,
            };
            assert_eq!(Action::from_field_bytes(&a.to_field_bytes()), Some(a));
        }
    }

    #[test]
    fn default_policy_accepts_standard_actions() {
        let p = Policy::default();
        assert!(p.accepts(&Action {
            kind: ActionKind::Unstake,
            amount: 10 * SOL
        }));
        assert!(p.accepts(&Action {
            kind: ActionKind::Stake,
            amount: SOL
        }));
    }

    #[test]
    fn rejects_non_denomination_amounts() {
        let p = Policy::default();
        // 7.5 SOL is not a standardized denomination.
        assert!(!p.accepts(&Action {
            kind: ActionKind::Stake,
            amount: 7_500_000_000
        }));
    }

    #[test]
    fn rejects_disallowed_kind() {
        let p = Policy {
            allowed_kinds: vec![ActionKind::Stake],
            denominations: vec![SOL],
            max_proposals_per_round: 1,
        };
        assert!(!p.accepts(&Action {
            kind: ActionKind::Unstake,
            amount: SOL
        }));
    }

    #[test]
    fn non_canonical_encoding_is_rejected() {
        let mut bytes = Action {
            kind: ActionKind::Stake,
            amount: SOL,
        }
        .to_field_bytes();
        bytes[0] = 1; // stray high byte
        assert!(Action::from_field_bytes(&bytes).is_none());
    }
}
