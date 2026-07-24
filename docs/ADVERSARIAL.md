# Adversarial evaluation

**Does the synchronized crowd actually defeat chain-analysis, or is it noise a
real observer sees through?** This is the question the bounty cares about most —
*"noise that actually defeats modern clustering, not naive randomness"* — so we
answer it with numbers, reproducibly, in [`crates/mp-eval`](../crates/mp-eval).

## Method

We simulate rounds under two behaviors and run the **same** adversary against
both:

- **mirror-pool** — every participant, *including the initiator*, executes at an
  i.i.d. uniform time inside the round's window. Uniform, synchronized jitter.
- **naive copy-trading** — the initiator acts first and followers react strictly
  later. (This is the leader/copy-trading pattern the bounty lists as the
  *adversary's* dream and what a naive "privacy" scheme leaks.)

The adversary is the **earliest-executor** heuristic: name the wallet that
executed first as the initiator. This is exactly what a timing-based
deanonymizer does. We measure **attribution accuracy** — the fraction of rounds
in which it names the true initiator — against the `1/N` random-guess baseline.

## Result

Crowd size `N = 50`, `5000` rounds. Random-guess baseline is `1/N = 0.0200`.

| Behavior | Attribution accuracy |
|---|---:|
| Naive copy-trading | **1.0000** |
| **mirror-pool** | **0.0198** |

The identical heuristic that names the initiator **100%** of the time under
copy-trading drops to **~`1/N`** — statistically indistinguishable from random
guessing — under mirror-pool. The ordering signal that leaks the initiator is
erased by the uniform, synchronized jitter.

## Reproduce

```bash
cargo run  -p mp-eval    # prints the comparison report
cargo test -p mp-eval    # asserts mirror-pool ~ 1/N and naive ~ 1.0
```

The tests fail if mirror-pool's attribution ever rises meaningfully above random,
so this property is guarded in CI.

## Scope & honesty

This isolates **timing / execution-ordering** attribution — the signal
copy-trading and leader/follower schemes leak, and the one mirror-pool's
synchronized rounds target directly. A real adversary combines signals; the
design addresses the others out of band (see [`DESIGN.md`](DESIGN.md) §9):

- **amount** fingerprinting → fixed denominations (all executions identical in
  size);
- **fee-payer** linkage → the proposer never pays (relayer), and the initiator's
  proposal is a zero-knowledge proof;
- **thin crowds** → the collective threshold aborts the round so no one is ever
  exposed in a small set;
- **cross-round** correlation → per-round-uncorrelated nullifiers and rotating
  participation.

What this harness does *not* claim: it is a model, not a live-mainnet study, and
the naive baseline is a clean copy-trading caricature. Its point is precise and
verifiable — mirror-pool removes the ordering signal that the most direct timing
adversary relies on, and it does so by construction, measured here rather than
asserted.
