//! Asker state and relaxation dynamics.
//!
//! `docs/05-formulation.md` §5.1, §5.4.
//!
//! The state is *carried*, not recomputed per request. That is the whole
//! difference from a conventional policy engine, and it follows from invariant
//! I1 in `docs/01`: approvals only ever push capability outward, and the only
//! inward force is elapsed time.

use crate::axis::{nominal_half_lives, rates_from_half_lives, Axis};
use crate::linalg::{self, Vec6, N};

/// Opaque asker identity.
///
/// Deliberately opaque: `docs/02` S1 requires the model be equivariant under
/// relabeling of askers, so nothing downstream may branch on the contents. An
/// identity is a key for looking up carried state and nothing more.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AskerId(pub String);

impl AskerId {
    pub fn new(s: impl Into<String>) -> Self {
        AskerId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AskerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A symmetry class: the set of askers that are interchangeable by
/// construction (`docs/02` S1, S3). Replicas of one deployment, identical
/// operator instances, sensors of the same model on the same segment.
///
/// This is what makes the orbit-residual detector in `docs/05` §5.8 possible,
/// and it is the one piece of configuration a deployment genuinely must supply
/// — the software cannot infer which askers are supposed to be alike.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymmetryClass(pub String);

impl SymmetryClass {
    pub fn new(s: impl Into<String>) -> Self {
        SymmetryClass(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The relaxation generator `Λ = diag(ln2 / T½)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relaxation {
    rates: Vec6,
}

impl Default for Relaxation {
    fn default() -> Self {
        Relaxation::from_half_lives(&nominal_half_lives())
    }
}

impl Relaxation {
    pub fn from_half_lives(half_lives: &Vec6) -> Self {
        Relaxation { rates: rates_from_half_lives(half_lives) }
    }

    pub fn from_rates(rates: Vec6) -> Self {
        Relaxation { rates }
    }

    pub fn rates(&self) -> &Vec6 {
        &self.rates
    }

    /// `D_Δt = exp(-Λ Δt)`, the per-axis decay factors.
    pub fn decay_factors(&self, dt_secs: f64) -> Vec6 {
        let dt = dt_secs.max(0.0);
        let mut d = [0.0; N];
        for i in 0..N {
            d[i] = (-self.rates[i] * dt).exp();
        }
        d
    }

    /// `R_Δt(x) = x₀ + D_Δt · (x - x₀)`.
    ///
    /// Note the fixed point is the baseline, not the origin: `docs/02` N3. An
    /// asker at rest returns to its own normal, not to zero.
    pub fn relax(&self, z: &Vec6, dt_secs: f64) -> Vec6 {
        let d = self.decay_factors(dt_secs);
        let mut out = [0.0; N];
        for i in 0..N {
            out[i] = z[i] * d[i];
        }
        out
    }
}

/// The carried state of one asker.
///
/// Stored as displacement `z = x - x₀` from that asker's own measured baseline,
/// which keeps the representation invariant under S1: two askers with identical
/// behavior relative to their own baselines have identical `z` regardless of
/// how different their absolute levels are.
#[derive(Debug, Clone, PartialEq)]
pub struct AskerState {
    pub id: AskerId,
    pub class: SymmetryClass,
    /// Displacement from baseline, in bits, per axis.
    pub z: Vec6,
    /// Unix timestamp (seconds, fractional) of the last event.
    pub last_seen: f64,
    /// Count of admitted requests, for diagnostics and the T2 bound check.
    pub admitted: u64,
    /// Count of denied requests. Denials are not free: they feed tempo.
    pub denied: u64,
    /// Count of requests held for review.
    pub held: u64,
}

impl AskerState {
    pub fn new(id: AskerId, class: SymmetryClass, now: f64) -> Self {
        AskerState {
            id,
            class,
            z: linalg::ZERO_V,
            last_seen: now,
            admitted: 0,
            denied: 0,
            held: 0,
        }
    }

    /// Advance to `now` by relaxation, returning the relaxed displacement
    /// without mutating. `docs/05` §5.10 relaxes before evaluating, so the
    /// barrier is always checked against a current state rather than a stale one.
    pub fn relaxed_at(&self, relax: &Relaxation, now: f64) -> Vec6 {
        relax.relax(&self.z, now - self.last_seen)
    }

    /// Apply relaxation in place.
    pub fn advance_to(&mut self, relax: &Relaxation, now: f64) {
        self.z = self.relaxed_at(relax, now);
        self.last_seen = now;
    }

    pub fn get(&self, ax: Axis) -> f64 {
        self.z[ax.index()]
    }

    pub fn set(&mut self, ax: Axis, v: f64) {
        self.z[ax.index()] = v;
    }

    /// Nudge tempo on a denial. Probing costs something, per `docs/05` §5.5.
    ///
    /// One denial is a fraction of a bit; a sustained probe accumulates into a
    /// real tempo excursion, and tempo's 30-second half-life means an
    /// occasional legitimate rejection is forgotten almost immediately.
    pub fn record_denial(&mut self, weight_bits: f64) {
        self.denied += 1;
        self.z[Axis::Tempo.index()] += weight_bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> AskerState {
        AskerState::new(AskerId::new("a"), SymmetryClass::new("c"), 0.0)
    }

    #[test]
    fn relaxation_halves_an_axis_after_one_half_life() {
        let r = Relaxation::default();
        let mut z = linalg::ZERO_V;
        z[Axis::Tempo.index()] = 8.0;
        let out = r.relax(&z, Axis::Tempo.nominal_half_life_secs());
        assert!((out[Axis::Tempo.index()] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn irreversibility_barely_decays_over_a_year() {
        let r = Relaxation::default();
        let mut z = linalg::ZERO_V;
        z[Axis::Irreversibility.index()] = 10.0;
        let year = 365.0 * 86400.0;
        let out = r.relax(&z, year);
        // Destroyed information does not come back. A year should cost well
        // under one percent of the accumulated irreversibility.
        assert!(out[Axis::Irreversibility.index()] > 9.9, "got {}", out[Axis::Irreversibility.index()]);
    }

    #[test]
    fn tempo_is_essentially_forgotten_after_ten_minutes() {
        let r = Relaxation::default();
        let mut z = linalg::ZERO_V;
        z[Axis::Tempo.index()] = 10.0;
        let out = r.relax(&z, 600.0);
        assert!(out[Axis::Tempo.index()] < 0.01, "got {}", out[Axis::Tempo.index()]);
    }

    #[test]
    fn relaxation_never_overshoots_the_baseline() {
        // N3: the space is a cone with a fixed point at baseline. Decay must
        // approach zero displacement, never cross it and grow negative.
        let r = Relaxation::default();
        let z = [5.0, 4.0, 3.0, 2.0, 1.0, 6.0];
        for dt in [0.0, 1.0, 1e3, 1e6, 1e12] {
            let out = r.relax(&z, dt);
            for i in 0..N {
                assert!(out[i] >= -1e-12 && out[i] <= z[i] + 1e-12);
            }
        }
    }

    #[test]
    fn zero_elapsed_time_is_the_identity() {
        let r = Relaxation::default();
        let z = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(r.relax(&z, 0.0), z);
    }

    #[test]
    fn negative_elapsed_time_is_clamped_not_amplified() {
        // Clock skew must never run the dynamics backwards; that would inflate
        // an asker's state for free.
        let r = Relaxation::default();
        let z = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(r.relax(&z, -1000.0), z);
    }

    #[test]
    fn denials_accumulate_tempo() {
        let mut st = s();
        for _ in 0..10 {
            st.record_denial(0.25);
        }
        assert_eq!(st.denied, 10);
        assert!((st.get(Axis::Tempo) - 2.5).abs() < 1e-12);
    }
}
