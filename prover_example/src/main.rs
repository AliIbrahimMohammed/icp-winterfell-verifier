//! Off-chain reference prover for the `stark_air::WorkAir` computation.
//!
//! This is NOT part of the canister and does not run on the IC. It exists so
//! you have something to generate a real proof with, to test the on-chain
//! verifier end-to-end. In a real deployment this logic lives wherever you
//! choose to run proving (your own server, a client machine, etc.) -- see
//! the top-level README for why proof generation is a poor fit for
//! on-chain execution.
//!
//! Run with: `cargo run --release -p prover_example`
//! It prints the public inputs and a ready-to-paste `dfx canister call`
//! command you can run against a deployed `stark_verifier` canister.

use stark_air::{HashFn, PublicInputs, RandCoin, WorkAir, VC};
use winterfell::{
    crypto::MerkleTree,
    math::{fields::f128::BaseElement, FieldElement},
    matrix::ColMatrix,
    AuxRandElements, BatchingMethod, CompositionPoly, CompositionPolyTrace,
    ConstraintCompositionCoefficients, DefaultConstraintCommitment, DefaultConstraintEvaluator,
    DefaultTraceLde, FieldExtension, PartitionOptions, Proof, ProofOptions, Prover, StarkDomain,
    Trace, TraceInfo, TracePolyTable, TraceTable,
};

/// Builds the execution trace for `x -> x^3 + 42`, run `n` times starting
/// from `start`. Must match `WorkAir`'s expectations exactly (single column,
/// same transition function).
fn build_do_work_trace(start: BaseElement, n: usize) -> TraceTable<BaseElement> {
    let trace_width = 1;
    let mut trace = TraceTable::new(trace_width, n);
    trace.fill(
        |state| {
            state[0] = start;
        },
        |_, state| {
            state[0] = state[0].exp(3u32.into()) + BaseElement::new(42);
        },
    );
    trace
}

struct WorkProver {
    options: ProofOptions,
}

impl WorkProver {
    fn new(options: ProofOptions) -> Self {
        Self { options }
    }
}

impl Prover for WorkProver {
    type BaseField = BaseElement;
    type Air = WorkAir;
    type Trace = TraceTable<Self::BaseField>;
    type HashFn = HashFn;
    type VC = VC;
    type RandomCoin = RandCoin;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, Self::Air, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> PublicInputs {
        let last_step = trace.length() - 1;
        PublicInputs {
            start: trace.get(0, 0),
            result: trace.get(0, last_step),
        }
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_trace_lde<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        trace_info: &TraceInfo,
        main_trace: &ColMatrix<Self::BaseField>,
        domain: &StarkDomain<Self::BaseField>,
        partition_option: PartitionOptions,
    ) -> (Self::TraceLde<E>, TracePolyTable<E>) {
        DefaultTraceLde::new(trace_info, main_trace, domain, partition_option)
    }

    fn build_constraint_commitment<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        composition_poly_trace: CompositionPolyTrace<E>,
        num_constraint_composition_columns: usize,
        domain: &StarkDomain<Self::BaseField>,
        partition_options: PartitionOptions,
    ) -> (Self::ConstraintCommitment<E>, CompositionPoly<E>) {
        DefaultConstraintCommitment::new(
            composition_poly_trace,
            num_constraint_composition_columns,
            domain,
            partition_options,
        )
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        air: &'a Self::Air,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

fn main() {
    // Trace length (must be a power of 2). Overridable via TRACE_LEN so the
    // same binary can generate both the small demo proof and much larger
    // proofs for scale/benchmark testing, e.g.:
    //   TRACE_LEN=65536 cargo run --release -p prover_example
    let start = BaseElement::new(3);
    let n: usize = std::env::var("TRACE_LEN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024);
    assert!(n.is_power_of_two(), "TRACE_LEN must be a power of two");

    let trace = build_do_work_trace(start, n);
    let result = trace.get(0, n - 1);

    // ~96-bit conjectured security. Increase num_queries / blowup_factor for
    // stronger (larger, slower) proofs.
    let options = ProofOptions::new(
        32,   // number of queries
        8,    // blowup factor
        0,    // grinding factor
        FieldExtension::None,
        8,    // FRI folding factor
        31,   // FRI max remainder polynomial degree
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    );

    let prover = WorkProver::new(options);
    let proof: Proof = prover.prove(trace).expect("proof generation failed");

    let proof_bytes = proof.to_bytes();
    let hex: String = proof_bytes.iter().map(|b| format!("{b:02x}")).collect();

    println!("start  = {start}");
    println!("result = {result}");
    println!("proof size: {} bytes", proof_bytes.len());
    println!();
    println!("Sanity check against the same AIR, off-chain:");
    let min_opts = winterfell::AcceptableOptions::MinConjecturedSecurity(95);
    let pub_inputs = PublicInputs { start, result };
    match winterfell::verify::<WorkAir, HashFn, RandCoin, MerkleTree<HashFn>>(
        proof.clone(),
        pub_inputs,
        &min_opts,
    ) {
        Ok(()) => println!("  local verify: OK"),
        Err(e) => println!("  local verify FAILED: {e}"),
    }

    println!();
    println!("To test the deployed canister, run:");
    println!();
    println!(
        "dfx canister call stark_verifier verify_proof '(record {{ proof_bytes = blob \"{}\"; start = \"{}\"; result = \"{}\"; min_security_bits = 95 : nat32 }})'",
        hex_to_candid_blob(&hex),
        start,
        result
    );
}

/// Candid blob literals use `\XX` escapes per byte rather than a bare hex
/// string -- this reformats accordingly.
fn hex_to_candid_blob(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|c| format!("\\{}", std::str::from_utf8(c).unwrap()))
        .collect()
}
