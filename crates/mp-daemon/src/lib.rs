//! HTTP surface for the manifold-plane admission engine (`docs/08` §8.3).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mp_adapters::agent::{AgentAdapter, ToolCall as AdapterToolCall, ToolKind};
use mp_adapters::Adapter;
use mp_barrier::{Barrier, BarrierConfig, Decision, Engine, EngineConfig};
use mp_core::axis::nominal_half_lives;
use mp_core::linalg::{self, Vec6, N};
use mp_core::metric::{self, Metric};
use mp_core::state::{AskerId, AskerState, Relaxation, SymmetryClass};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared daemon state.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<RwLock<DaemonInner>>,
}

pub struct DaemonInner {
    pub engine: Engine,
    pub adapter: AgentAdapter,
    pub barrier_cfg: BarrierConfig,
    pub engine_cfg: EngineConfig,
}

impl DaemonInner {
    pub fn new_default() -> Self {
        let barrier_cfg = BarrierConfig {
            alpha: 0.05,
            budget: 100.0,
            review_band: 0.02,
            denial_weight_bits: 0.25,
        };
        let engine_cfg = EngineConfig::default();
        let barrier = Barrier::new(Metric::identity(), barrier_cfg).expect("valid default barrier");
        DaemonInner {
            engine: Engine::new(barrier, Relaxation::default(), engine_cfg),
            adapter: AgentAdapter::new(),
            barrier_cfg,
            engine_cfg,
        }
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/decide", post(decide))
        .route("/v1/askers", get(list_askers))
        .route("/v1/askers/{id}", get(get_asker).put(put_asker))
        .route("/v1/coupling", put(put_coupling))
        .route("/v1/calibrate", post(calibrate))
        .route("/v1/config", get(get_config))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(Debug, Deserialize)]
pub struct DecideRequest {
    pub asker_id: String,
    #[serde(default = "default_symmetry")]
    pub symmetry_class: String,
    pub tool_call: ToolCallBody,
    pub at: f64,
}

fn default_symmetry() -> String {
    "default".into()
}

#[derive(Debug, Deserialize)]
pub struct ToolCallBody {
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

fn default_sensitivity() -> f64 {
    0.01
}

#[derive(Debug, Serialize)]
pub struct DecideResponse {
    pub decision: String,
    pub admissible_fraction: f64,
    pub coalitions_checked: usize,
    pub blocked_by_coalition: Option<usize>,
    pub margin_before: f64,
    pub margin_after: f64,
    pub required: f64,
    pub alpha_effective: f64,
    pub orbit_residual: f64,
    pub budget_fraction: f64,
    pub state_after: [f64; N],
    pub denied: u64,
    pub held: u64,
    pub admitted: u64,
}

fn parse_kind(s: &str) -> Result<ToolKind, ApiError> {
    match s {
        "ReadLocal" => Ok(ToolKind::ReadLocal),
        "ReadExternal" => Ok(ToolKind::ReadExternal),
        "WriteLocal" => Ok(ToolKind::WriteLocal),
        "SendExternal" => Ok(ToolKind::SendExternal),
        "Execute" => Ok(ToolKind::Execute),
        "SelfModify" => Ok(ToolKind::SelfModify),
        "Delegate" => Ok(ToolKind::Delegate),
        other => Err(ApiError::bad(format!("unknown tool kind: {other}"))),
    }
}

async fn decide(
    State(state): State<AppState>,
    Json(body): Json<DecideRequest>,
) -> Result<Json<DecideResponse>, ApiError> {
    let kind = parse_kind(&body.tool_call.kind)?;
    let call = AdapterToolCall {
        kind,
        payload_bytes: body.tool_call.payload_bytes,
        recipients: body.tool_call.recipients,
        argument_tainted: body.tool_call.argument_tainted,
        off_transcript: body.tool_call.off_transcript,
        source_sensitivity: body.tool_call.source_sensitivity,
    };

    let mut inner = state.inner.write().await;
    let asker = AskerId::new(body.asker_id.clone());
    let class = SymmetryClass::new(body.symmetry_class.clone());
    let current = inner.engine.displacement_at(&asker, body.at);
    let displacement = inner.adapter.displacement(&call, &current).into_vec();

    let proposal = mp_barrier::Proposal {
        asker: asker.clone(),
        class,
        displacement,
        at: body.at,
        label: format!("{:?}", kind),
    };
    let outcome = inner.engine.decide(&proposal);
    let st = inner
        .engine
        .state(&asker)
        .ok_or_else(|| ApiError::internal("asker missing after decide"))?;

    Ok(Json(DecideResponse {
        decision: outcome.verdict.decision.as_str().to_string(),
        admissible_fraction: outcome.admissible_fraction,
        coalitions_checked: outcome.coalitions_checked,
        blocked_by_coalition: outcome.verdict.blocked_by_coalition,
        margin_before: outcome.verdict.margin_before,
        margin_after: outcome.verdict.margin_after,
        required: outcome.verdict.required,
        alpha_effective: outcome.verdict.alpha_effective,
        orbit_residual: outcome.verdict.orbit_residual,
        budget_fraction: outcome.verdict.budget_fraction,
        state_after: st.z,
        denied: st.denied,
        held: st.held,
        admitted: st.admitted,
    }))
}

#[derive(Debug, Serialize)]
pub struct AskerOut {
    pub asker_id: String,
    pub symmetry_class: String,
    pub z: [f64; N],
    pub last_seen: f64,
    pub admitted: u64,
    pub denied: u64,
    pub held: u64,
    pub relaxed_z: [f64; N],
}

#[derive(Debug, Serialize)]
pub struct AskersList {
    pub askers: Vec<AskerOut>,
}

fn asker_out(engine: &Engine, st: &AskerState, now: f64) -> AskerOut {
    AskerOut {
        asker_id: st.id.as_str().to_string(),
        symmetry_class: st.class.as_str().to_string(),
        z: st.z,
        last_seen: st.last_seen,
        admitted: st.admitted,
        denied: st.denied,
        held: st.held,
        relaxed_z: engine.displacement_at(&st.id, now),
    }
}

async fn list_askers(State(state): State<AppState>) -> Json<AskersList> {
    let inner = state.inner.read().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let askers = inner
        .engine
        .askers()
        .map(|st| asker_out(&inner.engine, st, now))
        .collect();
    Json(AskersList { askers })
}

async fn get_asker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AskerOut>, ApiError> {
    let inner = state.inner.read().await;
    let asker = AskerId::new(id);
    let st = inner
        .engine
        .state(&asker)
        .ok_or_else(|| ApiError::not_found("asker not found"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Ok(Json(asker_out(&inner.engine, st, now)))
}

#[derive(Debug, Deserialize)]
pub struct PutAskerBody {
    #[serde(default = "default_symmetry")]
    pub symmetry_class: String,
    #[serde(default = "zero_z")]
    pub z: [f64; N],
    pub last_seen: f64,
    #[serde(default)]
    pub admitted: u64,
    #[serde(default)]
    pub denied: u64,
    #[serde(default)]
    pub held: u64,
}

fn zero_z() -> [f64; N] {
    [0.0; N]
}

async fn put_asker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PutAskerBody>,
) -> Json<AskerOut> {
    let mut inner = state.inner.write().await;
    let asker = AskerId::new(id);
    let mut st = AskerState::new(
        asker.clone(),
        SymmetryClass::new(body.symmetry_class),
        body.last_seen,
    );
    st.z = body.z;
    st.admitted = body.admitted;
    st.denied = body.denied;
    st.held = body.held;
    inner.engine.upsert_asker(st);
    let st = inner.engine.state(&asker).expect("just upserted");
    Json(asker_out(&inner.engine, st, body.last_seen))
}

#[derive(Debug, Deserialize)]
pub struct CouplingBody {
    pub a: String,
    pub b: String,
    pub kappa_bits: f64,
}

async fn put_coupling(
    State(state): State<AppState>,
    Json(body): Json<CouplingBody>,
) -> StatusCode {
    let mut inner = state.inner.write().await;
    inner.engine.graph_mut().set_coupling(
        &AskerId::new(body.a),
        &AskerId::new(body.b),
        body.kappa_bits,
    );
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
pub struct CalibrateBody {
    pub samples: Vec<[f64; N]>,
    #[serde(default = "default_quantile")]
    pub quantile: f64,
}

fn default_quantile() -> f64 {
    0.999
}

#[derive(Debug, Serialize)]
pub struct CalibrateResponse {
    pub metric: [[f64; N]; N],
    pub budget: f64,
    pub projection_distance: f64,
    pub alpha: f64,
    pub review_band: f64,
}

async fn calibrate(
    State(state): State<AppState>,
    Json(body): Json<CalibrateBody>,
) -> Result<Json<CalibrateResponse>, ApiError> {
    if body.samples.len() < 2 {
        return Err(ApiError::bad("need at least 2 samples to calibrate"));
    }
    let rates = mp_core::axis::rates_from_half_lives(&nominal_half_lives());
    let samples: Vec<Vec6> = body.samples;
    let fitted = metric::fit(&samples, &rates).map_err(|e| ApiError::bad(format!("{e:?}")))?;
    let budget = metric::calibrate_budget(&fitted, &samples, body.quantile).max(1e-6);

    let mut inner = state.inner.write().await;
    inner.barrier_cfg.budget = budget;
    let barrier = Barrier::new(fitted.clone(), inner.barrier_cfg)
        .map_err(|e| ApiError::bad(e))?;
    inner.engine.set_barrier(barrier);

    Ok(Json(CalibrateResponse {
        metric: *fitted.as_matrix(),
        budget,
        projection_distance: fitted.projection_distance(),
        alpha: inner.barrier_cfg.alpha,
        review_band: inner.barrier_cfg.review_band,
    }))
}

#[derive(Debug, Serialize)]
pub struct ConfigOut {
    pub barrier: BarrierConfigOut,
    pub engine: EngineConfigOut,
}

#[derive(Debug, Serialize)]
pub struct BarrierConfigOut {
    pub alpha: f64,
    pub budget: f64,
    pub review_band: f64,
    pub denial_weight_bits: f64,
}

#[derive(Debug, Serialize)]
pub struct EngineConfigOut {
    pub kappa_min: f64,
    pub max_coalition: usize,
    pub idle_evict_secs: f64,
}

async fn get_config(State(state): State<AppState>) -> Json<ConfigOut> {
    let inner = state.inner.read().await;
    Json(ConfigOut {
        barrier: BarrierConfigOut {
            alpha: inner.barrier_cfg.alpha,
            budget: inner.barrier_cfg.budget,
            review_band: inner.barrier_cfg.review_band,
            denial_weight_bits: inner.barrier_cfg.denial_weight_bits,
        },
        engine: EngineConfigOut {
            kappa_min: inner.engine_cfg.kappa_min,
            max_coalition: inner.engine_cfg.max_coalition,
            idle_evict_secs: inner.engine_cfg.idle_evict_secs,
        },
    })
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad(msg: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    fn not_found(msg: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    fn internal(msg: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}

/// Silence unused Decision import warning in some builds — keep for tests.
#[allow(dead_code)]
fn _decision_str(d: Decision) -> &'static str {
    d.as_str()
}

#[allow(dead_code)]
fn _zero() -> Vec6 {
    linalg::ZERO_V
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app() -> Router {
        let state = AppState {
            inner: Arc::new(RwLock::new(DaemonInner::new_default())),
        };
        app(state)
    }

    async fn json_body(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = test_app();
        let res = app
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn small_read_local_is_admitted() {
        let app = test_app();
        let body = serde_json::json!({
            "asker_id": "t1",
            "symmetry_class": "default",
            "tool_call": {"kind": "ReadLocal", "payload_bytes": 64},
            "at": 1.0
        });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/decide")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_body(res).await;
        assert_eq!(v["decision"], "admit");
        assert_eq!(v["admitted"], 1);
    }

    #[tokio::test]
    async fn huge_execute_is_denied_or_held() {
        // Tight budget so a large Execute is refused.
        let state = AppState {
            inner: Arc::new(RwLock::new(DaemonInner::new_default())),
        };
        {
            let mut inner = state.inner.write().await;
            inner.barrier_cfg.budget = 5.0;
            let b = Barrier::new(Metric::identity(), inner.barrier_cfg).unwrap();
            inner.engine.set_barrier(b);
        }
        let app = super::app(state);
        let body = serde_json::json!({
            "asker_id": "attacker",
            "tool_call": {
                "kind": "Execute",
                "payload_bytes": 1_000_000,
                "argument_tainted": true,
                "source_sensitivity": 1.0
            },
            "at": 1.0
        });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/decide")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = json_body(res).await;
        let d = v["decision"].as_str().unwrap();
        assert!(d == "deny" || d == "hold", "got {d}");
    }

    #[tokio::test]
    async fn asker_seed_round_trip() {
        let app = test_app();
        let put = serde_json::json!({
            "symmetry_class": "replicas",
            "z": [0.1, 0.2, 0.0, 0.0, 0.0, 0.0],
            "last_seen": 42.0,
            "admitted": 3
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/askers/seeded")
                    .header("content-type", "application/json")
                    .body(Body::from(put.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/v1/askers/seeded")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = json_body(res).await;
        assert_eq!(v["asker_id"], "seeded");
        assert_eq!(v["symmetry_class"], "replicas");
        assert_eq!(v["admitted"], 3);
        assert!((v["z"][0].as_f64().unwrap() - 0.1).abs() < 1e-9);
    }
}
