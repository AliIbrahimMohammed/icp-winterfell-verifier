//! Algebraic Intermediate Representation (AIR) for the computation this
//! canister knows how to verify.
//!
//! A STARK verifier is only meaningful *with respect to a specific AIR* --
//! the proof itself does not describe what was computed, only that *some*
//! trace satisfying the AIR's constraints exists and is consistent with the
//! public inputs. So this file is the single most important piece of the
//! canister: whatever computation you actually care about, its AIR belongs
//! here, and it MUST be byte-for-byte identical to the AIR used by whatever
//! prover generated the proof.
//!
//! For a concrete, runnable example we use Winterfell's own reference
//! computation: starting from a field element `start`, repeatedly apply
//! `x -> x^3 + 42` for the number of steps encoded in the trace, and prove
//! the final value equals `result`. Swap this module out for your own AIR
//! (implementing the same `Air` trait) when you move past the example.

use winterfell::{
    crypto::{hashers::Blake3_256, DefaultRandomCoin, MerkleTree},
    math::{fields::f128::BaseElement, FieldElement, ToElements},
    Air, AirContext, Assertion, EvaluationFrame, ProofOptions, TraceInfo,
    TransitionConstraintDegree,
};

/// Hash function used by the prover. Must match whatever the prover used --
/// this is one of the "shared secret" parameters between prover and verifier.
pub type HashFn = Blake3_256<BaseElement>;

/// Randomness source for the Fiat-Shamir transform. Deterministic given the
/// transcript, so it's safe for replicated (consensus) execution.
pub type RandCoin = DefaultRandomCoin<HashFn>;

/// Vector-commitment scheme backing the trace/constraint commitments.
pub type VC = MerkleTree<HashFn>;

/// Public inputs: values that both the prover and verifier must agree on,
/// and which tie a specific proof to a specific claim ("computation starting
/// at `start` produced `result`").
#[derive(Clone)]
pub struct PublicInputs {
    pub start: BaseElement,
    pub result: BaseElement,
}

impl ToElements<BaseElement> for PublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        vec![self.start, self.result]
    }
}

pub struct WorkAir {
    context: AirContext<BaseElement>,
    start: BaseElement,
    result: BaseElement,
}

impl Air for WorkAir {
    type BaseField = BaseElement;
    type PublicInputs = PublicInputs;

    fn new(trace_info: TraceInfo, pub_inputs: PublicInputs, options: ProofOptions) -> Self {
        assert_eq!(1, trace_info.width(), "trace must have exactly one column");

        // One transition constraint (the cubing step), of degree 3.
        let degrees = vec![TransitionConstraintDegree::new(3)];

        // Two boundary assertions: value at step 0 and value at the last step.
        let num_assertions = 2;

        WorkAir {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            start: pub_inputs.start,
            result: pub_inputs.result,
        }
    }

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        let current_state = frame.current()[0];
        let next_state = current_state.exp(3u32.into()) + E::from(BaseElement::new(42));
        result[0] = frame.next()[0] - next_state;
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last_step = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, self.start),
            Assertion::single(0, last_step, self.result),
        ]
    }
}
