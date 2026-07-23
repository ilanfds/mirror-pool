# mirror-pool

A **synchronized behavioral mixer** for Solana — a crowd-sourced anonymity set
for *behavior*, not funds.

Many independent wallets perform the **same standardized action inside the same
synchronized time window**, so an observer can see *that* an action happened but
cannot determine *which participant genuinely wanted it* nor *who caused the
crowd to form around it*. It ports the Tornado Cash architecture (commitments,
Merkle membership set, nullifiers, relayers) from *hiding which deposit is
withdrawn* to *hiding who originated a behavioral pattern* — non-custodial, in
Rust, with zero-knowledge proofs verified on-chain.

> ⚠️ **Experimental.** Ships with development trusted-setup keys. Not for
> production use until the release gates in [`docs/ROADMAP.md`](docs/ROADMAP.md)
> §5 are met (trusted-setup ceremony, audit, keeper decentralization).

## Documentation

- **[docs/DESIGN.md](docs/DESIGN.md)** — architecture / whitepaper.
- **[docs/ROADMAP.md](docs/ROADMAP.md)** — phased implementation plan.

## Workspace layout

```
crates/
  mp-crypto     Poseidon, incremental Merkle tree, notes & nullifiers  (pure Rust)
  mp-proof      Groth16 proving (arkworks)
  mp-agent      participant: keygen, durable-nonce pre-sign, round agent
  mp-relayer    trust-minimized proposal submitter
  mp-keeper     trust-minimized execution synchronizer
programs/       on-chain Anchor program            (added in ROADMAP phase 2)
circuits/       R1CS proposal circuit + Groth16     (added in ROADMAP phase 3)
docs/           design & roadmap
```

## Build

```bash
cargo build --workspace
cargo test  --workspace
```

## License

MIT — see [LICENSE](LICENSE).
