//! Reconcile a `ManifoldPolicy` into a running configuration.
//!
//! Pure: takes a spec and a cluster snapshot, returns the actions to take. The
//! operator can change an admission controller's budget, which makes it a
//! privileged component in its own right, so the part that decides *what* to
//! change is verifiable without a cluster in the loop.

use crate::client::{ClusterClient, ClusterError};
use crate::policy::{ManifoldPolicy, PolicyStatus};
use mp_barrier::BarrierConfig;

/// Minimum members for a symmetry class to provide an orbit residual.
/// Mirrors `mp_barrier::orbit::MIN_PEERS`; below this the class is decorative.
pub const MIN_CLASS_SIZE: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Apply a budget measured by `mp-calibrate`.
    SetBudget { bits_squared: f64 },
    /// Clamp a budget change that exceeded `maxBudgetDrift` (`docs/07` F5).
    ClampBudgetDrift { requested: f64, applied: f64 },
    /// Apply the derived alpha.
    SetAlpha { alpha: f64 },
    /// Register a symmetry class.
    RegisterClass { name: String, members: usize },
    /// A class too small to yield an orbit residual.
    WarnUndersizedClass { name: String, members: usize },
    /// No calibration available; running on bootstrap values.
    WarnUncalibrated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileReport {
    pub actions: Vec<Action>,
    pub status: PolicyStatus,
}

impl ReconcileReport {
    pub fn is_degraded(&self) -> bool {
        self.status.phase != "Ready"
    }
}

/// Reconcile one policy.
pub fn reconcile<C: ClusterClient>(
    client: &C,
    policy: &ManifoldPolicy,
    current_budget: f64,
) -> Result<ReconcileReport, ClusterError> {
    policy.spec.validate().map_err(ClusterError::Decode)?;

    let ns = policy.metadata.namespace.clone().unwrap_or_else(|| "default".into());
    let mut actions = Vec::new();
    let mut conditions = Vec::new();

    // Budget: from calibration if available, otherwise flagged.
    let mut calibrated = false;
    let mut budget = current_budget;

    if let Some(cm) = &policy.spec.calibration_config_map {
        match client.get_config_map(&ns, cm) {
            Ok(data) => match data.get("budgetBitsSquared").and_then(|s| s.parse::<f64>().ok()) {
                Some(requested) if requested > 0.0 => {
                    calibrated = true;
                    let max_delta = current_budget * policy.spec.max_budget_drift;
                    let delta = requested - current_budget;

                    if current_budget > 0.0 && delta.abs() > max_delta {
                        // F5: bound how far one window can move the boundary.
                        // A blunt guard, and labelled as one — the principled
                        // fix is a barrier on the parameter trajectory itself.
                        let applied = current_budget + max_delta * delta.signum();
                        actions.push(Action::ClampBudgetDrift { requested, applied });
                        conditions.push(format!(
                            "budget drift clamped: requested {requested:.2}, applied {applied:.2} \
                             (maxBudgetDrift={}) — docs/07 F5",
                            policy.spec.max_budget_drift
                        ));
                        budget = applied;
                    } else {
                        actions.push(Action::SetBudget { bits_squared: requested });
                        budget = requested;
                    }
                }
                _ => {
                    conditions.push(format!(
                        "calibration ConfigMap {cm} has no usable budgetBitsSquared"
                    ));
                    actions.push(Action::WarnUncalibrated);
                }
            },
            Err(ClusterError::NotFound(_)) => {
                conditions.push(format!("calibration ConfigMap {cm} not found"));
                actions.push(Action::WarnUncalibrated);
            }
            Err(e) => return Err(e),
        }
    } else {
        conditions.push("no calibrationConfigMap set; running on bootstrap values".into());
        actions.push(Action::WarnUncalibrated);
    }

    // Alpha, derived from the stated requirement rather than configured.
    let alpha = BarrierConfig::alpha_for_min_steps(
        budget,
        budget * (1.0 - policy.spec.approach_fraction).max(1e-9),
        policy.spec.min_steps_to_boundary,
    );
    actions.push(Action::SetAlpha { alpha });

    // Symmetry classes.
    for class in &policy.spec.symmetry_classes {
        let members = client.count_matching(&ns, &class.selector)?;
        if members < MIN_CLASS_SIZE {
            actions.push(Action::WarnUndersizedClass { name: class.name.clone(), members });
            conditions.push(format!(
                "symmetry class {:?} has {members} members, below the {MIN_CLASS_SIZE} needed \
                 for an orbit residual; members fall back on their own baselines (docs/07 F2)",
                class.name
            ));
        } else {
            actions.push(Action::RegisterClass { name: class.name.clone(), members });
        }
    }

    let phase = if !calibrated {
        "Degraded"
    } else if conditions.is_empty() {
        "Ready"
    } else {
        "ReadyWithWarnings"
    };

    let status = PolicyStatus {
        observed_generation: policy.metadata.generation,
        phase: phase.to_string(),
        budget_bits_squared: budget,
        alpha,
        calibrated,
        conditions,
    };

    client.patch_status(
        &ns,
        &policy.metadata.name,
        &serde_json::to_string(&status).unwrap_or_default(),
    )?;

    Ok(ReconcileReport { actions, status })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::FakeClient;
    use crate::policy::{Metadata, PolicySpec, SymmetryClassSpec};
    use std::collections::BTreeMap;

    fn policy(cm: Option<&str>) -> ManifoldPolicy {
        ManifoldPolicy {
            api_version: "manifoldplane.io/v1alpha1".into(),
            kind: "ManifoldPolicy".into(),
            metadata: Metadata { name: "p".into(), namespace: Some("mp".into()), generation: 1 },
            spec: PolicySpec {
                domain: "kubernetes".into(),
                min_steps_to_boundary: 200.0,
                approach_fraction: 0.99,
                symmetry_classes: vec![SymmetryClassSpec {
                    name: "controllers".into(),
                    selector: [("app".to_string(), "ctrl".to_string())].into_iter().collect(),
                }],
                calibration_config_map: cm.map(|s| s.to_string()),
                max_budget_drift: 0.25,
            },
            status: None,
        }
    }

    fn cal(budget: &str) -> BTreeMap<String, String> {
        [("budgetBitsSquared".to_string(), budget.to_string())].into_iter().collect()
    }

    #[test]
    fn a_calibrated_policy_with_a_healthy_class_becomes_ready() {
        let c = FakeClient::new().with_config_map("mp", "cal", cal("512")).with_count("ctrl", 20);
        let r = reconcile(&c, &policy(Some("cal")), 512.0).unwrap();
        assert_eq!(r.status.phase, "Ready");
        assert!(r.status.calibrated);
        assert_eq!(r.status.budget_bits_squared, 512.0);
        assert!(!r.is_degraded());
    }

    #[test]
    fn a_missing_calibration_yields_degraded_not_a_silent_default() {
        let c = FakeClient::new().with_count("ctrl", 20);
        let r = reconcile(&c, &policy(Some("cal")), 64.0).unwrap();
        assert_eq!(r.status.phase, "Degraded");
        assert!(!r.status.calibrated);
        assert!(r.actions.contains(&Action::WarnUncalibrated));
    }

    #[test]
    fn a_large_budget_jump_is_clamped() {
        // docs/07 F5: an adversary active during the calibration window would
        // otherwise move the boundary in its favour, one plausible window at a
        // time. This bounds the per-window move.
        let c = FakeClient::new().with_config_map("mp", "cal", cal("10000")).with_count("ctrl", 20);
        let r = reconcile(&c, &policy(Some("cal")), 100.0).unwrap();
        assert!(matches!(
            r.actions[0],
            Action::ClampBudgetDrift { requested, applied }
                if requested == 10000.0 && (applied - 125.0).abs() < 1e-9
        ));
        assert_eq!(r.status.budget_bits_squared, 125.0);
        assert_eq!(r.status.phase, "ReadyWithWarnings");
    }

    #[test]
    fn a_downward_budget_jump_is_clamped_too() {
        // Shrinking the budget sharply is also an attack: it pushes every
        // legitimate asker outside Omega at once, and the engine then refuses
        // them all. Denial of service by recalibration.
        let c = FakeClient::new().with_config_map("mp", "cal", cal("1")).with_count("ctrl", 20);
        let r = reconcile(&c, &policy(Some("cal")), 100.0).unwrap();
        assert_eq!(r.status.budget_bits_squared, 75.0);
    }

    #[test]
    fn an_undersized_symmetry_class_is_reported() {
        let c = FakeClient::new().with_config_map("mp", "cal", cal("512")).with_count("ctrl", 2);
        let r = reconcile(&c, &policy(Some("cal")), 512.0).unwrap();
        assert!(r.actions.iter().any(|a| matches!(
            a,
            Action::WarnUndersizedClass { members: 2, .. }
        )));
        assert_eq!(r.status.phase, "ReadyWithWarnings");
        assert!(r.status.conditions.iter().any(|c| c.contains("F2")));
    }

    #[test]
    fn alpha_is_recomputed_when_the_budget_changes() {
        let c = FakeClient::new().with_config_map("mp", "cal", cal("512")).with_count("ctrl", 20);
        let r = reconcile(&c, &policy(Some("cal")), 512.0).unwrap();
        assert!(r.status.alpha > 0.0 && r.status.alpha < 1.0);
        assert!(r.actions.iter().any(|a| matches!(a, Action::SetAlpha { .. })));
    }

    #[test]
    fn status_is_written_back() {
        let c = FakeClient::new().with_config_map("mp", "cal", cal("512")).with_count("ctrl", 20);
        let _ = reconcile(&c, &policy(Some("cal")), 512.0).unwrap();
        assert_eq!(c.patches.borrow().len(), 1);
        assert!(c.patches.borrow()[0].contains("\"phase\":\"Ready\""));
    }

    #[test]
    fn an_invalid_spec_is_refused_before_any_cluster_write() {
        let c = FakeClient::new();
        let mut p = policy(Some("cal"));
        p.spec.symmetry_classes.clear();
        assert!(reconcile(&c, &p, 512.0).is_err());
        assert!(c.patches.borrow().is_empty(), "must not write status for an invalid spec");
    }
}
