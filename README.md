# icp-winterfell-verifier

On-chain [Winterfell](https://github.com/0xPolygonMiden/winterfell) STARK proof verification on the Internet Computer Protocol (ICP), shipped as a canister, together with an off-chain example prover for end-to-end testing.

This project proves the correctness of a computation on the ICP without anyone re-executing it. Because verification runs as a deterministic, consensus-certified `update` call, every replica on the subnet independently agrees on the result — a genuinely trustless on-chain verifier.

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [How It Works](#how-it-works)
- [Getting Started](#getting-started)
- [Build & Deploy](#build--deploy)
- [Generating Proofs & Verifying On-Chain](#generating-proofs--verifying-on-chain)
- [Candid Interface](#candid-interface)
- [Benchmarks](#benchmarks)
- [Security Considerations](#security-considerations)
- [Known Limitations](#known-limitations)
- [Project Layout](#project-layout)
- [References](#references)

## Features

- **Consensus-backed verification** — `verify_proof` is an `update` call, so the result is certified by subnet consensus rather than a single untrusted replica.
- **Deterministic by construction** — pure finite-field arithmetic over a Fiat–Shamir transcript: no floats, no OS randomness, no threads, so replicated execution always produces identical results.
- **Shared AIR crate** — the Algebraic Intermediate Representation (AIR) lives in its own crate, linked byte-for-byte by both the prover and the canister, eliminating the risk of silent prover/verifier drift.
- **WASM-sandbox friendly** — the canister deliberately avoids Winterfell's `concurrent` feature (rayon OS threads are unavailable in a canister's single-threaded WASM sandbox); multi-threaded proving stays off-chain.
- **Benchmarked** — verified against a live (local PocketIC) replica, with instruction/cycle measurements across three trace lengths.

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                     icp-winterfell-verifier                    │
│                                                                │
│  ┌───────────────────┐        ┌────────────────────────────┐  │
│  │   air/ (stark_air)│        │ canister/ (stark_verifier) │  │
│  │   AIR definition  │◄──────►│ IC canister: verifies      │  │
│  │  (constraint      │  ONE   │ Proof::from_bytes +        │  │
│  │   system for the  │  crate │ winterfell::verify         │  │
│  │   computation)    │        │                            │  │
│  └─────────┬─────────┘        └──────────────┬─────────────┘  │
│            │                                │                │
│  ┌─────────▼─────────┐          ┌───────────▼─────────────┐   │
│  │ prover_example/   │  proof  │  dfx canister call       │   │
│  │ off-chain proving │─────────►  stark_verifier          │   │
│  └───────────────────┘ + inputs│  verify_proof            │   │
│                               └──────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
```

**Why a shared AIR?** A STARK proof does not describe what was computed — it attests that *some* execution trace satisfying the AIR's constraints exists and matches the public inputs. The verifier therefore only means something *relative to a fixed AIR*. Because `stark_air` is compiled into both the prover and the canister, the two can never silently disagree about the computation's definition.

**The example computation.** The default `WorkAir` is Winterfell's reference workload: starting from a field element `start`, repeatedly apply `x → x³ + 42` for the length of the trace, and prove the final value equals `result`. Swapping in your own computation is as simple as replacing `air/src/lib.rs` with your own `Air` implementation; `canister/src/lib.rs` does not change beyond the type parameter.

## How It Works

1. An off-chain prover executes the computation, records the execution trace, and runs Winterfell's STARK proving protocol to produce a compact `Proof` (proof size grows logarithmically-ish with trace length).
2. The serialized proof is submitted to the canister's `verify_proof` update call with the public inputs (`start`, `result`) and a minimum conjectured security level.
3. Every replica deterministically decodes the proof, re-derives the Fiat–Shamir transcript, evaluates the AIR transition constraints, and checks the proof against the Merkle-tree commitments.
4. `Valid` or `Invalid { reason }` is returned — a result agreed upon by the whole subnet, not asserted by one machine.

## Getting Started

### Prerequisites

- Rust toolchain with WASM target:
  ```
  rustup target add wasm32-unknown-unknown
  ```
- The [DFINITY SDK (`dfx`)](https://internetcomputer.org/docs/building-apps/getting-started/install).
- `candid-extractor` (to regenerate Candid if the interface changes):
  ```
  cargo install candid-extractor
  ```

> **Note:** this code was written against Winterfell 0.13.1 and ic-cdk 0.20.2. Minor API changes across released versions may require small pin adjustments; run `cargo check` / `dfx build` as your first step.

## Build & Deploy

```bash
dfx start --background
dfx deploy stark_verifier
```

If you have the Rust toolchain plus the `wasm32-unknown-unknown` target, `dfx deploy` compiles the canister itself. Otherwise a prebuilt WASM can be installed with:

```bash
dfx canister create stark_verifier
dfx canister install stark_verifier --wasm artifacts/stark_verifier.wasm --mode install
```

Regenerating the Candid file after an interface change:

```bash
cargo build --release --target wasm32-unknown-unknown -p stark_verifier
candid-extractor target/wasm32-unknown-unknown/release/stark_verifier.wasm > canister/stark_verifier.did
```

## Generating Proofs & Verifying On-Chain

Generate a test proof for the default trace length (1,024 steps):

```bash
cargo run --release -p prover_example
```

This prints the proof size, a local (off-chain) verification sanity check, and a ready-to-paste `dfx canister call` command with the proof bytes and public inputs already filled in. Run it against the deployed canister:

```bash
dfx canister call stark_verifier verify_proof '(record {
  proof_bytes = blob "...";
  start = "3";
  result = "...";
  min_security_bits = 95 : nat32;
})'
```

Expect `(variant { Valid })`. Corrupt a byte in the proof and re-run to confirm you get `(variant { Invalid = record { reason = "..." } })` instead.

### Scaling the trace length

The prover reads `TRACE_LEN` (a power of two, defaults to 1,024) so you can probe how verification cost scales:

```bash
TRACE_LEN=65536  cargo run --release -p prover_example
TRACE_LEN=262144 cargo run --release -p prover_example
```

For large proofs the Candid argument may exceed shell limits; use `--argument-file` with a file containing just the `(record { ... })` argument.

## Candid Interface

<!-- Generated from canister/src/lib.rs; regenerate via candid-extractor if edited. -->

```candid
type VerifyRequest = record {
  proof_bytes         : blob;
  start               : text;  // base-10 decimal string
  result              : text;  // base-10 decimal string
  min_security_bits   : nat32;
};
type VerifyResult = variant {
  Valid;
  Invalid : record { reason : text };
};
service : {
  verify_proof : (VerifyRequest) -> (VerifyResult);
  health       : () -> (text) query;
};
```

Public inputs are passed as base-10 decimal strings rather than native integers because field elements are 128-bit and Candid integer tooling is uneven across languages; the parsing pattern generalizes to any AIR public-input encoding you choose.

## Benchmarks

Same AIR, same canister binary — only the prover's trace length changes. This shows the logarithmic-ish growth that is the point of a STARK:

| Trace length (`n`) | Proof size | Instructions to verify | Approx. cycles (local) |
|---:|---:|---:|---:|
| 1,024   | 29,615 B | 19,181,892 | ~89,875,975 |
| 65,536  | 68,930 B | 39,405,745 (2.05×) | ~191,825,050 |
| 262,144 | 85,191 B | 47,726,037 (2.49×) | ~233,950,090 |

A 256× larger computation costs under 2.5× more to verify on-chain; proof size grows less than 3×. Instruction counts are sampled with `ic_cdk::api::instruction_counter()` and written to the canister's debug log (`dfx canister logs stark_verifier`); cycle figures are `dfx canister status` balance deltas. Measured on a local Pocket replica, not mainnet. See `NETWORK_TESTING.md` for the replayable procedure.

## Security Considerations

- **Update calls trap safely.** A structurally malformed proof (as opposed to one that decodes fine but fails a cryptographic check) can currently reach an `assert_eq!` inside Winterfell's `Air::new` and trap the whole call. A trapped `update` is atomic — state is not corrupted and the proof is not accepted — but the caller sees an error instead of a graceful `Invalid`. Wrap the untrusted-input path in `std::panic::catch_unwind` if you expect adversarial (not merely buggy) callers.
- **Security levels.** 95–100 bits of conjectured security is a reasonable default. Raise `min_security_bits` for higher-value use cases; it costs larger, more expensive-to-verify proofs.
- **Determinism is load-bearing.** Any nondeterminism in verification would break replicated execution; the dependency stack was chosen around this constraint.

## Known Limitations

- This canister verifies proofs only for the AIR compiled into it. Winterfell's `Air` is a compile-time trait, not a runtime value, so multiple computations means one `Air` implementation each — potentially one canister per computation family if constraint counts/degrees grow large.
- Very large proofs (many queries / high blowup factor) cost more to verify and produce large Candid blobs. Verification scales sub-linearly with trace length, but it is not free — benchmark with realistic proofs for your cycle budget.
- Proof generation is intentionally not on-chain. Fully on-chain proving is a much harder problem (instruction limits, memory limits, no threading in canisters) — benchmark `prover_example` at your real trace size before deciding whether on-chain proving is viable at all.

## Project Layout

```
icp-winterfell-verifier/
├── air/                # stark_air crate: the AIR definition, shared by both
│                       # the canister and the off-chain prover
├── canister/           # stark_verifier crate: the IC canister + Candid file
├── prover_example/     # off-chain binary that generates test proofs
├── dfx.json            # dfx canister configuration
└── Cargo.toml          # workspace root
```

## References

- [Winterfell STARK library](https://github.com/0xPolygonMiden/winterfell) — proving/verification engine.
- [Arithmetization I: Algebraic Intermediate Representation](https://eprint.iacr.org/) — the AIR background.
- [Internet Computer docs](https://internetcomputer.com) — canister development, dfx, Candid.
- `NETWORK_TESTING.md` — the exact commands run against a live local replica, exact and replayable.