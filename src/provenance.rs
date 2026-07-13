//! Provenance semiring types — the algebraic foundation.
//!
//! See TIA-ARCH-001 through TIA-ARCH-003, and Appendix A of the SRS.

use std::fmt;

/// A semiring value in one of the supported concrete semirings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "semiring")]
pub enum SemiringValue {
    /// Boolean: affected or not
    Boolean(bool),
    /// Viterbi: confidence in [0,1]
    Confidence(f64),
    /// Tropical: distance as hop count
    Distance(u32),
    /// Provenance (polynomial): the master semiring
    Provenance(String),
    /// Cost/expected duration in ms
    Cost(u64),
}

/// Marker trait for semiring types.
pub trait ProvenanceSemiring: Clone + fmt::Debug {
    type Element: Clone + fmt::Debug;
    fn zero() -> Self::Element;
    fn one() -> Self::Element;
    fn add(a: &Self::Element, b: &Self::Element) -> Self::Element;
    fn mul(a: &Self::Element, b: &Self::Element) -> Self::Element;
}

/// Boolean semiring (∧, ∨, false, true)
#[derive(Debug, Clone)]
pub struct BooleanSemiring;

impl ProvenanceSemiring for BooleanSemiring {
    type Element = bool;
    fn zero() -> bool {
        false
    }
    fn one() -> bool {
        true
    }
    fn add(a: &bool, b: &bool) -> bool {
        *a || *b
    }
    fn mul(a: &bool, b: &bool) -> bool {
        *a && *b
    }
}

/// Viterbi semiring (max, ×, 0, 1) for confidence
#[derive(Debug, Clone)]
pub struct ViterbiSemiring;

impl ProvenanceSemiring for ViterbiSemiring {
    type Element = f64;
    fn zero() -> f64 {
        0.0
    }
    fn one() -> f64 {
        1.0
    }
    fn add(a: &f64, b: &f64) -> f64 {
        a.max(*b)
    }
    fn mul(a: &f64, b: &f64) -> f64 {
        a * b
    }
}

/// Tropical semiring (min, +, ∞, 0) for distance
#[derive(Debug, Clone)]
pub struct TropicalSemiring;

impl ProvenanceSemiring for TropicalSemiring {
    type Element = u32;
    fn zero() -> u32 {
        u32::MAX
    }
    fn one() -> u32 {
        0
    }
    fn add(a: &u32, b: &u32) -> u32 {
        (*a).min(*b)
    }
    fn mul(a: &u32, b: &u32) -> u32 {
        a.saturating_add(*b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boolean_semiring() {
        assert!(!BooleanSemiring::zero());
        assert!(BooleanSemiring::one());
        assert!(BooleanSemiring::add(&true, &false));
        assert!(!BooleanSemiring::mul(&true, &false));
    }

    #[test]
    fn test_viterbi_semiring() {
        assert!((ViterbiSemiring::add(&0.5, &0.8) - 0.8).abs() < 1e-12);
        assert!((ViterbiSemiring::mul(&0.8, &0.8) - 0.64).abs() < 1e-12);
    }

    #[test]
    fn test_tropical_semiring() {
        assert_eq!(TropicalSemiring::add(&3, &5), 3);
        assert_eq!(TropicalSemiring::mul(&3, &5), 8);
    }
}
