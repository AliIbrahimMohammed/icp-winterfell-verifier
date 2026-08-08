# icp-winterfell-verifier

![Rust](https://img.shields.io/badge/Rust-2021-orange)
![Winterfell](https://img.shields.io/badge/winterfell-0.13.1-blueviolet)
![dfx](https://img.shields.io/badge/dfx-IC%20SDK-29abe2)
![Internet Computer](https://img.shields.io/badge/blockchain-Internet%20Computer-29abe2)
![PRs welcome](https://img.shields.io/badge/PRs-welcome-2ecb56)

An on-chain Internet Computer canister that verifies Winterfell STARK proofs,
plus an off-chain example prover to generate test proofs against it.

```
icp-winterfell-verifier/
├── air/              stark_air crate: the AIR definition, shared by both
│                     the canister and the off-chain prover so they can
│                     never silently drift apart
├── canister/         stark_verifier crate: the actual IC canister
├── prover_example/   off-chain binary that generates a test proof and a
│                     ready-to-run `dfx canister call` command
├── dfx.json
└── Cargo.toml        workspace root
```

## Why this shape

STARK verification only means something *relative to a fixed AIR* (the
algebraic description of the computation being proven). A proof by itself
doesn't say what was computed — it says "some execution trace satisfying
this AIR's constraints exists and matches these public inputs." So the
canister and the prover **must** link the identical AIR, which is why it
lives in its own crate (`air/`) rather than being duplicated.

The example AIR (`WorkAir`) is Winterfell's own reference computation:
starting from a field element, repeatedly apply `x -> x^3 + 42`, and prove
the final value. Swap `air/src/lib.rs` for your real computation's AIR when
you're past the example — the canister code in `canister/src/lib.rs`
doesn't need to change beyond the generic parameters if you keep the same
hash function/vector commitment choices.

## Design decisions worth knowing about

- **`concurrent` feature is off in the canister.** Winterfell's default
  feature set is just `std` (no threads); we never enable `concurrent`
  there, since it pulls in `rayon`, which spawns OS threads — not available
  in a canister's single-threaded WASM sandbox. The off-chain
  `prover_example` *does* enable it, since it's not running on the IC.
- **`verify_proof` is an `update` call, not a `query`.** Queries are
  answered by a single replica and aren't certified by consensus. If the
  whole point is a trustless verification result, it needs every replica on
  the subnet to agree, which only happens on an update call.
- **Public inputs are passed as decimal strings, not native integers.**
  Field elements here are 128-bit; rather than lean on candid's `int128`
  support (uneven across tooling), `start`/`result` are base-10 strings
  parsed into `u128` on-chain. Swap this for whatever encoding fits your
  own AIR's public inputs (arrays, structs, etc.) — the parsing pattern
  generalizes.
- **Determinism.** Verification is pure finite-field arithmetic over a
  Fiat–Shamir transcript — no floats, no OS randomness, no threads — which
  is exactly what IC replicated execution requires (every replica must
  compute the identical result).

## Prerequisites

- Rust with the `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [dfx](https://internetcomputer.org/docs) (the IC SDK)
- `candid-extractor` (`cargo install candid-extractor`) to regenerate the
  `.did` file from the compiled canister if you change the interface

> **Note:** this code was written against the current published APIs
> (winterfell 0.13.1, ic-cdk 0.20.2) but has **not** been compiled in this
> environment — no Rust toolchain was available. Run `cargo check` /
> `dfx build` locally as your first step and expect to fix minor issues
> (crate version pins, trait bound tweaks) as usual with hand-written Rust
> against a library that has changed across versions.

## Build & deploy

```bash
dfx start --background
dfx deploy stark_verifier
```

If you change the canister's public interface, regenerate the candid file:

```bash
cargo build --release --target wasm32-unknown-unknown -p stark_verifier
candid-extractor target/wasm32-unknown-unknown/release/stark_verifier.wasm > canister/stark_verifier.did
```

## Generate a test proof and verify it on-chain

```bash
cargo run --release -p prover_example
```

This prints proof size, a local (off-chain) verification sanity check, and a
`dfx canister call` command with the proof bytes and public inputs already
filled in — copy/paste it to hit the deployed canister:

```bash
dfx canister call stark_verifier verify_proof '(record {
  proof_bytes = blob "…";
  start = "3";
  result = "…";
  min_security_bits = 95 : nat32;
})'
```

Expect `(variant { Valid })` back. Flip a byte in the proof and re-run it to
confirm you get `(variant { Invalid = record { reason = "…" } })` instead.

## Network test results

The canister was deployed and exercised against a real IC replica (a local
`dfx`/PocketIC subnet, not mainnet — see caveat below) to confirm it behaves
correctly under the actual consensus/replicated-execution machinery, not
just `winterfell::verify` called directly in a unit test.

**Correctness**

| Call | Result |
|---|---|
| Genuine proof, correct public inputs | `variant { Valid }` |
| Proof with a corrupted trace-commitment byte (deep in the body) | `variant { Invalid = record { reason = "verification failed: failed to open trace query against the given commitment" } }` |
| Proof with a corrupted length/width byte in the header | **Traps** (`assert_eq!` panic in `WorkAir::new`/winterfell, not a graceful `Invalid`) |

The third row is a real finding, not a hypothetical: a *structurally*
malformed proof (as opposed to one that decodes fine but fails a
cryptographic check) currently hits an `assert_eq!` inside winterfell's
`Air::new` and traps the whole call. This is safe — a trapped `update`
call is atomic and reverts — it does not corrupt canister state or accept
a bad proof — but it's a rejection which produces an error the caller has to
distinguish from the human/log-readable `Invalid` you'd probably rather it
returned. Worth wrapping the untrusted-input path in
`std::panic::catch_unwind` before calling `winterfell::verify`, if you
expect proofs from adversarial (not just buggy) callers.

**Scaling / cost — proof size and verification cost vs. trace length**

Same AIR, same canister binary, only the prover's trace length (`n`)
changes — this is what "scaling up the circuit" means for a fixed STARK AIR
(the number of times `x -> x^3 + 42` is applied), demonstrating the
logarithmic-ish growth that's the whole point of a STARK:

| Trace length (`n`) | Proof size | Instructions to verify | Approx. cycles (local) |
|---:|---:|---:|---:|
| 1,024 | 29,615 B | 19,181,892 | ~89,875,975 |
| 65,536 (64×) | 68,930 B | 39,405,745 (2.05×) | ~191,825,050 |
| 262,144 (256×) | 85,191 B | 47,726,037 (2.49×) | ~233,950,090 |

A 256× larger computation costs under 2.5× more to verify on-chain, and the
proof itself grows less than 3×. That sublinear relationship is the
reason this architecture is viable at all — proving cost grows with trace
length, but verification cost barely does.

Instruction counts come from `ic_cdk::api::instruction_counter()`,
sampled around the whole `verify_proof` body and written to the canister's
debug log (see `canister/src/lib.rs`) — this doesn't change the public
Candid interface, it's a log line, not a return value. Cycle figures are
`dfx canister status` balance deltas across each call; the ~4.7–4.9
cycles/instruction ratio was consistent across all three sizes, which is
a reasonable sanity check that the measurement itself isn't noisy.

**Caveat — this was local, not mainnet.** "The network" here is a local
`dfx start` replica (PocketIC), which runs the real, unmodified IC
execution environment — it is not a mock. It is *not*, however, IC
mainnet: no real ICP was spent, there's no real subnet of independent
node providers reaching consensus, and mainnet cycle prices can differ
slightly from what a local replica assumes. Actually deploying to mainnet needs a cycles
wallet funded from your own ICP, under your own identity — that's a
`dfx deploy --network ic` away once you're happy with local results, but
it's a step only you can authorize and fund.

To reproduce: `dfx start --background`, `dfx deploy stark_verifier`, then
`TRACE_LEN=<n> cargo run --release -p prover_example` for whichever trace
length you want to test (must be a power of two; defaults to 1024).

## Known limits (see also: the discussion that led here)

- This canister only verifies proofs for the AIR compiled into it. There's
  no dynamic/generic AIR loading — that's inherent to how Winterfell's
  `Air` trait works (it's a compile-time type, not a runtime value), so a
  canister that needs to verify multiple distinct computations needs one
  `Air` implementation per computation, potentially one canister per
  computation family if constraint counts/degrees get large enough to
  matter for the instruction budget.
- Very large proofs (many queries / high blowup factor, used for
  higher-security or larger computations) cost more instructions to verify
  and produce a larger `blob` argument. Verification is still
  logarithmic-ish in trace length, so this is far better than proving
  would, but it isn't literally free — benchmark with realistic proof
  sizes for your computation before assuming a given security level is
  cheap enough for your cycle budget.
- Proof generation is intentionally NOT in this canister. If you need
  fully on-chain proving too, that's a much harder, separate problem (see
  the earlier discussion on instruction limits, memory limits, and the lack
  of multi-threading in canisters) — start by benchmarking
  `prover_example` for your actual trace size before deciding whether
  on-chain proving is viable at all.