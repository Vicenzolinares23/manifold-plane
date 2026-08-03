//! `manifold-planed` — the admission daemon.
//!
//! Serves one domain adapter behind the barrier kernel. Routes:
//!
//! ```text
//!   POST /admit      domain-specific request, returns a Verdict
//!   GET  /healthz    liveness
//!   GET  /readyz     readiness — false until the budget is calibrated
//!   GET  /state      per-asker positions, for operators
//!   GET  /metrics    Prometheus text format
//! ```

mod config;
mod http;
mod state;

use config::{Config, Domain};
use mp_barrier::{Barrier, Engine};
use mp_core::metric::Metric;
use mp_core::state::Relaxation;
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "manifold-planed — trajectory-based admission control\n\n\
             usage: manifold-planed [--config PATH] [--print-config]\n\n\
             Without --config, built-in defaults are used and every weakening\n\
             condition is reported at startup. See docs/ for the derivation."
        );
        return;
    }

    if args.iter().any(|a| a == "--print-config") {
        println!(
            "{}",
            serde_json::to_string_pretty(&Config::default()).unwrap()
        );
        return;
    }

    let cfg = match load_config(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fatal: {e}");
            std::process::exit(2);
        }
    };

    if let Err(e) = cfg.validate() {
        eprintln!("fatal: invalid config: {e}");
        std::process::exit(2);
    }

    // Weakening conditions are printed loudly and individually. A deployment
    // may run in any of these states; it should do so knowingly rather than
    // discover it during an incident.
    for w in cfg.warnings() {
        eprintln!("WARNING: {}", w.0);
    }

    let relax = match cfg.half_lives_secs {
        Some(hl) => Relaxation::from_half_lives(&hl),
        None => Relaxation::default(),
    };

    // The identity metric asserts all six axes are interchangeable and
    // uncorrelated, which docs/02 N1 explicitly rejects. It is a bootstrap
    // only, and `mp-calibrate` replaces it with a fitted one.
    let metric = Metric::identity();
    if !metric.is_feasible(relax.rates()) {
        eprintln!("fatal: metric violates the Lyapunov condition (docs/05 §5.6)");
        std::process::exit(2);
    }

    let barrier = match Barrier::new(metric, cfg.barrier_config()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("fatal: {e}");
            std::process::exit(2);
        }
    };

    eprintln!(
        "manifold-planed: domain={:?} listen={} alpha={:.6} budget={:.3} bits^2",
        cfg.domain,
        cfg.listen,
        cfg.alpha(),
        cfg.budget_bits_squared
    );
    eprintln!(
        "  alpha derived from: >= {} admitted requests to reach {:.0}% of budget (docs/06 T2)",
        cfg.min_steps_to_boundary,
        cfg.approach_fraction * 100.0
    );

    let engine = Arc::new(Mutex::new(Engine::new(barrier, relax, cfg.engine_config())));
    let shared = state::Shared::new(cfg.clone(), engine);

    let listen = cfg.listen.clone();
    let max_body = cfg.max_body_bytes;
    let handler_state = shared.clone();

    if let Err(e) = http::serve(&listen, max_body, 256, move |req| {
        state::route(&handler_state, req)
    }) {
        eprintln!("fatal: cannot bind {listen}: {e}");
        std::process::exit(1);
    }
}

fn load_config(args: &[String]) -> Result<Config, String> {
    let mut path: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--config" {
            path = args.get(i + 1).map(|s| s.as_str());
            if path.is_none() {
                return Err("--config requires a path".into());
            }
        }
        i += 1;
    }

    match path {
        None => Ok(Config::default()),
        Some(p) => {
            let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read {p}: {e}"))?;
            Config::from_toml_like(&src)
        }
    }
}

/// Domain is compiled in rather than dispatched per request: one daemon guards
/// one kind of controller, and letting a Kubernetes review arrive at an ICS
/// daemon would silently price it with the wrong `g`.
pub fn domain_name(d: Domain) -> &'static str {
    match d {
        Domain::Kubernetes => "kubernetes",
        Domain::Ics => "ics",
        Domain::Agent => "agent",
    }
}
