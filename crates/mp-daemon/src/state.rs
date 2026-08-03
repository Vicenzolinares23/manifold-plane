//! Shared daemon state, routing, and the decision log.

use crate::config::{Config, Domain};
use crate::http::{Request, Response};
use mp_adapters::{agent, ics, k8s, Adapter};
use mp_barrier::{Decision, Engine, Proposal};
use mp_core::state::{AskerId, SymmetryClass};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct Counters {
    pub admitted: AtomicU64,
    pub held: AtomicU64,
    pub denied: AtomicU64,
    pub errors: AtomicU64,
    pub coalition_blocks: AtomicU64,
}

#[derive(Clone)]
pub struct Shared {
    pub cfg: Arc<Config>,
    pub engine: Arc<Mutex<Engine>>,
    pub counters: Arc<Counters>,
}

impl Shared {
    pub fn new(cfg: Config, engine: Arc<Mutex<Engine>>) -> Self {
        Shared { cfg: Arc::new(cfg), engine, counters: Arc::new(Counters::default()) }
    }
}

/// Wire format for an admission request.
///
/// One envelope for all three domains, with exactly one payload populated. The
/// alternative — three daemons with three schemas — would duplicate the whole
/// serving path for no gain, since the kernel is identical.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmitBody {
    pub asker: String,
    /// Symmetry class. `docs/07` F2: an asker in a singleton class has no orbit
    /// residual and falls back entirely on its own baseline, which is the
    /// weakest configuration available.
    pub class: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub kubernetes: Option<K8sBody>,
    #[serde(default)]
    pub ics: Option<IcsBody>,
    #[serde(default)]
    pub agent: Option<AgentBody>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct K8sBody {
    pub verb: String,
    pub resource: String,
    #[serde(default = "one")]
    pub namespace_span: u32,
    #[serde(default)]
    pub grants_permissions: bool,
    #[serde(default)]
    pub granted_verb_count: u32,
    #[serde(default)]
    pub evades_audit: bool,
    #[serde(default)]
    pub destroys_state: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcsBody {
    pub function: String,
    pub criticality: String,
    #[serde(default = "one")]
    pub point_count: u32,
    #[serde(default)]
    pub excursion_fraction: f64,
    #[serde(default)]
    pub outside_historical_envelope: bool,
    #[serde(default)]
    pub bypasses_sbo: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentBody {
    pub kind: String,
    #[serde(default)]
    pub payload_bytes: u64,
    #[serde(default)]
    pub recipients: u32,
    #[serde(default)]
    pub argument_tainted: bool,
    #[serde(default)]
    pub off_transcript: bool,
    #[serde(default = "default_sensitivity")]
    pub source_sensitivity: f64,
}

fn one() -> u32 {
    1
}
fn default_sensitivity() -> f64 {
    0.01
}

#[derive(Debug, Serialize)]
pub struct AdmitResponse {
    pub decision: &'static str,
    pub allowed: bool,
    /// Everything an operator needs to check the decision by hand against
    /// `docs/06`. "The model said so" is not an explanation.
    pub margin_before: f64,
    pub margin_after: f64,
    pub required: f64,
    pub alpha_effective: f64,
    pub orbit_residual: f64,
    pub budget_fraction: f64,
    pub admissible_fraction: f64,
    pub blocked_by_coalition: Option<usize>,
    pub reason: String,
}

fn parse_verb(s: &str) -> Option<k8s::Verb> {
    use k8s::Verb::*;
    Some(match s.to_ascii_lowercase().as_str() {
        "get" => Get,
        "list" => List,
        "watch" => Watch,
        "create" => Create,
        "update" => Update,
        "patch" => Patch,
        "delete" => Delete,
        "deletecollection" => DeleteCollection,
        "exec" => Exec,
        "impersonate" => Impersonate,
        _ => return None,
    })
}

fn parse_resource(s: &str) -> k8s::ResourceClass {
    use k8s::ResourceClass::*;
    match s.to_ascii_lowercase().as_str() {
        "pod" | "pods" => Pod,
        "secret" | "secrets" => Secret,
        "configmap" | "configmaps" => ConfigMap,
        "serviceaccount" | "serviceaccounts" => ServiceAccount,
        "role" | "roles" => Role,
        "clusterrole" | "clusterroles" => ClusterRole,
        "rolebinding" | "rolebindings" => RoleBinding,
        "clusterrolebinding" | "clusterrolebindings" => ClusterRoleBinding,
        "deployment" | "statefulset" | "daemonset" | "job" | "cronjob" => Workload,
        "node" | "nodes" => Node,
        "persistentvolume" | "persistentvolumeclaim" => PersistentVolume,
        "validatingwebhookconfiguration" | "mutatingwebhookconfiguration" => WebhookConfig,
        // Unknown resource classes price as `Other`, the cheapest class. That
        // is a real gap — a CRD conferring broad power reads as harmless. The
        // operator maps its CRDs explicitly, and `docs/07` should carry this.
        _ => Other,
    }
}

fn parse_ics_function(s: &str) -> Option<ics::FunctionCode> {
    use ics::FunctionCode::*;
    Some(match s.to_ascii_lowercase().as_str() {
        "readcoils" => ReadCoils,
        "readdiscreteinputs" => ReadDiscreteInputs,
        "readholdingregisters" => ReadHoldingRegisters,
        "readinputregisters" => ReadInputRegisters,
        "writesinglecoil" => WriteSingleCoil,
        "writesingleregister" => WriteSingleRegister,
        "writemultiplecoils" => WriteMultipleCoils,
        "writemultipleregisters" => WriteMultipleRegisters,
        "directoperate" => DirectOperate,
        "select" => Select,
        "diagnostic" => Diagnostic,
        "configwrite" => ConfigWrite,
        _ => return None,
    })
}

fn parse_criticality(s: &str) -> Option<ics::PointCriticality> {
    use ics::PointCriticality::*;
    Some(match s.to_ascii_lowercase().as_str() {
        "telemetry" => Telemetry,
        "interlocked" => Interlocked,
        "direct" => Direct,
        "safetyfunction" | "safety" => SafetyFunction,
        _ => return None,
    })
}

fn parse_tool_kind(s: &str) -> Option<agent::ToolKind> {
    use agent::ToolKind::*;
    Some(match s.to_ascii_lowercase().as_str() {
        "readlocal" => ReadLocal,
        "readexternal" => ReadExternal,
        "writelocal" => WriteLocal,
        "sendexternal" => SendExternal,
        "execute" => Execute,
        "selfmodify" => SelfModify,
        "delegate" => Delegate,
        _ => return None,
    })
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn route(s: &Shared, req: Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/healthz") => Response::text(200, "ok\n"),
        ("GET", "/readyz") => {
            // Not ready until the budget was calibrated. Serving decisions
            // against an arbitrary boundary and reporting healthy would be a
            // lie the deployment acts on.
            if s.cfg.budget_is_calibrated {
                Response::text(200, "ready\n")
            } else {
                Response::text(503, "budget not calibrated (docs/03)\n")
            }
        }
        ("GET", "/metrics") => Response::text(200, metrics(s)),
        ("GET", "/state") => state_dump(s),
        ("POST", "/admit") => admit(s, &req.body),
        _ => Response::not_found(),
    }
}

fn admit(s: &Shared, body: &[u8]) -> Response {
    let parsed: AdmitBody = match serde_json::from_slice(body) {
        Ok(b) => b,
        Err(e) => {
            s.counters.errors.fetch_add(1, Ordering::Relaxed);
            return Response::json(
                400,
                serde_json::json!({ "error": format!("bad request: {e}") }).to_string(),
            );
        }
    };

    let now = now_secs();
    let asker = AskerId::new(parsed.asker.clone());
    let class = SymmetryClass::new(parsed.class.clone());

    let current = {
        let e = s.engine.lock().expect("engine mutex");
        e.displacement_at(&asker, now)
    };

    let displacement = match (s.cfg.domain, &parsed) {
        (Domain::Kubernetes, p) => {
            let Some(b) = &p.kubernetes else {
                return bad(s, "kubernetes payload required for this domain");
            };
            let Some(verb) = parse_verb(&b.verb) else {
                return bad(s, "unknown verb");
            };
            k8s::K8sAdapter::new(64)
                .displacement(
                    &k8s::AdmissionRequest {
                        verb,
                        resource: parse_resource(&b.resource),
                        namespace_span: b.namespace_span,
                        grants_permissions: b.grants_permissions,
                        granted_verb_count: b.granted_verb_count,
                        evades_audit: b.evades_audit,
                        destroys_state: b.destroys_state,
                    },
                    &current,
                )
                .into_vec()
        }
        (Domain::Ics, p) => {
            let Some(b) = &p.ics else {
                return bad(s, "ics payload required for this domain");
            };
            let (Some(function), Some(criticality)) =
                (parse_ics_function(&b.function), parse_criticality(&b.criticality))
            else {
                return bad(s, "unknown function or criticality");
            };
            ics::IcsAdapter::new(256)
                .displacement(
                    &ics::IcsRequest {
                        function,
                        criticality,
                        point_count: b.point_count,
                        excursion_fraction: b.excursion_fraction,
                        outside_historical_envelope: b.outside_historical_envelope,
                        bypasses_sbo: b.bypasses_sbo,
                    },
                    &current,
                )
                .into_vec()
        }
        (Domain::Agent, p) => {
            let Some(b) = &p.agent else {
                return bad(s, "agent payload required for this domain");
            };
            let Some(kind) = parse_tool_kind(&b.kind) else {
                return bad(s, "unknown tool kind");
            };
            agent::AgentAdapter::new()
                .displacement(
                    &agent::ToolCall {
                        kind,
                        payload_bytes: b.payload_bytes,
                        recipients: b.recipients,
                        argument_tainted: b.argument_tainted,
                        off_transcript: b.off_transcript,
                        source_sensitivity: b.source_sensitivity,
                    },
                    &current,
                )
                .into_vec()
        }
    };

    let proposal = Proposal {
        asker: asker.clone(),
        class,
        displacement,
        at: now,
        label: parsed.label.clone(),
    };

    let outcome = {
        let mut e = s.engine.lock().expect("engine mutex");
        e.decide(&proposal)
    };

    let v = outcome.verdict;
    match v.decision {
        Decision::Admit => s.counters.admitted.fetch_add(1, Ordering::Relaxed),
        Decision::Hold => s.counters.held.fetch_add(1, Ordering::Relaxed),
        Decision::Deny => s.counters.denied.fetch_add(1, Ordering::Relaxed),
    };
    if v.blocked_by_coalition.is_some() {
        s.counters.coalition_blocks.fetch_add(1, Ordering::Relaxed);
    }

    let reason = explain(&v);

    if s.cfg.log_decisions {
        eprintln!(
            "{} asker={} label={:?} h={:.4}->{:.4} req={:.4} a_eff={:.6} rho={:.2}",
            v.decision.as_str(),
            asker,
            outcome.label,
            v.margin_before,
            v.margin_after,
            v.required,
            v.alpha_effective,
            v.orbit_residual
        );
    }

    Response::json(
        200,
        serde_json::to_string(&AdmitResponse {
            decision: v.decision.as_str(),
            allowed: v.decision == Decision::Admit,
            margin_before: v.margin_before,
            margin_after: v.margin_after,
            required: v.required,
            alpha_effective: v.alpha_effective,
            orbit_residual: v.orbit_residual,
            budget_fraction: v.budget_fraction,
            admissible_fraction: outcome.admissible_fraction,
            blocked_by_coalition: v.blocked_by_coalition,
            reason,
        })
        .unwrap_or_else(|_| "{}".into()),
    )
}

/// Human-readable explanation of a verdict.
fn explain(v: &mp_barrier::Verdict) -> String {
    match v.decision {
        Decision::Admit => format!(
            "step leaves margin {:.4}, at or above the required {:.4}",
            v.margin_after, v.required
        ),
        Decision::Hold => format!(
            "step leaves margin {:.4}, below the required {:.4} but within the review band",
            v.margin_after, v.required
        ),
        Decision::Deny => {
            if let Some(n) = v.blocked_by_coalition {
                format!(
                    "blocked by a coalition of {n} coupled askers: jointly the step leaves \
                     margin {:.4}, below the required {:.4} (docs/05 §5.7)",
                    v.margin_after, v.required
                )
            } else if v.margin_before < 0.0 {
                "asker is already outside the safe set; refusing rather than re-deriving \
                 a margin from an invalid position"
                    .to_string()
            } else {
                format!(
                    "step would leave margin {:.4}, below the required {:.4} at \
                     alpha_eff={:.6} (docs/06 T1)",
                    v.margin_after, v.required, v.alpha_effective
                )
            }
        }
    }
}

fn bad(s: &Shared, msg: &str) -> Response {
    s.counters.errors.fetch_add(1, Ordering::Relaxed);
    Response::json(400, serde_json::json!({ "error": msg }).to_string())
}

fn metrics(s: &Shared) -> String {
    let c = &s.counters;
    let askers = s.engine.lock().map(|e| e.asker_count()).unwrap_or(0);
    format!(
        "# HELP manifold_plane_decisions_total Admission decisions by outcome.\n\
         # TYPE manifold_plane_decisions_total counter\n\
         manifold_plane_decisions_total{{decision=\"admit\"}} {}\n\
         manifold_plane_decisions_total{{decision=\"hold\"}} {}\n\
         manifold_plane_decisions_total{{decision=\"deny\"}} {}\n\
         # HELP manifold_plane_coalition_blocks_total Denials attributable to coalition separation.\n\
         # TYPE manifold_plane_coalition_blocks_total counter\n\
         manifold_plane_coalition_blocks_total {}\n\
         # HELP manifold_plane_request_errors_total Malformed or unroutable requests.\n\
         # TYPE manifold_plane_request_errors_total counter\n\
         manifold_plane_request_errors_total {}\n\
         # HELP manifold_plane_tracked_askers Askers with carried state.\n\
         # TYPE manifold_plane_tracked_askers gauge\n\
         manifold_plane_tracked_askers {}\n\
         # HELP manifold_plane_budget_calibrated Whether c was measured from a benign corpus.\n\
         # TYPE manifold_plane_budget_calibrated gauge\n\
         manifold_plane_budget_calibrated {}\n",
        c.admitted.load(Ordering::Relaxed),
        c.held.load(Ordering::Relaxed),
        c.denied.load(Ordering::Relaxed),
        c.coalition_blocks.load(Ordering::Relaxed),
        c.errors.load(Ordering::Relaxed),
        askers,
        if s.cfg.budget_is_calibrated { 1 } else { 0 },
    )
}

fn state_dump(s: &Shared) -> Response {
    let e = match s.engine.lock() {
        Ok(e) => e,
        Err(_) => return Response::json(500, r#"{"error":"engine poisoned"}"#),
    };
    Response::json(
        200,
        serde_json::json!({ "tracked_askers": e.asker_count() }).to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_barrier::{Barrier, EngineConfig};
    use mp_core::metric::Metric;
    use mp_core::state::Relaxation;

    /// A daemon with a budget in the range a real calibration produces.
    ///
    /// The built-in default of 64 bits^2 is a bootstrap and is deliberately far
    /// too strict for the adapter displacement scales — see
    /// `the_bootstrap_default_budget_denies_almost_everything` below. Tests that
    /// exercise behavior rather than the bootstrap use a realistic budget.
    fn shared(domain: Domain) -> Shared {
        let cfg = Config {
            domain,
            log_decisions: false,
            budget_bits_squared: 4096.0,
            budget_is_calibrated: true,
            ..Default::default()
        };
        let barrier = Barrier::new(Metric::identity(), cfg.barrier_config()).unwrap();
        let engine =
            Engine::new(barrier, Relaxation::default(), EngineConfig::default());
        Shared::new(cfg, Arc::new(Mutex::new(engine)))
    }

    fn post(s: &Shared, body: &str) -> Response {
        route(s, Request { method: "POST".into(), path: "/admit".into(), body: body.into() })
    }

    #[test]
    fn health_is_always_up() {
        let s = shared(Domain::Kubernetes);
        let r = route(&s, Request { method: "GET".into(), path: "/healthz".into(), body: vec![] });
        assert_eq!(r.status, 200);
    }

    #[test]
    fn readiness_is_false_until_the_budget_is_calibrated() {
        // Reporting ready while deciding against an arbitrary boundary would be
        // a lie the deployment acts on.
        let cfg = Config { log_decisions: false, ..Default::default() };
        assert!(!cfg.budget_is_calibrated);
        let barrier = Barrier::new(Metric::identity(), cfg.barrier_config()).unwrap();
        let engine = Engine::new(barrier, Relaxation::default(), EngineConfig::default());
        let s = Shared::new(cfg, Arc::new(Mutex::new(engine)));
        let r = route(&s, Request { method: "GET".into(), path: "/readyz".into(), body: vec![] });
        assert_eq!(r.status, 503);
    }

    #[test]
    fn the_bootstrap_default_budget_denies_almost_everything() {
        // Documents a real property of the uncalibrated default rather than
        // hiding it. With c = 64 bits^2 and alpha derived for 200 steps, the
        // per-step allowance is ~1.5 bits^2, while a single ClusterRoleBinding
        // grant is ~39. So the bootstrap fails closed on ordinary traffic.
        //
        // That is the safe direction to be wrong in, and it is why /readyz
        // reports 503 until a real calibration has run. An operator who ignores
        // both the startup warning and the readiness probe gets a daemon that
        // refuses everything, which is loud rather than silent.
        let cfg = Config { log_decisions: false, ..Default::default() };
        let barrier = Barrier::new(Metric::identity(), cfg.barrier_config()).unwrap();
        let engine = Engine::new(barrier, Relaxation::default(), EngineConfig::default());
        let s = Shared::new(cfg, Arc::new(Mutex::new(engine)));

        let r = post(
            &s,
            r#"{"asker":"x","class":"c","kubernetes":{"verb":"create",
                "resource":"clusterrolebinding","grants_permissions":true,
                "granted_verb_count":32}}"#,
        );
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("\"allowed\":false"), "{body}");
    }

    #[test]
    fn a_benign_kubernetes_read_is_admitted() {
        let s = shared(Domain::Kubernetes);
        let r = post(
            &s,
            r#"{"asker":"sa1","class":"controllers","kubernetes":{"verb":"get","resource":"configmap"}}"#,
        );
        assert_eq!(r.status, 200);
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("\"decision\":\"admit\""), "{body}");
    }

    #[test]
    fn an_escalation_chain_is_eventually_refused() {
        // Individually-fine Kubernetes steps, repeated. A memoryless policy
        // engine admits every one of these forever.
        let s = shared(Domain::Kubernetes);
        let mut decisions = Vec::new();
        for _ in 0..40 {
            let r = post(
                &s,
                r#"{"asker":"climber","class":"controllers","kubernetes":
                    {"verb":"create","resource":"clusterrolebinding",
                     "grants_permissions":true,"granted_verb_count":32}}"#,
            );
            let body = String::from_utf8(r.body).unwrap();
            decisions.push(body.contains("\"allowed\":true"));
        }
        assert!(decisions[0], "the first step should be allowed");
        assert!(decisions.iter().any(|d| !d), "the chain should eventually be refused");
    }

    #[test]
    fn a_wrong_domain_payload_is_rejected() {
        let s = shared(Domain::Ics);
        let r = post(
            &s,
            r#"{"asker":"x","class":"c","kubernetes":{"verb":"get","resource":"pod"}}"#,
        );
        assert_eq!(r.status, 400);
    }

    #[test]
    fn an_unknown_verb_is_rejected_rather_than_defaulted() {
        // Defaulting an unrecognized verb to something cheap would let an
        // attacker pick a verb spelling the parser does not know.
        let s = shared(Domain::Kubernetes);
        let r = post(
            &s,
            r#"{"asker":"x","class":"c","kubernetes":{"verb":"frobnicate","resource":"pod"}}"#,
        );
        assert_eq!(r.status, 400);
    }

    #[test]
    fn unknown_json_fields_are_rejected() {
        let s = shared(Domain::Kubernetes);
        let r = post(&s, r#"{"asker":"x","class":"c","surprise":1}"#);
        assert_eq!(r.status, 400);
    }

    #[test]
    fn every_verdict_carries_an_explanation() {
        let s = shared(Domain::Agent);
        let r = post(
            &s,
            r#"{"asker":"bot","class":"agents","agent":{"kind":"sendexternal",
                "payload_bytes":50000000,"recipients":900,"argument_tainted":true,
                "source_sensitivity":1.0}}"#,
        );
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("\"reason\":"), "{body}");
        assert!(body.contains("docs/"), "explanation should cite the derivation: {body}");
    }

    #[test]
    fn metrics_render_in_prometheus_format() {
        let s = shared(Domain::Kubernetes);
        let _ = post(
            &s,
            r#"{"asker":"a","class":"c","kubernetes":{"verb":"get","resource":"pod"}}"#,
        );
        let out = metrics(&s);
        assert!(out.contains("# TYPE manifold_plane_decisions_total counter"));
        assert!(out.contains("manifold_plane_decisions_total{decision=\"admit\"} 1"));
    }

    #[test]
    fn an_ics_safety_write_is_refused_from_a_loaded_position() {
        let s = shared(Domain::Ics);
        let body = r#"{"asker":"plc1","class":"plcs","ics":{"function":"writesingleregister",
            "criticality":"safetyfunction","excursion_fraction":1.0,"point_count":1}}"#;
        let first = String::from_utf8(post(&s, body).body).unwrap();
        // A full-range write to a safety function is 20 bits of irreversibility
        // against a 64 bits^2 budget: it should not pass on its own.
        assert!(first.contains("\"allowed\":false"), "{first}");
    }
}
