//! The admission kernel: a discrete-time exponential control barrier function.
//!
//! `docs/05-formulation.md` §5.5, proofs in `docs/06-proofs.md`.
//!
//! The rule is
//!
//! ```text
//!     ADMIT r  ⟺  h(x⁺) ≥ (1 - α)·h(x)
//! ```
//!
//! rather than the obvious `V(x⁺) ≤ c`. The difference is the entire point. A
//! threshold permits a full-speed walk to the boundary and admits the largest
//! possible step everywhere — exactly what a patient attacker wants. The
//! barrier condition makes the admissible step shrink in proportion to the
//! margin remaining, so approach speed decays geometrically to zero.
//!
//! Because the constraint is enforced at every step and is relative to the
//! current margin, it says something about *every possible infinite future
//! sequence* while examining only one step from one position. That is what
//! reconciles invariant I6 (danger lives in sequences) with I7 (no lookahead
//! is available at decision time).

use mp_core::linalg::{self, Vec6};
use mp_core::metric::Metric;

pub mod coalition;
pub mod engine;
pub mod orbit;

pub use coalition::{Coalition, CouplingGraph};
pub use engine::{Engine, EngineConfig, Outcome, Proposal};
pub use orbit::OrbitResidual;

/// Outcome of an admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Admit,
    /// Inside the review band: escalate to a human or to step-up
    /// authentication. Binary admit/deny is wrong for agent tool-calls and
    /// often wrong elsewhere (`docs/05` §5.5).
    Hold,
    Deny,
}

impl Decision {
    pub fn is_admitted(self) -> bool {
        matches!(self, Decision::Admit)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Admit => "admit",
            Decision::Hold => "hold",
            Decision::Deny => "deny",
        }
    }
}

/// Why a decision came out the way it did.
///
/// Every field is a number an operator can check by hand against `docs/06`. An
/// admission control that cannot explain itself is not deployable regardless of
/// how good its mathematics is, and "the model said so" is not an explanation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    pub decision: Decision,
    /// `h(x)` before the step, in bits².
    pub margin_before: f64,
    /// `h(x⁺)` after the proposed step, in bits².
    pub margin_after: f64,
    /// `(1-α_eff)·h(x)` — what the step had to clear.
    pub required: f64,
    /// `α` after the orbit-residual adjustment.
    pub alpha_effective: f64,
    /// Π₄ from `docs/03`: how far this asker has drifted from its peers.
    pub orbit_residual: f64,
    /// Fraction of budget consumed after the step, Π₃.
    pub budget_fraction: f64,
    /// If a coalition rather than the asker itself blocked this, its size.
    pub blocked_by_coalition: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarrierConfig {
    /// `α ∈ (0,1]`, the fraction of remaining margin spendable per step.
    ///
    /// Not a tuning knob. `docs/06` T2 inverts the time-to-boundary bound so an
    /// operator states "an attacker needs at least N admitted requests to get
    /// from nominal to 99% of budget" and α follows. Use
    /// [`BarrierConfig::alpha_for_min_steps`].
    pub alpha: f64,
    /// Danger budget `c`, in bits². Measured as a high quantile of `V` over the
    /// benign corpus (`mp_core::metric::calibrate_budget`), never chosen.
    pub budget: f64,
    /// Review band as a fraction of the budget. Steps that miss the barrier
    /// condition by less than this are held rather than denied.
    pub review_band: f64,
    /// Tempo cost in bits charged for a denied request.
    pub denial_weight_bits: f64,
}

impl Default for BarrierConfig {
    fn default() -> Self {
        BarrierConfig {
            alpha: 0.05,
            budget: 1.0,
            review_band: 0.02,
            denial_weight_bits: 0.25,
        }
    }
}

impl BarrierConfig {
    /// Derive `α` from an operational requirement, per `docs/06` T2.
    ///
    /// `min_steps` is the least number of admitted requests an attacker must
    /// make to move the margin from `h_from` down to `h_to`. This is the
    /// measurement procedure `docs/03` promised for α.
    pub fn alpha_for_min_steps(h_from: f64, h_to: f64, min_steps: f64) -> f64 {
        if min_steps <= 0.0 || h_from <= 0.0 || h_to <= 0.0 || h_to >= h_from {
            return 1.0;
        }
        // h_to = (1-α)^N · h_from  ⟹  α = 1 - (h_to/h_from)^(1/N)
        (1.0 - (h_to / h_from).powf(1.0 / min_steps)).clamp(1e-9, 1.0)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.alpha > 0.0 && self.alpha <= 1.0) {
            return Err(format!("alpha must be in (0,1], got {}", self.alpha));
        }
        if self.budget <= 0.0 {
            return Err(format!("budget must be positive, got {}", self.budget));
        }
        if self.review_band < 0.0 {
            return Err(format!(
                "review_band must be non-negative, got {}",
                self.review_band
            ));
        }
        Ok(())
    }
}

/// The admission kernel.
#[derive(Debug, Clone, Copy)]
pub struct Barrier {
    metric: Metric,
    cfg: BarrierConfig,
}

impl Barrier {
    pub fn new(metric: Metric, cfg: BarrierConfig) -> Result<Self, String> {
        cfg.validate()?;
        Ok(Barrier { metric, cfg })
    }

    pub fn config(&self) -> &BarrierConfig {
        &self.cfg
    }

    pub fn metric(&self) -> &Metric {
        &self.metric
    }

    /// `V(z) = zᵀ M z`, in bits².
    pub fn potential(&self, z: &Vec6) -> f64 {
        self.metric.potential(z)
    }

    /// `h(z) = c - V(z)`. Non-negative inside the safe set.
    pub fn margin(&self, z: &Vec6) -> f64 {
        self.cfg.budget - self.potential(z)
    }

    pub fn is_safe(&self, z: &Vec6) -> bool {
        self.margin(z) >= 0.0
    }

    /// Evaluate one step, ignoring peers and coalitions.
    ///
    /// `z` must already be relaxed to the decision time (`docs/05` §5.10).
    pub fn evaluate(&self, z: &Vec6, step: &Vec6) -> Verdict {
        self.evaluate_with_residual(z, step, 0.0)
    }

    /// Evaluate one step with an orbit residual tightening α (`docs/05` §5.8).
    ///
    /// An asker that has drifted from its symmetry-class peers is held to a
    /// proportionally tighter speed limit. No new threshold is introduced —
    /// the residual is measured in units of the group's own present spread.
    pub fn evaluate_with_residual(&self, z: &Vec6, step: &Vec6, residual: f64) -> Verdict {
        let z_next = linalg::add(z, step);

        let h_before = self.margin(z);
        let h_after = self.margin(&z_next);

        let alpha_eff = self.cfg.alpha / (1.0 + residual.max(0.0));
        let required = (1.0 - alpha_eff) * h_before;

        let decision = if h_before < 0.0 {
            // Already outside the safe set. Forward invariance says this cannot
            // happen via admitted steps, so it means the state was seeded
            // outside, the budget shrank on recalibration, or there is a bug.
            // Refuse and let the operator find out, rather than quietly
            // re-deriving a margin from an invalid position.
            Decision::Deny
        } else if h_after >= required {
            Decision::Admit
        } else if h_after > 0.0 && h_after >= required - self.cfg.review_band * self.cfg.budget {
            Decision::Hold
        } else {
            Decision::Deny
        };

        Verdict {
            decision,
            margin_before: h_before,
            margin_after: h_after,
            required,
            alpha_effective: alpha_eff,
            orbit_residual: residual,
            budget_fraction: self.potential(&z_next) / self.cfg.budget,
            blocked_by_coalition: None,
        }
    }

    /// The step-size envelope from `docs/06` T4:
    ///
    /// ```text
    ///     ‖g‖²_M + 2⟨z, g⟩_M ≤ α·h(z)
    /// ```
    ///
    /// Returns the slack. Non-negative means the step is admissible. Exposed
    /// separately because it is the quantity an adapter needs in order to
    /// *shrink* a request into admissibility rather than reject it outright.
    pub fn envelope_slack(&self, z: &Vec6, step: &Vec6) -> f64 {
        let lhs = self.metric.potential(step) + 2.0 * self.metric.inner(z, step);
        self.cfg.alpha * self.margin(z) - lhs
    }

    /// Largest `s ≥ 0` such that the scaled step `s·g` saturates the barrier
    /// condition, with no upper clamp.
    ///
    /// Solves `s²‖g‖²_M + 2s⟨z,g⟩_M − α·h = 0` for the positive root. This is
    /// the *optimal adversary's* step: it is what an attacker who can choose
    /// request magnitude freely would take at every point. The adversarial
    /// simulator and the T2 bound check use this; enforcement does not.
    pub fn saturating_scale(&self, z: &Vec6, step: &Vec6) -> f64 {
        let a = self.metric.potential(step);
        let b = 2.0 * self.metric.inner(z, step);
        let c = -self.cfg.alpha * self.margin(z);

        if c >= 0.0 {
            return 0.0; // no margin left to spend
        }
        if a.abs() < 1e-18 {
            // Degenerate: the step has no length in the metric.
            return if b.abs() < 1e-18 {
                f64::INFINITY
            } else {
                (-c / b).max(0.0)
            };
        }
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return 0.0;
        }
        ((-b + disc.sqrt()) / (2.0 * a)).max(0.0)
    }

    /// Largest `s ∈ [0,1]` such that the scaled step `s·g` is admissible.
    ///
    /// The clamp is what separates this from [`Barrier::saturating_scale`]: a
    /// requested step may be *shrunk* into admissibility, never enlarged. Useful
    /// for domains where a request can be partially satisfied — narrowing an
    /// RBAC grant, clamping an ICS setpoint, restricting an agent tool's scope —
    /// which is strictly better than denial when it is available.
    pub fn max_admissible_scale(&self, z: &Vec6, step: &Vec6) -> f64 {
        self.saturating_scale(z, step).clamp(0.0, 1.0)
    }

    /// The T2 bound: the most `V` an optimal adversary can reach in `n` steps.
    ///
    /// `V_n ≤ c − (1−α)ⁿ·(c − V_0)`. Used by the simulator to check measured
    /// attacker performance against theory.
    pub fn adversary_bound(&self, v0: f64, n: u32) -> f64 {
        let h0 = self.cfg.budget - v0;
        self.cfg.budget - (1.0 - self.cfg.alpha).powi(n as i32) * h0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_core::linalg::N;

    fn barrier(alpha: f64, budget: f64) -> Barrier {
        Barrier::new(
            Metric::identity(),
            BarrierConfig {
                alpha,
                budget,
                review_band: 0.0,
                denial_weight_bits: 0.25,
            },
        )
        .unwrap()
    }

    fn axis_step(i: usize, v: f64) -> Vec6 {
        let mut s = [0.0; N];
        s[i] = v;
        s
    }

    #[test]
    fn a_small_step_from_the_origin_is_admitted() {
        let b = barrier(0.5, 100.0);
        assert_eq!(
            b.evaluate(&[0.0; N], &axis_step(0, 1.0)).decision,
            Decision::Admit
        );
    }

    #[test]
    fn a_step_that_leaves_the_set_is_denied() {
        let b = barrier(0.5, 100.0);
        let v = b.evaluate(&[0.0; N], &axis_step(0, 50.0));
        assert_eq!(v.decision, Decision::Deny);
        assert!(v.margin_after < 0.0);
    }

    #[test]
    fn admissible_step_shrinks_as_the_boundary_approaches() {
        // The property that distinguishes the barrier rule from a threshold.
        // Under a threshold the admissible step stays large until the last
        // moment; here it decays toward zero.
        let b = barrier(0.1, 100.0);
        let mut last = f64::INFINITY;
        for v0 in [0.0f64, 25.0, 50.0, 75.0, 90.0, 99.0] {
            let z = axis_step(0, v0.sqrt());
            let s = b.max_admissible_scale(&z, &axis_step(0, 1.0));
            assert!(
                s < last,
                "scale should decrease monotonically: {s} !< {last}"
            );
            last = s;
        }
        assert!(
            last < 0.15,
            "near the boundary the step should be tiny, got {last}"
        );
    }

    #[test]
    fn t1_forward_invariance_under_a_greedy_adversary() {
        // docs/06 T1. An adversary always taking the largest admissible step
        // must never leave the safe set, for any number of steps.
        let b = barrier(0.2, 50.0);
        let mut z = [0.0; N];
        for _ in 0..100_000 {
            let dir = axis_step(2, 1.0);
            let s = b.max_admissible_scale(&z, &dir);
            let step = linalg::scale(&dir, s);
            if b.evaluate(&z, &step).decision.is_admitted() {
                z = linalg::add(&z, &step);
            }
            assert!(
                b.margin(&z) >= -1e-9,
                "escaped the safe set: h={}",
                b.margin(&z)
            );
        }
    }

    #[test]
    fn t2_adversary_never_reaches_the_budget_in_finite_steps() {
        let b = barrier(0.2, 50.0);
        let mut z = [0.0; N];
        for _ in 0..10_000 {
            let dir = axis_step(2, 1.0);
            let s = b.saturating_scale(&z, &dir);
            z = linalg::add(&z, &linalg::scale(&dir, s));
        }
        assert!(b.potential(&z) < b.config().budget);
    }

    #[test]
    fn t2_measured_approach_matches_the_analytic_bound() {
        // A saturating adversary should track V_n = c - (1-α)^n (c - V_0)
        // exactly. If it does not, either the bound or max_admissible_scale is
        // wrong, and the two are derived independently.
        let b = barrier(0.1, 100.0);
        let mut z = [0.0; N];
        for n in 1..=60u32 {
            let dir = axis_step(3, 1.0);
            let s = b.saturating_scale(&z, &dir);
            z = linalg::add(&z, &linalg::scale(&dir, s));
            let bound = b.adversary_bound(0.0, n);
            let actual = b.potential(&z);
            assert!(
                actual <= bound + 1e-6,
                "step {n}: {actual} exceeded bound {bound}"
            );
            assert!(
                actual >= bound - 1e-6,
                "step {n}: saturating adversary should achieve the bound, {actual} vs {bound}"
            );
        }
    }

    #[test]
    fn t4_envelope_slack_agrees_with_the_decision() {
        let b = barrier(0.3, 40.0);
        let z = axis_step(1, 3.0);
        for mag in [0.01, 0.1, 0.5, 1.0, 2.0, 5.0] {
            let step = axis_step(1, mag);
            let slack = b.envelope_slack(&z, &step);
            let admitted = b.evaluate(&z, &step).decision.is_admitted();
            assert_eq!(
                slack >= -1e-12,
                admitted,
                "envelope and decision disagree at {mag}"
            );
        }
    }

    #[test]
    fn t4_steps_back_toward_baseline_are_never_throttled() {
        // Falls out of the cross-term in T4 rather than being designed in: an
        // asker that reduces its own capability is always allowed to.
        let b = barrier(0.01, 10.0);
        let z = axis_step(0, 3.0);
        for mag in [0.1, 1.0, 2.9] {
            assert_eq!(
                b.evaluate(&z, &axis_step(0, -mag)).decision,
                Decision::Admit
            );
        }
    }

    #[test]
    fn orbit_residual_tightens_the_speed_limit() {
        let b = barrier(0.5, 100.0);
        let z = [0.0; N];
        let step = axis_step(0, 7.0);
        let clean = b.evaluate_with_residual(&z, &step, 0.0);
        let drifted = b.evaluate_with_residual(&z, &step, 5.0);
        assert!(drifted.alpha_effective < clean.alpha_effective);
        assert_eq!(clean.decision, Decision::Admit);
        assert_eq!(drifted.decision, Decision::Deny);
    }

    #[test]
    fn a_state_already_outside_the_set_is_refused() {
        let b = barrier(0.5, 1.0);
        assert_eq!(
            b.evaluate(&axis_step(0, 10.0), &[0.0; N]).decision,
            Decision::Deny
        );
    }

    #[test]
    fn alpha_derivation_round_trips() {
        let alpha = BarrierConfig::alpha_for_min_steps(100.0, 1.0, 200.0);
        let h = 100.0 * (1.0 - alpha).powi(200);
        assert!((h - 1.0).abs() < 1e-6, "got h={h}");
    }

    #[test]
    fn config_rejects_out_of_range_alpha() {
        assert!(Barrier::new(
            Metric::identity(),
            BarrierConfig {
                alpha: 1.5,
                ..Default::default()
            }
        )
        .is_err());
        assert!(Barrier::new(
            Metric::identity(),
            BarrierConfig {
                alpha: 0.0,
                ..Default::default()
            }
        )
        .is_err());
    }
}
