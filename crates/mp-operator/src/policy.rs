//! The `ManifoldPolicy` custom resource.
//!
//! What a deployment declares, and — just as importantly — what it cannot.
//! There is no field for `alpha` and no field for the metric tensor. Both are
//! derived or measured (`docs/03`), and exposing them as knobs would invite
//! exactly the tuning-until-the-demo-looks-good that the fudge-term diagnostic
//! in the methodology warns about.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifoldPolicy {
    #[serde(default = "api_version")]
    pub api_version: String,
    #[serde(default = "kind")]
    pub kind: String,
    pub metadata: Metadata,
    pub spec: PolicySpec,
    #[serde(default)]
    pub status: Option<PolicyStatus>,
}

fn api_version() -> String {
    "manifoldplane.io/v1alpha1".into()
}
fn kind() -> String {
    "ManifoldPolicy".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicySpec {
    /// Which controller this policy guards.
    pub domain: String,

    /// The operational requirement alpha is derived from (`docs/06` T2).
    /// Stated as request counts because that is what operators can reason about.
    pub min_steps_to_boundary: f64,
    #[serde(default = "default_approach")]
    pub approach_fraction: f64,

    /// Symmetry classes. `docs/02` S1: the one thing the software genuinely
    /// cannot infer, because only the operator knows which askers are supposed
    /// to be interchangeable.
    pub symmetry_classes: Vec<SymmetryClassSpec>,

    /// Name of a ConfigMap holding a calibration produced by `mp-calibrate`.
    /// Absent means uncalibrated, which the operator surfaces as a degraded
    /// status rather than quietly running on defaults.
    #[serde(default)]
    pub calibration_config_map: Option<String>,

    /// Ceiling on how far one reconcile may move the budget. Blunt mitigation
    /// for `docs/07` F5 — recalibration is itself slow-walkable, and this bounds
    /// the per-window drift without pretending to solve it.
    #[serde(default = "default_drift")]
    pub max_budget_drift: f64,
}

fn default_approach() -> f64 {
    0.99
}
fn default_drift() -> f64 {
    0.25
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SymmetryClassSpec {
    pub name: String,
    /// Label selector identifying members. Members must be genuinely
    /// interchangeable; a class whose members do different work produces a
    /// large orbit residual for everyone and throttles the whole class.
    pub selector: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PolicyStatus {
    pub observed_generation: u64,
    pub phase: String,
    pub budget_bits_squared: f64,
    pub alpha: f64,
    pub calibrated: bool,
    pub conditions: Vec<String>,
}

impl PolicySpec {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.domain.as_str(), "kubernetes" | "ics" | "agent") {
            return Err(format!("unknown domain {:?}", self.domain));
        }
        if self.min_steps_to_boundary < 1.0 {
            return Err("minStepsToBoundary must be at least 1".into());
        }
        if !(self.approach_fraction > 0.0 && self.approach_fraction < 1.0) {
            return Err("approachFraction must be in (0,1)".into());
        }
        if self.symmetry_classes.is_empty() {
            return Err("at least one symmetry class is required (docs/02 S1)".into());
        }
        for c in &self.symmetry_classes {
            if c.selector.is_empty() {
                return Err(format!("symmetry class {:?} has an empty selector", c.name));
            }
        }
        if !(self.max_budget_drift > 0.0 && self.max_budget_drift <= 1.0) {
            return Err("maxBudgetDrift must be in (0,1]".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> PolicySpec {
        PolicySpec {
            domain: "kubernetes".into(),
            min_steps_to_boundary: 200.0,
            approach_fraction: 0.99,
            symmetry_classes: vec![SymmetryClassSpec {
                name: "controllers".into(),
                selector: [("app".to_string(), "ctrl".to_string())].into_iter().collect(),
            }],
            calibration_config_map: Some("mp-calibration".into()),
            max_budget_drift: 0.25,
        }
    }

    #[test]
    fn a_well_formed_spec_validates() {
        assert!(spec().validate().is_ok());
    }

    #[test]
    fn a_policy_with_no_symmetry_classes_is_rejected() {
        // Without classes there is no orbit residual, which disables half the
        // system (docs/07 F2). Better to refuse the policy than to run degraded.
        let mut s = spec();
        s.symmetry_classes.clear();
        assert!(s.validate().is_err());
    }

    #[test]
    fn an_empty_selector_is_rejected() {
        let mut s = spec();
        s.symmetry_classes[0].selector.clear();
        assert!(s.validate().is_err());
    }

    #[test]
    fn an_unknown_domain_is_rejected() {
        let mut s = spec();
        s.domain = "mainframe".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn the_spec_exposes_no_alpha_or_metric_field() {
        // Guards an intentional API decision. Both are derived or measured, and
        // making them settable invites tuning until the demo looks right, which
        // is the failure mode docs/03's measurement discipline exists to stop.
        let json = serde_json::to_string(&spec()).unwrap();
        assert!(!json.contains("\"alpha\""));
        assert!(!json.contains("metric"));
        assert!(json.contains("minStepsToBoundary"));
    }

    #[test]
    fn unknown_spec_fields_are_rejected() {
        let r: Result<PolicySpec, _> =
            serde_json::from_str(r#"{"domain":"kubernetes","minStepsToBoundary":10,
                "symmetryClasses":[],"alpha":0.9}"#);
        assert!(r.is_err(), "alpha must not be silently accepted");
    }

    #[test]
    fn a_policy_round_trips_through_json() {
        let p = ManifoldPolicy {
            api_version: api_version(),
            kind: kind(),
            metadata: Metadata { name: "p".into(), namespace: None, generation: 3 },
            spec: spec(),
            status: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: ManifoldPolicy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }
}
