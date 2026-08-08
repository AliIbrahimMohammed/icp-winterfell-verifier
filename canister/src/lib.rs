//! On-chain STARK proof verifier canister.
//!
//! Exposes a single meaningful method, `verify_proof`, which decodes a
//! Winterfell `Proof` and runs it through `winterfell::verify` against a
//! fixed AIR (see `air.rs`) and caller-supplied public inputs. Because
//! verification is pure finite-field arithmetic over a Fiat-Shamir
//! transcript (no floats, no OS randomness, no threads), it is deterministic
//! and safe to run as a replicated `update` call, so the result is backed by
//! subnet consensus rather than a single untrusted replica.
//!
//! This canister deliberately does NOT generate proofs. Proof generation
//! belongs off-chain (or in a separate, non-consensus-critical canister) --
//! see the accompanying README for why.

use candid::CandidType;
use serde::Deserialize;
use stark_air::{HashFn, PublicInputs, RandCoin, WorkAir, VC};
use winterfell::{math::fields::f128::BaseElement, AcceptableOptions, Proof};

#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct VerifyRequest {
    /// Serialized Winterfell `Proof`, produced on the prover side via
    /// `Proof::to_bytes()`.
    pub proof_bytes: Vec<u8>,

    /// Public input: starting value of the computation, as a base-10 string
    /// (field elements can exceed 64 bits, so we avoid relying on candid's
    /// int128/nat128 support and parse a decimal string instead).
    pub start: String,

    /// Public input: claimed final value of the computation, same encoding.
    pub result: String,

    /// Minimum conjectured security level (in bits) a proof must meet to be
    /// accepted. 95-100 is a reasonable default; raise it for higher-value
    /// use cases at the cost of larger, more expensive-to-verify proofs.
    pub min_security_bits: u32,
}

#[derive(CandidType, Deserialize, Clone, Debug)]
pub enum VerifyResult {
    Valid,
    Invalid { reason: String },
}

/// Verifies a Winterfell STARK proof on-chain.
///
/// Declared as an `update` call (not `query`) on purpose: query calls are
/// answered by a single replica and are not certified by consensus, which
/// defeats the point of putting verification on a blockchain in the first
/// place. An update call forces every replica on the subnet to independently
/// re-run verification and agree on the result.
#[ic_cdk::update]
fn verify_proof(req: VerifyRequest) -> VerifyResult {
    // Instruction counter is sampled around the whole call so operators can
    // observe how verification cost scales with proof size / trace length.
    // Purely diagnostic: it's written to the canister's debug log, not
    // returned to the caller, so it doesn't affect the public interface.
    let start_instructions = ic_cdk::api::instruction_counter();
    let proof_len = req.proof_bytes.len();
    let result = verify_proof_inner(req);
    let used = ic_cdk::api::instruction_counter().saturating_sub(start_instructions);
    ic_cdk::println!(
        "verify_proof: proof_bytes={proof_len}B instructions={used}"
    );
    result
}

fn verify_proof_inner(req: VerifyRequest) -> VerifyResult {
    let proof = match Proof::from_bytes(&req.proof_bytes) {
        Ok(p) => p,
        Err(e) => {
            return VerifyResult::Invalid {
                reason: format!("failed to decode proof bytes: {e}"),
            }
        }
    };

    let start = match parse_base_element(&req.start) {
        Ok(v) => v,
        Err(e) => {
            return VerifyResult::Invalid {
                reason: format!("invalid `start` public input: {e}"),
            }
        }
    };
    let result = match parse_base_element(&req.result) {
        Ok(v) => v,
        Err(e) => {
            return VerifyResult::Invalid {
                reason: format!("invalid `result` public input: {e}"),
            }
        }
    };

    let pub_inputs = PublicInputs { start, result };
    let acceptable_options = AcceptableOptions::MinConjecturedSecurity(req.min_security_bits);

    match winterfell::verify::<WorkAir, HashFn, RandCoin, VC>(proof, pub_inputs, &acceptable_options)
    {
        Ok(()) => VerifyResult::Valid,
        Err(e) => VerifyResult::Invalid {
            reason: format!("verification failed: {e}"),
        },
    }
}

/// Cheap liveness/readiness check. Safe as a `query` since it touches no
/// proof data and needs no consensus guarantee.
#[ic_cdk::query]
fn health() -> String {
    "stark_verifier canister is running".to_string()
}

fn parse_base_element(s: &str) -> Result<BaseElement, String> {
    let n: u128 = s
        .trim()
        .parse()
        .map_err(|_| format!("could not parse '{s}' as a u128 decimal integer"))?;
    Ok(BaseElement::new(n))
}

ic_cdk::export_candid!();
