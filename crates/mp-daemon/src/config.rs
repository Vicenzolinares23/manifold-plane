//! Daemon configuration.
//!
//! Every constant here traces to a measurement procedure in
//! `docs/03-dimensional-analysis.md`. Where a value can be *derived* from an
//! operational requirement rather than typed in, the config takes the
//! requirement and derives the value — `alpha` is the important case.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub listen: String,
    pub domain: Domain,

    /// Operational requirement from which `α` is derived (`docs/06` T2).
    ///
    /// Read as: "an attacker must need at least this many admitted requests to
    /// go from a nominal position to `approach_fraction` of the budget." This
    /// is stated instead of `α` because operators can reason about request
    /// counts and cannot reason about a barrier coefficient.
    pub min_steps_to_boundary: f64,
    /// Fraction of the budget the above requirement refers to.
    pub approach_fraction: f64,

    /// Danger budget `c`, in bits². Calibrated by `mp-calibrate` from a benign
    /// corpus; a hand-set value here is a bootstrap and is warned about.
    pub budget_bits_squared: f64,
    /// Set true once `budget_bits_squared` came from a real calibration run.
    pub budget_is_calibrated: bool,

    /// Review band as a fraction of the budget.
    pub review_band: f64,
    /// Tempo bits charged for a denied request.
    pub denial_weight_bits: f64,

    /// `κ_min`: coupling in bits above which askers are treated jointly.
    pub kappa_min_bits: f64,
    pub max_coalition: usize,
    pub idle_evict_secs: f64,

    /// Per-axis half-lives in seconds. Defaults are the nominal values from
    /// `docs/03`; a deployment fits its own.
    pub half_lives_secs: Option<[f64; 6]>,

    /// Refuse a recalibration that moves `c` by more than this fraction in one
    /// window. A blunt guard against `docs/07` F5 (recalibration is itself
    /// slow-walkable), and honestly labelled as blunt — the principled fix is
    /// a barrier on the parameter trajectory, which is not implemented.
    pub max_budget_drift_per_refit: f64,

    /// Fail open on internal error instead of closed.
    ///
    /// Defaults to false. An admission controller that fails open is a
    /// controller an attacker can disable by crashing it, and the whole point
    /// of this system is that capability accumulates when nobody is looking.
    pub fail_open: bool,

    pub log_decisions: bool,
    pub max_body_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Kubernetes,
    Ics,
    Agent,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen: "127.0.0.1:8443".to_string(),
            domain: Domain::Kubernetes,
            min_steps_to_boundary: 200.0,
            approach_fraction: 0.99,
            budget_bits_squared: 64.0,
            budget_is_calibrated: false,
            review_band: 0.02,
            denial_weight_bits: 0.25,
            kappa_min_bits: 0.5,
            max_coalition: 8,
            idle_evict_secs: 30.0 * 86400.0,
            half_lives_secs: None,
            max_budget_drift_per_refit: 0.25,
            fail_open: false,
            log_decisions: true,
            max_body_bytes: 1 << 20,
        }
    }
}

/// A startup condition that weakens the system without stopping it.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning(pub String);

impl Config {
    pub fn from_toml_like(src: &str) -> Result<Config, String> {
        // Deliberately JSON rather than TOML: it keeps the dependency list at
        // serde alone, and the config is machine-generated in every deployment
        // path we support.
        serde_json::from_str(src).map_err(|e| format!("config parse error: {e}"))
    }

    /// `α`, derived from the operational requirement rather than configured.
    pub fn alpha(&self) -> f64 {
        let h_from = self.budget_bits_squared;
        let h_to = self.budget_bits_squared * (1.0 - self.approach_fraction).max(1e-9);
        mp_barrier::BarrierConfig::alpha_for_min_steps(h_from, h_to, self.min_steps_to_boundary)
    }

    pub fn barrier_config(&self) -> mp_barrier::BarrierConfig {
        mp_barrier::BarrierConfig {
            alpha: self.alpha(),
            budget: self.budget_bits_squared,
            review_band: self.review_band,
            denial_weight_bits: self.denial_weight_bits,
        }
    }

    pub fn engine_config(&self) -> mp_barrier::EngineConfig {
        mp_barrier::EngineConfig {
            kappa_min: self.kappa_min_bits,
            max_coalition: self.max_coalition,
            idle_evict_secs: self.idle_evict_secs,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.budget_bits_squared <= 0.0 {
            return Err("budget_bits_squared must be positive".into());
        }
        if !(self.approach_fraction > 0.0 && self.approach_fraction < 1.0) {
            return Err("approach_fraction must be in (0,1)".into());
        }
        if self.min_steps_to_boundary < 1.0 {
            return Err("min_steps_to_boundary must be at least 1".into());
        }
        if self.max_coalition < 2 {
            return Err("max_coalition must be at least 2".into());
        }
        self.barrier_config().validate()
    }

    /// Conditions that weaken the system without preventing startup.
    ///
    /// Emitted loudly at boot. A deployment can run in every one of these
    /// states; it should do so knowingly.
    pub fn warnings(&self) -> Vec<Warning> {
        let mut w = Vec::new();
        if !self.budget_is_calibrated {
            w.push(Warning(
                "budget_bits_squared was not calibrated from a benign corpus. \
                 docs/03 requires c be measured as a high quantile of V, not chosen. \
                 Decisions are being made against an arbitrary boundary."
                    .into(),
            ));
        }
        if self.fail_open {
            w.push(Warning(
                "fail_open is set. An attacker who can crash this process can disable \
                 admission control entirely."
                    .into(),
            ));
        }
        if self.half_lives_secs.is_none() {
            w.push(Warning(
                "using nominal half-lives from docs/03 rather than values fitted to this \
                 deployment. A half-life is a property of an environment, not of this software."
                    .into(),
            ));
        }
        if self.max_coalition < 4 {
            w.push(Warning(
                "max_coalition below 4 will miss most real coalitions (docs/05 §5.7).".into(),
            ));
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn alpha_is_derived_and_honours_the_step_requirement() {
        // The whole point of stating min_steps rather than alpha: the derived
        // alpha should actually deliver the requested number of steps.
        let mut c = Config::default();
        c.min_steps_to_boundary = 500.0;
        c.approach_fraction = 0.99;
        let alpha = c.alpha();

        let h0 = c.budget_bits_squared;
        let h_target = c.budget_bits_squared * 0.01;
        let steps = (h_target / h0).ln() / (1.0 - alpha).ln();
        assert!((steps - 500.0).abs() < 1.0, "derived alpha gives {steps} steps, wanted 500");
    }

    #[test]
    fn demanding_more_steps_yields_a_smaller_alpha() {
        let mut slow = Config::default();
        slow.min_steps_to_boundary = 10_000.0;
        let mut fast = Config::default();
        fast.min_steps_to_boundary = 10.0;
        assert!(slow.alpha() < fast.alpha());
    }

    #[test]
    fn an_uncalibrated_budget_warns() {
        let c = Config::default();
        assert!(c.warnings().iter().any(|w| w.0.contains("calibrated")));
    }

    #[test]
    fn fail_open_warns() {
        let c = Config { fail_open: true, ..Default::default() };
        assert!(c.warnings().iter().any(|w| w.0.contains("fail_open")));
    }

    #[test]
    fn a_fully_configured_deployment_has_no_warnings() {
        let c = Config {
            budget_is_calibrated: true,
            half_lives_secs: Some([30.0, 300.0, 3600.0, 43200.0, 172800.0, 3.15e12]),
            ..Default::default()
        };
        assert!(c.warnings().is_empty(), "unexpected warnings: {:?}", c.warnings());
    }

    #[test]
    fn config_round_trips_through_json() {
        let c = Config::default();
        let s = serde_json::to_string(&c).unwrap();
        let back = Config::from_toml_like(&s).unwrap();
        assert_eq!(back.listen, c.listen);
        assert_eq!(back.budget_bits_squared, c.budget_bits_squared);
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        // A typo in a security control's config must not silently take the
        // default. deny_unknown_fields makes that impossible.
        let r = Config::from_toml_like(r#"{"budget_bits_sqaured": 10.0}"#);
        assert!(r.is_err());
    }

    #[test]
    fn invalid_values_are_rejected() {
        let c = Config { approach_fraction: 1.5, ..Default::default() };
        assert!(c.validate().is_err());
        let c = Config { budget_bits_squared: -1.0, ..Default::default() };
        assert!(c.validate().is_err());
    }
}
