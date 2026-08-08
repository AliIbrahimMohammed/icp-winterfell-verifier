# icp-winterfell-verifier

**A verifiable-computation canister for the Internet Computer — in plain words.**

This project lets you take a computation, produce a tiny cryptographic *proof* that the computation was done correctly, and have the Internet Computer **verify that proof by itself** — without re-running the computation and without trusting anyone.

It runs on the [Internet Computer Protocol (ICP)](https://internetcomputer.com), uses the [Winterfell STARK library](https://github.com/0xPolygonMiden/winterfell), and comes with everything you need to try it from zero: a prover, a canister, and every command spelled out.

---

## Table of Contents

1. [What Problem Does This Solve?](#what-problem-does-this-solve)
2. [Stark Knowledge, Quickly](#stark-knowledge-quickly)
3. [Anatomy of This Project](#anatomy-of-this-project)
4. [Before You Start (Prerequisites)](#before-you-start-prerequisites)
5. [Tutorial: Deploy — Prove — Verify](#tutorial-deploy--prove--verify)
   - [Step 0: Build the project](#step-0-build-the-project)
   - [Step 1: Start the local network](#step-1-start-the-local-network)
   - [Step 2: Deploy the canister](#step-2-deploy-the-canister)
   - [Step 3: Generate a proof](#step-3-generate-a-proof)
   - [Step 4: Verify it on-chain](#step-4-verify-it-on-chain)
   - [Step 5: Make it fail on purpose](#step-5-make-it-fail-on-purpose)
6. [What Is Happening Under the Hood](#what-is-happening-under-the-hood)
7. [Answers to Questions You Probably Have (FAQ)](#faq)
8. [Costs and Scaling in Human Words](#costs-and-scaling-in-human-words)
9. [Known Limits and Where to Go From Here](#known-limits-and-where-to-go-from-here)
10. [Further Reading](#further-reading)

---

## What Problem Does This Solve?

Imagine someone claims: *"I ran a very long computation, and here is the result."*

Why should you believe them? Normally you'd have to:

- re-run the computation yourself (expensive or impossible), **or**
- trust them (they could lie), **or**
- hire an auditor (trust + money).

A *STARK proof* replaces all of that. The claim comes with a small proof that is:
- **Small** — much smaller than the computation itself,
- **Fast to check** — checking is dramatically cheaper than computing,
- **Trustless** — checking requires no secret information and no trust in the prover.

Now put that check inside an Internet Computer canister. The canister `verify_proof` function runs on every replica of the subnet and the result is agreed on by **consensus** — it is not just one machine's opinion. That combination — *mathematically forced correctness* + *blockchain consensus* — is what this project demonstrates.

**The demo computation** (Winterfell's classic example): start with the number 3, apply `x → x³ + 42` (cube, then add 42) just over a thousand times, and claim the final value. In symbols: the sequence is `3, 3³+42, (3³+42)³+42, ...`. The canister never computes this — it just checks the proof.

---

## Stark Knowledge, Quickly

(If you already know STARKs, [skip ahead](#anatomy-of-this-project).)

| Term | Meaning — in everyday words |
|---|---|
| **Trace** | The spreadsheet of the entire computation — every step's value, row by row. |
| **AIR** | A set of *rules* (algebraic constraints) the trace must obey at every step — for us: "each row is exactly `previous³ + 42`". |
| **Prover** | The role that runs the computation legitimately, writes the trace, and does the heavy mathematics to produce a proof. |
| **Verifier** | The role that reads a proof + public inputs and says ONLY `Valid` or `Invalid`. The verifier does *not* replay the computation. |
| **Public inputs** | The things both sides agree on beforehand: e.g. `start = 3` and `result = whatever it was meant to be` (here: the two endpoints of the claim). |
| **Fiat–Shamir** | A trick to run the interactive "you give challenges, I answer" proof **non-interactively**: the challenges are hashes of the conversation so far (Blake3, for this project's config). Deterministic → every replica computes identical challenges → consensus-safe. |
| **Security bits** | How hard it is to forge the proof. ~95–100 bits on the consumer hardware scale is a common setting. Higher = stronger = larger/slower proofs. |
| **Trace length** | How many steps the computation ran. Bigger trace = longer prover work, but the **verifier stays fast** — that's the whole magic we measure below. |

The single most important sentence: **a STARK proof says "there exists some trace satisfying these rules with these public inputs" — the *rules* (the AIR) are not part of the proof and they are not optional.** If prover and verifier had different rule-sets in mind, everything silently breaks. That is why this project ships the AIR as a **shared library** — see below.

---

## Anatomy of This Project

```
icp-winterfell-verifier/
├── air/                (crate "stark_air")        ← the RULES (WorkAir)
│                        shared byte-for-byte by:
│                        • the off-chain prover
│                        • the on-chain verifier
├── canister/          (crate "stark_verifier")    ← the IC canister
│   src/lib.rs         the verify_proof update function
│   stark_verifier.did the Candid interface declaration
├── prover_example/    (binary) the off-chain prover + a helper
│   src/main.rs        that prints a copy-paste `dfx canister call`
├── dfx.json           tells `dfx` how to build/deploy the canister
├── Cargo.toml         Rust workspace root
└── NETWORK_TESTING.md tracking how the numbers below were measured
```

### Why is the AIR a separate library?

Re-read the sentence above: **rules must match on both sides, exactly — never two versions.** Winterfell's `Air` trait is a zero-virtual-cost compile-time type — it is not a config file. By putting the rules in `air/` and linking the *same crate* into both binaries, "prover and verifier disagree about the math" becomes a **host-linking error rather than a subtle runtime bug**. That isolation is the project's strongest design point.

---

## Before You Start (Prerequisites)

| Tool | Why | Install |
|---|---|---|
| Rust 1.82+ | ships the whole thing | [rustup.rs](https://rustup.rs) |
| `wasm32-unknown-unknown` target | compiles the canister to WebAssembly | `rustup target add wasm32-unknown-unknown` |
| [`dfx`](https://internetcomputer.org/docs/building-apps/getting-started/install) | IC SDK — starts a replica, deploys canisters | official ICP docs |
| `candid-extractor` | regenerates the Candid file if you change the interface | `cargo install candid-extractor` |

The whole demo can be run on your laptop against a **local replica** — no coins, no network, no fees. (Mainnet deployment is a separate step at the very end of this README; it costs real cycles.)

---

## Tutorial: Deploy — Prove — Verify

### Step 0: Build the project *(once)*

```bash
cargo build --release
```

If any library APIs shifted since this was written, fix pins (Winterfell 0.13.1 / ic-cdk 0.20.2) and rebuild — entirely normal for a Rust workspace against moving dependencies. `cargo check` is the fastest sanity loop after edits.

### Step 1: Start the local network

```bash
dfx start --background
```

This starts a **PocketIC**-flavored local replica — a real IC execution engine on your machine (exactly the same Candid/consensus machinery mainnet runs, minus the price).

### Step 2: Deploy the canister

```bash
dfx deploy stark_verifier
```

`dfx` will compile `canister/` to a `.wasm` file and install it. You now have a live canister bound to an address:

```bash
dfx canister id stark_verifier   # prints the canister_id
```

You can already check it's alive:

```bash
dfx canister call stark_verifier health
# → ("stark_verifier canister is running")
```

### Step 3: Generate a proof

```bash
cargo run --release -p prover_example
```

The prover script:

1. builds the 1024-row computation trace (starting at `3`),
2. runs Winterfell's `prove` — the actual heavy STARK math,
3. sanity‑checks the proof *locally* first, so any AIR mismatch shows up **before** you pay any cycles,
4. prints a complete ready-to-paste `dfx canister call` command.

### Step 4: Verify it on-chain

Copy the printed command into your terminal. It will look something like:

```bash
dfx canister call stark_verifier verify_proof '(record {
  proof_bytes = blob "\1b\22…";
  start = "3";
  result = "…";
  min_security_bits = 95 : nat32;
})'
```

You get the verdict:

```
(variant { Valid })
```

That's the entire loop: **you never computed `x³+42` on-chain; the subnet agreed the proof is correct.**

### Step 5: Make it fail on purpose

Edit the pasted command — change one byte inside the `blob "…"`, or set `result` to `"0"`. Then re-run. Now you see the rejection path:

```
(variant { Invalid = record { reason = "…" } })
```

A STARK verifier is only worth something if it can say `Invalid` loudly and correctly. Play with this: a wrong `result`, a corrupt blob, a lower `min_security_bits` than the proof was built with.

---

## What Is Happening Under the Hood

A *very rough* map of `verify_proof` (`canister/src/lib.rs`):

```
                                    ┌───────────────┐
   blob, start="3", result="…",min→ │ verify_proof   │
                                    └──────┬────────┘
                                           │
                        ① decode the bytes  │  Proof::from_bytes
                                           ▼
                                     ┌─────────────┐
                                     │  parsed Proof│
                                     └─────────────┘
                                           │
                        ② parse "3" → 128-bit number, and the result
                                           │   (base-10 → u128)
                        ③ hand (proof, inputs, min_security_bits)
                                           │  winterfell::verify
                                           ▼
                        ┌────────────────────────────────────┐
                        │  per-replica deterministic check:   │
                        │   • re-run Fiat–Shamir challenges   │
                        │   • evaluate AIR transition rules   │
                        │   • check Merkle commitment openings│
                        └────────────────────────────────────┘
                                           │
                          ◄ every replica computes the identical verdict ►
                                           ▼
                              (variant { Valid })   /   (variant { Invalid … })
```

**The single most important keyword in this file is `update`.** With `dfx canister call … .verify_proof` (an *update* call):

- the call is executed on *every replica* of the subnet,
- replicas must *agree* on the result and sign it,
- a lone malicious replica cannot produce a fake verdict result.

Queries, by contrast, are answered by *one* replica only — they would give you exactly what that one machine wants to say. For a trustless claim you want the `update` path. That's a deliberate trade (updates are slower + cost cycles; queries are instant).

**Determinism is load-bearing.** The verifier uses no floats, no OS randomness, no threads — the only randomness (challenges) comes from the Fiat–Shamir hash of the transcript itself, so every replica, in any time zone or temperature, computes the identical answer. This is a hard requirement for canister consensus, and Winterfell's verifier is engineered to satisfy it in `stdlib` mode. (Do **not** enable Winterfell's `concurrent` feature inside the canister: it pulls in `rayon`, which spawns real OS threads — the canister WebAssembly sandbox has no threads.)

**Public inputs as decimal strings:** the field elements here are 128-bit, larger than any single Candid integer that every tool and language handles uniformly. So the canister takes `start` and `result` as base-10 decimal strings, and parses them to `u128` internally (`parse_base_element` in `lib.rs`, with a human-readable error if you pass garbage like `"hello"`). This trick is simple and generalizes to any AIR public interface (arrays, structs…).

---

## FAQ

**Q: Can this canister prove my computation?**
The canister only *checks* proofs about the AIR that was compiled into it — `WorkAir`'s `x → x³ + 42` for now. Replacing it with your own computation means writing your own `Air` implementation in `air/src/lib.rs` (a small trait with exactly 3 methods). The canister code in `canister/src/lib.rs` does not need to change unless you add other public inputs.

**Q: Why isn't proving inside this canister?**
Proving is, in comparison, *very* computationally heavy: megabytes of trace, NTT-size polynomial arithmetic, lots of memory. A canister has a single-threaded WebAssembly sandbox with instruction limits. Proving is a better fit for a dedicated server / bare-metal / smartphone — and the *proof* is the only thing that needs to be on-chain.

**Q: What does `min_security_bits` mean to me?**
The floor you demand for accepting a proof: the proof's security level must be ≥ your number. If `95` is passed and the proof was generated with, say, only 60 bits of raw output, the verifier **rejects**. Typical handshake: prover picks its parameters (number of queries × blowup factor), verifier states its minimum, mismatch = invalid.

**Q: What about concurrency and determinism?**
Two distinct worries, both handled:
1. Threads: Winterfell's `concurrent` feature is gated out of the canister crate (the `rayon` hazard).
2. Nondeterminism: the verifier takes `&[u8]` input only — no mutable global state, no entropy, no floats; Candid and `u128` decimal parsing are fixed, OS-independent computations.

**Q: Proof too big for the terminal?**
Large traces (say 65,536 steps or above) can produce Candid blobs of tens of kilobytes — the shell's ARG_MAX will refuse. Use the file-based form:

```bash
dfx canister call stark_verifier verify_proof --argument-file argfile.txt
```

(`NETWORK_TESTING.md` walks through extracting the argument file from the prover's printed output.)

**Q: Where are the instruction/cycle numbers coming from?**
The canister logs `verify_proof: proof_bytes=<N>B instructions=<M>` on every call via `ic_cdk::api::instruction_counter()`, visible with `dfx canister logs stark_verifier`. Cycle figures are `dfx canister status stark_verifier` balance deltas across the call. This is exactly the measurement that produced the table below.

---

## Costs and Scaling (In Human Words)

STARK magic, made visible: keep the same canister binary, keep the same AIR — **grow only the trace length** (i.e., run `x→x³+42` a thousand vs. 262 thousand times), and watch what the verifier pays:

| Trace length (`n`)  | Proof size  | Verify instructions | Approx. cycles (local) |
|---|---:|---:|---:|
| 1,024   | 29,615 B   | ~19.2 M   | ~90 M  |
| 65,536  | 68,930 B   | ~39.4 M   | ~192 M  |
| 262,144 | 85,191 B   | ~47.7 M   | ~234 M  |

Read the rows as *relative* numbers: **a computation 256× bigger costs only ~2.5× more to verify on-chain** — proof size doesn't even triple. The prover (the part you pay off-line, in your own CPU time) does grow with the trace; the verifier (the part on-chain, where cycles are real money) barely does. That asymmetry is the business case for STARKs: **make giant computations cheap to check, and put each verdict behind subnet consensus.**

*(Measured on a local PocketIC replica — the real IC execution environment. Absolute cycle prices can differ slightly on mainnet; the behavior itself is identical.)*

---

## Known Limits and Where to Go From Here

1. **One AIR per canister.** Winterfell's `Air` is a compile-time type, not a runtime selectable. Verify a second computation → second canister (or a routing parent). That's a feature of security: no dynamic code-loading anywhere near a verifier.
2. **Malformed-proof edge case.** A proof that decodes okay but is structurally wrong (e.g., wrong trace width) can hit an `assert!` inside Winterfell before the CRYPTOGRAPHY has any say — literally: the canister *traps* (an update-call abort, atomic; state unaffected). It is safe but not a pretty rejected-variant. Hardening idea: wrap the untrusted-path `winterfell::verify` in `std::panic::catch_unwind` and return a graceful `Invalid`.
3. **On-chain proving is out of scope** (see FAQ). If you dream of fully on-chain proving, benchmark the off-chain prover at your real trace size first — the numbers will bring you to your senses honestly.
4. **Mainnet deployment is on you.** Everything above was *local*. Going to mainnet (`dfx deploy --network ic`) requires funding a wallet with real cycles under your own identity — the single action this repo cannot and will not do for you.

---

## Learning More

- [Winterfell source + docs](https://github.com/0xPolygonMiden/winterfell) — the STARK engine this project builds on.
- [STARK Anatomy](https://aszner.github.io/stark-anatomy/) — a readable, step-by-step explanation of how a STARK is built and checked.
- [Internet Computer developer docs](https://internetcomputer.org/docs) — canisters, Candid, `dfx` from scratch.
- [Arithmetization I: Algebraic Intermediate Representation](https://eprint.iacr.org/2019/946) — the paper behind the AIR design.
- `NETWORK_TESTING.md` in this repo — every command that produced the numbers above, exactly as you could re-run them.

---

*“The Internet Computer believes in your proof because it does not have to — it can check it. Checking is cheaper than trusting, and that's the entire point.”*