# mirror-pool — Implementation Roadmap

**Draft v0.1 — companion to [`DESIGN.md`](./DESIGN.md).**

This document turns the architecture into a concrete, phased build plan. It is
ordered to **de-risk the hardest parts early** (the ZK circuit and on-chain
verification) while producing a working, demonstrable increment at each step.

---

## 0. Guiding principles

- **Vertical slice first.** Build the thinnest end-to-end path (deposit →
  anonymous propose → round → synchronized execute) before breadth.
- **One action to start.** Scope the v1 vocabulary to a *single* standardized
  action — **native SOL stake / deactivate** — because it has no external
  protocol dependency and is trivial to test. Add Marinade / LP / swaps later.
- **Test at the lowest level that proves the property.** Pure-Rust crypto tests
  before any validator; `LiteSVM` for fast program tests; `Surfpool`
  (local + mainnet fork) only for integration with real protocols.
- **Honesty over polish.** Ship dev trusted-setup keys with a loud "ceremony
  required" banner; mark every production blocker explicitly (§ Release gates).
- **Each phase ends green.** No phase is "done" until its Definition of Done
  (DoD) passes in CI.

---

## 1. Tech decisions (locked for v1)

| Concern | Choice | Rationale |
|---|---|---|
| On-chain framework | **Anchor** | velocity + tooling; hot paths can move to Pinocchio later |
| ZK proving (off-chain) | **arkworks** (`ark-bn254`, `ark-groth16`, `ark-relations`, `ark-r1cs-std`) | pure Rust circuit + proving → satisfies Rust-only |
| On-chain verification | **`groth16-solana`** + `sol_alt_bn128_*` syscalls | production-proven Groth16 verify on Solana |
| Hash | **`light-poseidon`** (BN254) | ZK-friendly, Solana-compatible |
| Program unit tests | **`LiteSVM`** | fast, in-process, no validator |
| Integration tests | **`Surfpool`** (local + mainnet fork) | exercise real staking against a fork |
| Off-chain runtime | **`tokio`** + `solana-client` / `solana-sdk` | standard Rust Solana stack |
| First action (vocabulary v1) | **native stake program** (`Stake`, `Deactivate`) | no external dep, easy to assert |

> These are defaults, revisable if a phase surfaces a blocker (e.g. proof size
> or CU budget forcing a Pinocchio rewrite of the verifier path).

---

## 2. Component / dependency map

```
mp-crypto ──┬─► circuits/propose ──► mp-proof ──┐
            │                                    ├─► mp-agent ──► mp-keeper
            └─► programs/mirror_pool ◄───────────┘            └─► mp-relayer
                     ▲
                (groth16-solana verify)
```

**Critical path:** `mp-crypto` → `circuits/propose` → on-chain Groth16 verify.
Everything else (agent, relayer, keeper, incentives) can proceed in parallel
once the proof round-trips on-chain.

---

## 3. Phases

Each phase lists **scope**, **DoD**, and **risk**.

### Phase 0 — Workspace & CI  ·  size: S
**Scope**
- Cargo workspace with crate skeletons per `DESIGN.md` §11.
- CI: `cargo build`, `clippy -D warnings`, `fmt --check`, `test`.
- MIT license headers, `README.md` stub, `CONTRIBUTING.md`.
- `rust-toolchain.toml`, pinned Solana + Anchor versions.

**DoD** — `cargo build --workspace` and CI are green on an empty skeleton.
**Risk** — Low. Version alignment (Anchor ↔ Solana ↔ arkworks) is the only trap.

---

### Phase 1 — `mp-crypto` (pure Rust)  ·  size: M
**Scope**
- BN254 `Fr` field helpers; byte/field encoding conventions (fix endianness now).
- Poseidon wrappers (`light-poseidon`): `H(k,r)`, `H(k, round_id)`.
- Incremental fixed-height Merkle tree: insert, current root, root history,
  `opening(l)` (sister-node path).
- Note type: `(k, r)` ↔ commitment `C`; nullifier derivation.

**DoD**
- Unit + property tests: insert-then-open verifies against root for random trees.
- **Cross-checked test vectors** that the on-chain program (Phase 2) must
  reproduce bit-for-bit.
**Risk** — Medium. Endianness / field-encoding mismatches here cause the hardest
bugs later; nail the vectors down.

---

### Phase 2 — On-chain program, **no ZK yet**  ·  size: L
**Scope**
- Accounts: `PoolConfig`, `MerkleTreeState` (root + history + filled subtrees +
  next index), nullifier PDAs, `Round`, `IncentivePot`.
- Instructions: `initialize_pool`, `deposit` (on-chain incremental Merkle
  insert), round state machine (`open → seal → commit → threshold → close`),
  `advance_round`.
- `propose` present but **proof check stubbed / feature-gated off**.

**DoD** (LiteSVM)
- Depositing the Phase-1 test vectors yields the **exact same root** on-chain.
- Round lifecycle: phases advance by slot/time; `commit` increments counter;
  threshold correctly emits `GO`/`ABORT`.
- Double-init, wrong-phase, and overflow paths revert.
**Risk** — Medium. On-chain Merkle insert must match `mp-crypto` exactly; CU
budget for the insert at target tree height.

---

### Phase 3 — Proposal circuit (`circuits/propose` + `mp-proof`)  ·  size: L
**Scope**
- R1CS constraints for `S_propose` (`DESIGN.md` §6.4): commitment, Merkle
  membership, nullifier, `A` binding — the canonical Tornado statement + `A`.
- Groth16 **dev** setup; `prove`; native (off-chain) `verify`.
- Public-input encoding module (single source of truth, shared with the program).

**DoD**
- Proof for a known membership verifies off-chain; tampering with `A`, `nf`,
  `R_m`, or the opening makes verification fail.
- Proving time and constraint count recorded (baseline for optimization).
**Risk** — **High.** This is the crux. Circuit bugs are subtle; Merkle gadget +
Poseidon-in-circuit must match `mp-crypto`'s out-of-circuit hashing.

---

### Phase 4 — On-chain Groth16 verification  ·  size: L
**Scope**
- Wire `groth16-solana` into `propose`: verify proof, check `nf` unused, validate
  `A` against policy, register into the round.
- Freeze the public-input serialization across prover ↔ program.

**DoD** (LiteSVM)
- An off-chain-generated proof **verifies on-chain**; a tampered proof or reused
  nullifier reverts.
- CU cost of verification measured and within budget.
- 🎯 **Milestone: anonymous proposal works end-to-end.**
**Risk** — High. CU limits, `alt_bn128` input formatting, endianness parity with
Phase 3.

---

### Phase 5 — `mp-agent` (participant)  ·  size: L
**Scope**
- Keygen + encrypted note storage; execution wallet(s) + **durable nonce**
  account management.
- Round watcher (program-state subscription).
- Proof generation wiring (`mp-proof`).
- Policy engine: which rounds to join, per user config (action, denomination,
  caps).
- `commit`: build + **pre-sign** the native-stake execution tx against a durable
  nonce; hand to keeper (or self-broadcast).

**DoD** (Surfpool)
- A single agent completes a full round against **real native staking** on a
  fork: deposit → propose → commit(pre-sign) → execute.
- Nonce lifecycle correct; opt-out (advance nonce) invalidates the pre-signed tx.
**Risk** — Medium. Durable-nonce edge cases; blockhash/nonce lifecycle.

---

### Phase 6 — `mp-relayer` & `mp-keeper`  ·  size: M
**Scope**
- **Relayer**: accept proposal payloads, submit so proposer isn't fee payer;
  cannot alter `A` (bound in proof).
- **Keeper**: collect pre-signed execution txs, fire on `GO` inside a jittered
  window; self-broadcast fallback; multi-keeper tolerance.

**DoD** (Surfpool)
- **N ≥ 50 agents** run a synchronized round; executions land inside the window;
  keeper is absent from on-chain records (agents are fee payers).
- Killing a keeper mid-round → agents self-broadcast; round still completes.
**Risk** — Medium. Synchronization jitter tuning; RPC throughput at scale.

---

### Phase 7 — Incentives  ·  size: M
**Scope**
- Cover market: privacy-seeker funds the round pot; threshold-gated distribution
  to cover providers.
- Optional reciprocal cover-credits accounting on-chain.

**DoD** (LiteSVM + Surfpool)
- Pot funds/distributes correctly; payouts only on `GO`; credits balance under a
  multi-round simulation.
**Risk** — Medium. Payout without correlating reward wallets is only *partially*
solvable here (full solution is a research item — § Research track).

---

### Phase 8 — Integration, adversarial eval & docs  ·  size: L
**Scope**
- End-to-end scenario: **100+ agents**, a whale hiding an unstake across many
  rounds via fixed denominations.
- **Adversarial harness**: run clustering / copy-trade heuristics over the
  produced chain data and show attribution fails; report measured anonymity-set
  size and timing distribution.
- `README` with install path + quickstart; architecture notes; demo script.

**DoD**
- Reproducible demo (`just demo` / script) spins up the scenario on Surfpool.
- Adversarial report checked into `docs/` with metrics.
- 🎯 **Milestone: production-oriented, demonstrable, documented tool.**
**Risk** — Medium. Making the adversarial eval credible (not a strawman) is the
part judges will scrutinize.

---

## 4. MVP / walking skeleton (first demonstrable win)

The smallest slice that proves the thesis, cutting incentives, relayers, and
multi-action breadth:

> **Phases 1 → 4 + a minimal single-agent path from Phase 5**, with the native
> stake action and durable-nonce execution.
>
> Demo: one member deposits, anonymously proposes "deactivate 1 stake unit,"
> the round seals, the member pre-signs and the tx executes — with an
> on-chain-verified ZK proof and no link from the proposal to a wallet.

Hitting this proves the hardest 80% (circuit + on-chain verify + non-custodial
execution). Everything after is breadth and hardening.

---

## 5. Release gates (production blockers — do NOT ship without)

- [ ] **Trusted-setup ceremony** (Powers-of-Tau MPC) replacing dev keys.
- [ ] **Security review / audit** of the program and circuit.
- [ ] **Keeper/relayer decentralization** (no single censoring party).
- [ ] **Fuzzing** of instruction inputs and proof deserialization.
- [ ] **CU / cost profiling** at target tree height and participant scale.

These are tracked openly; the tool is labeled *experimental* until all pass.

---

## 6. Research track (parallel — where the deepest contribution lies)

Independent of the linear build, these advance the state of the art (see
`DESIGN.md` §10):

- **Unlinkable participation attestation** — ZK proof of "an anonymous member
  executed this round" feeding reward/slash without linking wallets.
- **Adversary-aware scheduling** — tuning denominations/cadence against concrete
  clustering models rather than naive randomness.
- **Transparent-setup migration** — a proof system avoiding trusted setup while
  keeping Solana verification affordable.

---

## 7. Mapping to the bounty judging criteria

| Criterion | How the plan delivers |
|---|---|
| **Impact** | Non-custodial, real ZK, defeats clustering on a real action (staking) — not a toy |
| **Quality** | Phased, tested at every layer, CI-green, honest release gates |
| **Volume** | On-chain program + circuit + proving + agent + relayer + keeper + incentives + adversarial eval — a full suite |

Depth path (win one repo): go all the way through Phase 8 + at least one
Research-track result.

---

## 8. Suggested order of execution (summary)

```
P0 workspace ─► P1 crypto ─► P2 program(no ZK) ─┐
                     └─► P3 circuit ─► P4 on-chain verify ─► [MVP]
                                                  │
              P5 agent ─► P6 relayer/keeper ─► P7 incentives ─► P8 integration+eval
```

Start: **Phase 0**, then **Phase 1**. First target: the **MVP** in §4.

---

*Draft v0.1 — open for revision. Licensed MIT.*
