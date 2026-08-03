//! Domain adapters: they turn requests into displacements, and nothing else.
//!
//! The kernel never sees a Kubernetes object, a Modbus frame, or an agent tool
//! call. It sees a vector in bits. That separation is what lets one engine serve
//! three domains whose practitioner communities do not talk to each other, and
//! it is the strongest evidence that the state space found in `docs/04` is real
//! rather than fitted to one setting.
//!
//! **These are the softest part of the system.** `docs/05` §5.11 is explicit:
//! the kernel's guarantee is conditional on `g` being a faithful measurement of
//! what a request actually confers. A wrong `g` produces a mathematically
//! impeccable bound on the wrong quantity. Every displacement below cites the
//! measurement rule from `docs/03` it implements, so a reviewer can check the
//! mapping rather than take it on faith.

// Fixed-size 6x6 arithmetic. Indexed loops mirror the index notation in
// docs/ line for line, which matters more here than iterator idiom: these
// routines are meant to be checked against the mathematics by hand.
#![allow(clippy::needless_range_loop)]

pub mod agent;
pub mod coupling;
pub mod ics;
pub mod k8s;

use mp_core::axis::Axis;
use mp_core::linalg::{Vec6, N};

/// Builder for a displacement vector, in bits.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Displacement {
    v: Vec6,
}

impl Displacement {
    pub fn zero() -> Self {
        Displacement { v: [0.0; N] }
    }

    /// Add `bits` along one axis.
    pub fn with(mut self, ax: Axis, bits: f64) -> Self {
        self.v[ax.index()] += bits;
        self
    }

    pub fn get(&self, ax: Axis) -> f64 {
        self.v[ax.index()]
    }

    pub fn into_vec(self) -> Vec6 {
        self.v
    }

    pub fn as_vec(&self) -> &Vec6 {
        &self.v
    }
}

/// `log2(count / baseline)`, the form every axis in `docs/03` reduces to.
///
/// Clamped below at zero: displacement is signed but a *request* never reduces
/// capability below the asker's own baseline. Returning a negative value here
/// would let an adapter hand back margin the asker never spent.
pub fn log_ratio_bits(count: f64, baseline: f64) -> f64 {
    if baseline <= 0.0 || count <= 0.0 {
        return 0.0;
    }
    (count / baseline).log2().max(0.0)
}

/// Bits of irreversibility from a preimage count (`docs/03` A3).
///
/// This is a literal entropy: the number of bits about the prior world state
/// that can no longer be recovered.
pub fn irreversibility_bits(preimages: f64) -> f64 {
    if preimages <= 1.0 {
        0.0 // bijective; the world can be put back
    } else {
        preimages.log2()
    }
}

/// Bits of opacity from the fraction of the action that leaves a record
/// (`docs/03` A4): `ω = -log2(recorded_fraction)`.
///
/// Capped rather than allowed to reach infinity at zero recording. The pole is
/// mathematically correct — a fully unobservable asker *is* infinitely opaque —
/// but an infinity propagates into `V` and makes every subsequent decision a
/// denial with no diagnostic value. The cap is a numerical concession and is
/// documented as one; 32 bits is far past any budget a deployment will set.
pub fn opacity_bits(recorded_fraction: f64) -> f64 {
    if recorded_fraction >= 1.0 {
        0.0
    } else if recorded_fraction <= 0.0 {
        32.0
    } else {
        (-recorded_fraction.log2()).min(32.0)
    }
}

/// Tempo bits from an observed rate against the asker's own baseline
/// (`docs/03` A6). Never absolute rate: `docs/01` I5 forbids it.
pub fn tempo_bits(rate: f64, baseline_rate: f64) -> f64 {
    log_ratio_bits(rate, baseline_rate)
}

/// What every adapter provides.
pub trait Adapter {
    /// The domain's native request type.
    type Request;

    /// Stable adapter name, carried into decision logs.
    fn name(&self) -> &'static str;

    /// `g(r, x)` — the displacement this request would apply.
    ///
    /// `current` is the asker's present displacement, because `g` is
    /// state-dependent by `docs/04`: acquiring a credential when you already
    /// hold broad reach is worth more than acquiring it in isolation.
    fn displacement(&self, req: &Self::Request, current: &Vec6) -> Displacement;
}

/// Amplify a displacement by the asker's existing reach.
///
/// The state-dependence of `g` from `docs/04`, in one place so all three
/// adapters share it. A step taken from a position of broad reach is worth
/// more than the same step taken from baseline, because the acquired capability
/// composes with what is already held.
///
/// Sub-linear in existing reach (`log2(1 + h)`) rather than linear: capability
/// composes, but two footholds are not twice one foothold, and a linear factor
/// would make the model explosively conservative for legitimately broad askers.
pub fn amplify_by_reach(base: Displacement, current: &Vec6) -> Displacement {
    let existing_reach = current[Axis::Reach.index()].max(0.0);
    let factor = 1.0 + (1.0 + existing_reach).log2();
    let mut out = Displacement::zero();
    for ax in mp_core::axis::ALL_AXES {
        out = out.with(ax, base.get(ax) * factor);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_changes_nothing_is_free() {
        assert_eq!(irreversibility_bits(1.0), 0.0);
        assert_eq!(irreversibility_bits(0.5), 0.0);
    }

    #[test]
    fn doubling_the_preimage_count_costs_one_bit() {
        assert!((irreversibility_bits(2.0) - 1.0).abs() < 1e-12);
        assert!((irreversibility_bits(256.0) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn full_recording_is_zero_opacity_and_none_is_capped() {
        assert_eq!(opacity_bits(1.0), 0.0);
        assert_eq!(opacity_bits(0.0), 32.0);
        assert!((opacity_bits(0.25) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_rate_at_baseline_costs_nothing_and_sixteen_fold_costs_four_bits() {
        assert_eq!(tempo_bits(10.0, 10.0), 0.0);
        assert!((tempo_bits(160.0, 10.0) - 4.0).abs() < 1e-12);
    }

    #[test]
    fn a_rate_below_baseline_never_returns_margin() {
        // Slowing down must not refund capability the asker already spent.
        assert_eq!(tempo_bits(1.0, 100.0), 0.0);
    }

    #[test]
    fn reach_amplification_is_sublinear() {
        let base = Displacement::zero().with(Axis::Authority, 1.0);
        let mut broad = [0.0; N];
        broad[Axis::Reach.index()] = 15.0;
        let amplified = amplify_by_reach(base, &broad);
        assert!(amplified.get(Axis::Authority) > 1.0);
        assert!(
            amplified.get(Axis::Authority) < 16.0,
            "linear amplification would be explosively conservative"
        );
    }

    #[test]
    fn amplification_is_the_identity_at_baseline() {
        let base = Displacement::zero().with(Axis::Authority, 2.0);
        let out = amplify_by_reach(base, &[0.0; N]);
        assert!((out.get(Axis::Authority) - 2.0).abs() < 1e-12);
    }
}
