# mirror-pool — A Synchronized Behavioral Mixer for Solana

**Design specification / whitepaper — draft v0.1**

> Part of the Superteam Brazil privacy tooling initiative. Sibling projects:
> `account-cooker` (autonomous believable activity) and `supersonic-tx` (fuzzy
> transaction bundles). This document specifies **mirror-pool**: a
> crowd-sourced anonymity set for *behavior*.

---

## Abstract

On a public ledger every action is legible: swaps, stakes, unstakes, LPs and
transfers are permanent, public, and increasingly fed into AI-powered analytics
that cluster wallets, attribute identities, and front-run intent in real time.

mirror-pool is **not a fund mixer**. It never takes custody of anyone's money.
It is a coordination protocol that makes *behavior* collectively deniable: many
independent wallets perform the **same standardized action inside the same
synchronized time window**, so that an observer can see *that* an action
happened but cannot determine *which participant genuinely wanted it* — nor
*which participant caused the crowd to form around it*.

We port the architecture of Tornado Cash — commitments, a Merkle membership
set, nullifiers, and relayers — from *hiding which deposit is being withdrawn*
to *hiding who originated a behavioral pattern*. The result is a production
oriented, non-custodial, Rust + Solana system built around zero-knowledge
proofs verified on-chain via the `alt_bn128` syscalls.

---

## 1. Motivation and threat model

### 1.1 What we are defending

This is about **behavioral obscurity**, not concealment of funds. The users are:

- **Algotraders** who don't want their strategies reverse-engineered by
  copy-traders shadowing their wallet.
- **Whales** who don't want every move shadowed and front-run.
- **Market makers** protecting flow and inventory.
- **Protocols & agents** that need to operate without broadcasting their entire
  playbook.
- **Everyday users** who don't want to be clustered, profiled, and tracked.

### 1.2 The adversary

We assume a well-resourced passive-and-active chain analyst who can:

- Read the full chain history and mempool, in real time.
- Run clustering / entity-resolution / copy-trade models over all activity.
- Correlate on **timing**, **amounts**, **fee payers**, **counterparties**, and
  **action sequences**.
- Sybil the pool: join as many members as it likes and observe from the inside.
- Selectively censor at the relayer/keeper layer (active).

We do **not** assume the adversary can break BN254 / Groth16 soundness, invert
Poseidon, or compel a signature from a key it does not hold.

### 1.3 Security goals

1. **Initiator anonymity (property A).** For any action that a round rallies
   around, the adversary cannot determine which participant *proposed*
   (originated) it, beyond the anonymity set of eligible proposers.
2. **Intent anonymity (property B).** For any single on-chain execution of the
   round's action, the adversary cannot determine whether that wallet executed
   it because the participant *genuinely wanted it* or purely as cover.
3. **Non-custody.** The protocol never holds participants' operating capital;
   all protocol interactions are `participant_wallet ↔ real_protocol`.
4. **Measurable privacy.** The realized anonymity-set size of any round is
   publicly auditable on-chain before a participant relies on it.
5. **Poison resistance.** No participant — insider or outsider — can steer the
   crowd into a harmful action.

### 1.4 Non-goals

- **Fund unlinkability.** After a private unstake, the resulting liquid funds
  are still in a traceable wallet. Laundering the *funds* is the job of a fund
  mixer or of `account-cooker`, not mirror-pool.
- **Hiding that a wallet is a pool member.** Membership (a deposit of a
  commitment) is public, exactly as in Tornado Cash. What is hidden is the link
  between membership and *origination / genuine intent*.
- **Consensus-level metadata** (IP addresses, RPC provider logs). Out of scope;
  mitigated operationally by relayer/keeper decentralization.

---

## 2. Relationship to Tornado Cash

mirror-pool reuses Tornado's cryptographic skeleton but retargets the secret it
protects.

| | Tornado Cash | mirror-pool |
|---|---|---|
| What enters the contract | **the funds** (custodial) | **only a commitment** `H(k,r)` — data, not money |
| Where the action happens | inside the contract | on each **participant's own wallet**, against the **real protocol** |
| Unit of privacy | a fixed-denomination note | a fixed-denomination *action* |
| Secret being hidden | *which deposit* is being withdrawn | *who originated / genuinely wanted* a behavioral pattern |
| Nullifier prevents | double-withdrawal | double-proposal within a round |
| Relayer role | submit withdrawal, hide recipient gas | submit proposal (hide proposer) / broadcast pre-signed executions (sync) |
| Anonymity set | all deposits since last zero balance | all wallets executing the round's action |

The design principle we inherit: **make the sensitive event indistinguishable
from a large set of identical events, and break the link to the actor with a
zero-knowledge membership proof plus a nullifier.**

---

## 3. The two anonymities (core idea)

A common confusion is to model this as copy-trading — "a leader acts, followers
copy." That is exactly wrong: a visible leader is *more* exposed, and a fixed
follower group is a perfect cluster. Instead, mirror-pool composes **two
independent anonymities**:

| Anonymity | Hides | Mechanism | Cost |
|---|---|---|---|
| **A — initiator** | *which participant caused the round's action* | anonymous ZK proposal + per-round nullifier, submitted via relayer | requires ZK |
| **B — intent** | *which of the executing wallets genuinely wanted it* | uniform action + tight synchronization across many wallets | **free** |

Property **B** is what delivers *"everyone's privacy"*: any participant whose
genuine intent matches the round's standardized action is hidden among the crowd
at no marginal cost. Property **A** is where the hard cryptography lives, and it
is what stops an observer from identifying who *summoned* the crowd.

**Steering ≠ privacy.** The right to *choose what the crowd rallies around* is a
governance question (§7), deliberately separated from the privacy mechanism.
Gating who may steer never gates who may hide.

---

## 4. Roles

- **Pool program** — the on-chain Solana program. Holds the membership Merkle
  tree, the nullifier set, per-round state, the injection-key commitment, and
  the (optional) incentive pot. Verifies Groth16 proofs. Never holds operating
  capital.
- **Member / executor** — anyone who deposits a membership commitment. Runs an
  agent that participates in rounds and provides cover. Fully self-custodial.
- **Proposer** — a member who summons a round's action anonymously. By default
  **any member may propose**; optional policies (§7.2) can restrict this.
- **Relayer** — a trust-minimized submitter of *proposals*, so the proposer's
  wallet is never the on-chain fee payer of a proposal. Cannot alter a proposal
  (the action is bound in the proof).
- **Keeper** — a trust-minimized broadcaster of *pre-signed executions*, firing
  the crowd's transactions inside one tight window. It can censor but cannot
  alter, steal, or act as counterparty (§6.5, §9).

Relayers and keepers are **infrastructure roles**, expected to be run by many
independent parties, and are economically incentivized (§8). Neither is ever a
custodian.

---

## 5. Cryptographic building blocks

- **Poseidon** (`H`) — a ZK-friendly hash over the BN254 scalar field, used for
  commitments, the Merkle tree, nullifiers, and the injection-key commitment.
  Rust: `light-poseidon`.
- **Incremental Merkle tree** — fixed-height (e.g. 26) membership accumulator;
  the program stores a rolling history of the last *n* roots so proposals may
  reference any recent root (as in Tornado).
- **Groth16 over BN254** — succinct proofs. Off-chain proving in Rust with
  `ark-groth16` and circuits expressed as R1CS via `ark-relations` /
  `ark-r1cs-std` (keeping the circuit itself in Rust). On-chain verification via
  `groth16-solana` using the `sol_alt_bn128_*` syscalls.
- **Durable nonces** — Solana nonce accounts let a participant pre-sign an
  execution transaction that stays valid until *their own* nonce advances,
  enabling "sign once at commit, broadcast later" without standing delegation
  (§6.4).

> **Trusted setup.** Groth16 requires a per-circuit trusted setup. Production
> deployment requires a multi-party (Powers-of-Tau style) ceremony; this repo
> ships a *development* proving/verifying key and documents the ceremony as a
> release blocker. This is called out honestly rather than hidden.

### 5.1 Notation

- `k` — nullifier secret (per member). `r` — commitment randomness.
- `C = H(k, r)` — membership commitment (the "deposit").
- `T_m` — membership Merkle tree, root `R_m`, opening `O(l)` for leaf index `l`.
- *(optional, curated mode only — §7.2)* `s` — a pool injection key held by the
  creator and shared off-chain with authorized proposers; `H_s = H(s)` is its
  public commitment. Omitted in the default open configuration.
- `round_id` — unique identifier of a round.
- `nf = H(k, round_id)` — per-round proposal nullifier. Derived from `k` (not
  `s`) so that distinct members yield distinct nullifiers, and so nullifiers are
  **uncorrelated across rounds** (long-lived membership, per-round anonymity).

---

## 6. Protocol

### 6.1 Pool creation

The creator chooses the action vocabulary/policy (§7.1), the threshold `N`, and
the round timing. In the **default (open) configuration** any member may
propose, so no injection key is needed. Optional injection-governance modes
(curated or bonded) are described in §7.2.

### 6.2 Join (membership)

A member samples `(k, r)`, computes `C = H(k, r)`, and submits `C` to the pool
program, which appends it to `T_m` and pushes the previous root into history.
Membership is **long-lived**: one deposit enables participation in arbitrarily
many future rounds. `(k, r)` is the member's private note; losing it means
losing the ability to propose (but never funds, which are never held here).

### 6.3 Round lifecycle

Rounds are instances of an **action template** at a **fixed denomination**
(§7.1), with explicit time windows:

```
 Propose ─► Seal ─► Commit ─► Threshold ─► Execute ─► Close
```

1. **Propose.** Anonymous proposals arrive (§6.4). Recurring rounds may be
   pre-scheduled and skip this phase entirely — participants just join, getting
   property B for free.
2. **Seal.** The round's action and parameters are **frozen**. This must precede
   Commit, because participants pre-sign the exact transaction.
3. **Commit.** Each participant pre-signs the exact execution transaction using
   a durable nonce and registers an anonymous commit; the public commit counter
   increments (§6.4, §6.5).
4. **Threshold.** If `commits ≥ N`, the program marks the round `GO`; otherwise
   `ABORT`. This is enforced **collectively**.
5. **Execute.** Only on `GO`: every committed participant's pre-signed
   transaction is broadcast (by keepers and/or self) inside a tight, jittered
   window. Each transaction is `participant_wallet ↔ real_protocol`.
6. **Close.** Nullifiers spent this round are finalized; incentive accounting
   settles (§8).

The `ABORT` path is the thin-crowd protection: in a weak round **nobody
executes**, so no participant is ever exposed in a small set. This recreates
Tornado's "the set is already visible when you withdraw" property inside a
single round — the commit phase *is* the visible set, the execute phase *is* the
withdrawal.

### 6.4 Anonymous proposal (property A)

To summon a round's action `A`, a proposer produces a Groth16 proof of the
statement (default open configuration):

```
S_propose[R_m, nf, A, round_id] =
  I KNOW  k, r, l, O  SUCH THAT
      C  = H(k, r)                       (commitment)
   ∧  O  opens C to R_m at index l        (membership in the pool)
   ∧  nf = H(k, round_id)                 (per-round nullifier)
```

Public inputs: `R_m`, `nf`, `round_id`, and a binding of `A`. The action `A` is
bound into the public inputs exactly as Tornado binds recipient and fee, so the
proof is **non-malleable**: a relayer cannot swap `A` for `A'`. This is the
canonical Tornado membership statement, extended only by binding `A`.

*Curated mode (optional, §7.2)* additionally proves `H(s) = H_s`, adding `H_s`
to the public inputs; *bonded mode* additionally proves membership in an
injector accumulator.

The program verifies the proof, checks that `nf` is unused for `round_id`
(one proposal per member per round), validates `A` against the policy (§7.1),
and appends `A` to the round's candidate set. The proposal is submitted **via a
relayer** so no known wallet is linked to the act of proposing.

- By **default any member may propose** — one proposal per member per round,
  enforced by the nullifier. This maximizes initiator anonymity and keeps the
  circuit minimal. Griefing is bounded because participants **opt in per round**:
  a proposal nobody wants simply fails to reach the threshold and aborts (§6.3),
  at zero cost to anyone.
- Restricting *who* may propose is an optional governance choice (§7.2), never a
  requirement for safety — safety comes from the constrained vocabulary (§7.1)
  plus per-round opt-in.

### 6.5 Commit and synchronized execution (property B)

Behavior is not fungible: to execute "unstake 100 SOL" each participant acts on
*their own* staked SOL, from *their own* wallet, against the *real* protocol.
The protocol coordinates **timing and uniformity**, never funds.

We support a spectrum of commit/execution bindings:

| Mode | Sign again at Execute? | Commit counter | Custody |
|---|---|---|---|
| **Voluntary** | yes (agent re-signs) | soft (committers may no-show) | fully self |
| **Durable-nonce (default)** | **no** (pre-signed at Commit) | **hard** | fully self |
| **Delegated** | no (standing scoped authority) | hard | scoped, revocable |

**Durable-nonce (recommended default).** At Commit the participant pre-signs the
exact execution transaction against a durable nonce, so it remains valid until
their own nonce advances, and hands it to a keeper. At Execute the keeper
broadcasts it. Consequences:

- **The keeper never interacts with protocols on its own behalf.** On-chain the
  record is `participant_wallet ↔ protocol`, signed by the participant, on the
  participant's accounts. If the participant is the fee payer, the keeper appears
  *nowhere* on-chain — it is a pure broadcaster.
- **A single keeper doing everything would be no cover at all** (one entity,
  custodial). The crowd exists precisely because many *distinct* wallets each
  act; the keeper only synchronizes *when*.
- **Trust-minimized keeper.** It holds a transaction signed by the participant:
  it cannot alter it (signature breaks), cannot steal (the tx only performs the
  scoped action on the participant's own funds), and can only *censor* — which
  merely thins that participant's round. Mitigated by running many keepers and
  by self-broadcast fallback.
- **Free opt-out.** Only the participant (the nonce authority) can advance their
  nonce. Advancing it invalidates the pre-signed transaction — a clean unilateral
  cancellation before Execute.
- **Hard commit counter.** Because each commit is a real, pending, signed
  transaction, the count that the threshold checks is trustworthy.

**Delegated.** For frictionless recurring cover, a participant grants the keeper
a *scoped, revocable* authority. On Solana this is idiomatic and safe because
authorities are separable: delegating a stake account's **stake authority**
(may stake/unstake/redelegate) while retaining its **withdraw authority** lets a
keeper provide cover but *never* move funds out.

---

## 7. Action vocabulary and injection governance

### 7.1 The constrained action vocabulary (safety core)

Because open strangers mirror actions summoned by a proposer they don't
personally trust, the primary poison defense is **constraining what can be
proposed**, not who proposes it:

- Only **whitelisted, non-custodial** interactions with vetted programs (stake,
  unstake, LP add/remove, swaps on approved venues, etc.).
- **No** arbitrary CPIs, token `approve` to arbitrary spenders, or authority
  transfers.
- **Fixed denominations** (e.g. units of {1, 10, 100} SOL). A large intent is
  expressed as many standard units across rounds, exactly as Tornado uses
  fixed-size notes. Non-standard amounts would deanonymize by size.
- **Bounded rate** (max actions per round, cooldowns).

A poison intent is thus *not expressable* and is rejected at the protocol level
regardless of who submitted it. This defeats the malicious insider too — because
even an authorized proposer cannot express a harmful action — and it lets the
proposer set stay large, which *protects* initiator anonymity.

Bias the vocabulary toward **net-neutral / yield-bearing** actions (staking,
LPing) so that providing cover costs little more than transaction fees (§8).

### 7.2 Injection governance (optional)

By **default, proposal is open**: any member may propose, one per round. This is
the recommended configuration — it maximizes initiator anonymity, keeps the
circuit minimal, and is safe because (a) the vocabulary constrains *what* can be
proposed (§7.1) and (b) participants **opt in per round**, so a bad proposal
just fails the threshold and aborts at no cost.

Restricting *who* may steer is a pluggable policy for deployments that want it —
one circuit, three configurations:

- **Open (default).** Any member proposes (one per round via the nullifier).
  Maximum proposer anonymity. Relies on §7.1 + per-round opt-in for safety.
- **Curated / shared key** (`H_s` published). Only holders of `s` may propose.
  Suited to a **closed, trust-based cohort** where the creator vouches for
  everyone they hand `s` to. Simpler than an accumulator, but note:
  - a shared secret is only as strong as its least careful holder; it cannot be
    revoked per-member and must be rotated on any leak;
  - crucially, it does **not** by itself stop a *malicious insider* (who knows
    `s`) — that is handled by §7.1, not by the gate.
  - Leaking `s` later does **not** retroactively deanonymize past proposals
    (membership ZK + `k`-derived nullifiers); it only enables future proposals
    until rotation.
- **Bonded / accumulator** (recommended for open, adversarial deployments).
  Replace the single `s` with a second Merkle accumulator of injector
  credentials `H(s_i)`, each backed by a stake bond. A proposer proves
  membership in *both* the pool tree and the injector tree; griefers are removed
  by slashing (leaf revocation). This is individually revocable, has no shared
  secret, keeps proposers anonymous, and implements "provide cover to get cover."

Because the injector set may be curated but the **executor/cover crowd is open
and public**, the anonymity set for property B remains large even when steering
is restricted: a big open crowd provides on-chain cover; a curated set steers.

---

## 8. Incentives

The pool is only private if many wallets execute the same action at the same
time. Executing costs fees and possibly slippage/churn, so cover is a public
good with a free-rider problem. "Incentives that keep people in it" is a
first-class design requirement, addressed in three composable layers:

1. **Cover market.** The party that needs privacy pays a fee into the round pot;
   the pot is distributed to the wallets that provided cover. This is Tornado's
   relayer fee generalized: demand (privacy-seekers) pays supply (cover
   providers), turning cover from a cost into a small profit.
2. **Reciprocal cover-credits (non-monetary).** Executing a round earns credits;
   summoning cover for your own action spends them. "Provide cover to get
   cover." No money changes hands, which avoids the reward-wallet clustering
   problem — at the cost of bootstrapping and accounting.
3. **Net-neutral / yield-bearing vocabulary.** Choosing actions people would
   perform anyway (stake for yield, LP for fees) minimizes the *cost of cover*
   to roughly transaction fees, which in turn keeps the required fee (1) or
   credit balance (2) small.

Paying cover providers on-chain risks correlating their reward wallets over
time; a fully unlinkable reward scheme is an open problem (§10).

---

## 9. Anonymity analysis

**What is hidden.**

- *Initiator* (property A): hidden among eligible proposers by the membership
  ZK proof; the relayer removes the proposal fee-payer footprint. Off-chain,
  the adversary cannot enumerate `s`-holders, so the effective proposer set is
  not trivially the (possibly small) authorized group.
- *Genuine intent* (property B): hidden among all wallets executing the round's
  standardized action.

**Realized set size** is a public, aggregate count of executions of action `A`
in round `T`. It requires **no per-member attestation** — the metric that
protects everyone is a *count*, not an *identification* — so measuring privacy
never itself creates a deanonymizing link.

**Leak vectors and mitigations.**

| Vector | Risk | Mitigation |
|---|---|---|
| **Amount** | non-standard sizes single you out | fixed denominations (§7.1) |
| **Timing** | execution ordering correlates actor | tight window + jitter; keeper batch broadcast |
| **Fee payer** | proposer's wallet pays for proposing | relayer submits proposals |
| **Thin crowd** | small set exposes the real intent | collective threshold + `ABORT` (§6.3) |
| **Cover labeling** | publicly-bonded coverers shrink the set | prefer aggregate threshold over per-member enforcement; unlinkable attestation (§10) |
| **Keeper knowledge** | keeper sees `wallet→action` before broadcast | same surface as a Tornado relayer; decentralize keepers, self-broadcast fallback |
| **Cross-round correlation** | repeated participation clusters a wallet | per-round-uncorrelated nullifiers; rotate execution wallets; net-neutral vocabulary makes participation ordinary |
| **Sybil membership** | one entity floods proposals via many leaves | deposit cost + per-round proposal cap; bonded injectors (§7.2) |

---

## 10. Open problems (research frontier)

These are deliberately unsolved in v1 and are where the strongest contributions
lie:

- **Unlinkable participation attestation.** Rewarding/slashing a specific
  member for (non-)execution tends to link their execution wallet to their
  identity, shrinking the set. A zero-knowledge attestation ("I, an anonymous
  member, executed this round") that feeds reward/slash without the link is the
  deep prize.
- **Decentralized, censorship-resistant keepers/relayers.** A committee,
  threshold-signed, or MEV-style competitive market for broadcasting rounds.
- **Trusted-setup ceremony.** A real Powers-of-Tau MPC for the proposal circuit,
  or migration to a transparent-setup proof system whose on-chain verification
  stays affordable on Solana.
- **Adaptive, adversary-aware scheduling.** Choosing denominations, round
  cadence, and vocabulary to maximize measured resistance against concrete
  clustering/copy-trade models, rather than naive randomness.

---

## 11. Rust / Solana architecture

Everything is Rust, end to end, per the bounty constraint.

```
mirror-pool/
├── programs/
│   └── mirror_pool/        # on-chain program (Anchor or Pinocchio)
│       # membership tree, root history, nullifier set, round state machine,
│       # Groth16 verification (groth16-solana / alt_bn128), incentive pot
├── circuits/
│   └── propose/            # R1CS proposal circuit (ark-relations / ark-r1cs-std)
│       # + Groth16 setup, proving/verifying keys (dev keys; ceremony TODO)
├── crates/
│   ├── mp-crypto/          # Poseidon (light-poseidon), Merkle, note/nullifier
│   ├── mp-proof/           # ark-groth16 proving; public-input encoding
│   ├── mp-agent/           # participant: keygen, commit, durable-nonce presign,
│   │                       #   round watcher, execution, policy engine
│   ├── mp-relayer/         # proposal relayer (trust-minimized)
│   └── mp-keeper/          # execution broadcaster / synchronizer
└── docs/
    └── DESIGN.md           # this document
```

**On-chain (Rust program):** membership Merkle tree + rolling root history;
per-round nullifier set; round state machine (`Propose→Seal→Commit→Threshold→
Execute→Close`); Groth16 verifier; optional injection-governance state (`H_s` /
injector accumulator); incentive pot accounting. Never holds operating capital.

**Off-chain (Rust):** proof generation (`ark-groth16`), agent orchestration
(`tokio`, `solana-client`), durable-nonce management, policy validation,
relayer and keeper services.

---

## 12. Security considerations (summary)

- **Non-custody invariant.** No instruction path lets the program, keeper, or
  relayer move a participant's operating funds. Enforced by construction:
  executions are participant-signed; delegation (if used) is scoped to
  non-withdraw authorities.
- **Proof non-malleability.** The action and round are bound into public inputs;
  relayers/keepers cannot alter intent.
- **Replay / double-proposal.** Prevented by per-round nullifiers.
- **Poison / griefing.** Prevented by the constrained vocabulary; further
  bounded by bonded injectors and per-round caps.
- **Signature compulsion.** Impossible by design; hence privacy guarantees are
  *measurable and economic* (threshold + incentives), never a promise that a
  given wallet will act.

---

## 13. References

1. A. Pertsev, R. Semenov, R. Storm. *Tornado Cash Privacy Solution*, v1.4, 2019.
2. J. Groth. *On the Size of Pairing-Based Non-interactive Arguments*,
   EUROCRYPT 2016.
3. Solana `alt_bn128` syscalls; Light Protocol `groth16-solana`.
4. `light-poseidon`; arkworks (`ark-groth16`, `ark-relations`, `ark-r1cs-std`).
5. Solana durable nonces (nonce accounts) documentation.

---

*Draft v0.1 — open for revision. Licensed MIT (see `LICENSE`).*
