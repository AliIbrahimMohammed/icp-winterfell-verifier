# Network testing notes

This documents exactly what was run against a live IC replica (local
`dfx`/PocketIC, not mainnet) for this round of testing, so it's reproducible.

## Setup

```bash
dfx start --background
dfx canister create stark_verifier
dfx canister install stark_verifier --wasm artifacts/stark_verifier.wasm --mode install
dfx canister call stark_verifier health
```

(If you have a Rust toolchain with the `wasm32-unknown-unknown` target
installed — `rustup target add wasm32-unknown-unknown` — you can build the
wasm yourself instead of using the prebuilt one in `artifacts/`:
`dfx deploy stark_verifier`.)

## Generating proofs at different trace lengths

`prover_example` reads `TRACE_LEN` (must be a power of two, defaults to
1024) so you can regenerate the exact proofs used in these results, or go
bigger:

```bash
TRACE_LEN=1024   cargo run --release -p prover_example > proof_1024.txt
TRACE_LEN=65536  cargo run --release -p prover_example > proof_65536.txt
TRACE_LEN=262144 cargo run --release -p prover_example > proof_262144.txt
```

Each run prints a ready-to-paste `dfx canister call` command. For the
larger sizes the candid blob argument is too long for a shell command line
(`Argument list too long`); use `--argument-file` instead — pull just the
`(record { ... })` argument out of the printed command into its own file
and run:

```bash
dfx canister call stark_verifier verify_proof --argument-file argfile_n65536.txt
```

`artifacts/example_proofs/` has both the full printed output and the
pre-extracted argument files for the three sizes tested, so you can replay
these calls without regenerating proofs.

## Measuring cost

- **Instructions**: the canister logs `verify_proof: proof_bytes=<N>B
  instructions=<M>` via `ic_cdk::api::instruction_counter()` on every call
  (see `canister/src/lib.rs`). View with `dfx canister logs stark_verifier`.
- **Cycles**: `dfx canister status stark_verifier | grep Balance` before and
  after a call; the delta is the cycle cost of that call. On a local
  replica this is an approximation of the mainnet fee schedule, not a
  mainnet-verified figure.

## What wasn't done: mainnet

Deploying to IC mainnet (`dfx deploy --network ic`) needs a cycles wallet
funded from real ICP under your own identity. That step is yours to take
once you're satisfied with local results — nothing here does it for you,
and no mainnet canister was created or paid for as part of this work.
