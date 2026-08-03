//! Latency of one admission decision.
//!
//! This sits in a request path, so the cost has to be stated as a number rather
//! than asserted to be small. Uses `std::time` rather than a benchmark harness
//! to keep the dependency list at zero — the measurement is coarse but the
//! quantity being measured is sub-microsecond against budgets in milliseconds,
//! so coarse is enough to answer the question.
//!
//! Run: `cargo run --release --bench decision`

use mp_barrier::{Barrier, BarrierConfig, Engine, EngineConfig, Proposal};
use mp_core::linalg::N;
use mp_core::metric::Metric;
use mp_core::state::{AskerId, Relaxation, SymmetryClass};

fn bench(label: &str, iters: u32, mut f: impl FnMut(u32)) {
    // Warm up so the first-call page faults do not land in the measurement.
    for i in 0..1000 {
        f(i);
    }
    let start = std::time::Instant::now();
    for i in 0..iters {
        f(i);
    }
    let elapsed = start.elapsed();
    let per = elapsed.as_nanos() as f64 / iters as f64;
    println!("  {label:<44} {per:>9.0} ns/op   {:>9.0} ops/s", 1e9 / per);
}

fn main() {
    println!("manifold-plane decision latency\n");

    let barrier = Barrier::new(
        Metric::identity(),
        BarrierConfig {
            alpha: 0.02,
            budget: 4096.0,
            review_band: 0.02,
            denial_weight_bits: 0.25,
        },
    )
    .unwrap();

    // The kernel alone: one 6x6 quadratic form.
    let mut step = [0.0; N];
    step[0] = 0.1;
    let z = [1.0, 2.0, 0.5, 0.3, 0.1, 1.5];
    bench("barrier evaluate (kernel only)", 2_000_000, |_| {
        std::hint::black_box(barrier.evaluate(&z, &step));
    });

    bench("saturating scale (adversary step solve)", 2_000_000, |_| {
        std::hint::black_box(barrier.saturating_scale(&z, &step));
    });

    // The full procedure, one asker, no peers.
    let mut engine = Engine::new(barrier, Relaxation::default(), EngineConfig::default());
    let p = Proposal {
        asker: AskerId::new("solo"),
        class: SymmetryClass::new("g"),
        displacement: step,
        at: 0.0,
        label: "bench".into(),
    };
    bench("full decision, 1 asker, no peers", 200_000, |i| {
        let mut q = p.clone();
        q.at = i as f64 * 0.001;
        std::hint::black_box(engine.decide(&q));
    });

    // With a realistic symmetry class. The orbit residual is O(peers), so this
    // is where the cost actually lives.
    for peers in [20usize, 200, 2000] {
        let barrier = Barrier::new(
            Metric::identity(),
            BarrierConfig {
                alpha: 0.02,
                budget: 4096.0,
                review_band: 0.02,
                denial_weight_bits: 0.25,
            },
        )
        .unwrap();
        let mut engine = Engine::new(barrier, Relaxation::default(), EngineConfig::default());
        for i in 0..peers {
            engine.decide(&Proposal {
                asker: AskerId::new(format!("peer{i}")),
                class: SymmetryClass::new("g"),
                displacement: step,
                at: 0.0,
                label: String::new(),
            });
        }
        bench(&format!("full decision, {peers} peers in class"), 20_000, |i| {
            let mut q = p.clone();
            q.at = i as f64 * 0.001;
            std::hint::black_box(engine.decide(&q));
        });
    }

    println!(
        "\nOrbit residual is O(peers) per decision — it takes a median and a MAD\n\
         over the class. At large class sizes that dominates, and the fix is to\n\
         cache the class median between decisions rather than to drop the\n\
         detector. Not implemented; the number is here so the tradeoff is visible."
    );
}
