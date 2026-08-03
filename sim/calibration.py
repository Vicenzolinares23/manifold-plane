"""Calibrate the metric M and budget c from a benign corpus.

`docs/03-dimensional-analysis.md`, `docs/05-formulation.md` §5.2, §5.9.

Neither M nor c is chosen. M is the shrinkage-regularized inverse covariance of
benign displacements, projected onto the Lyapunov-feasible cone; c is a high
quantile of V over the same corpus. The shrinkage intensity comes from
Ledoit-Wolf in closed form. There are no knobs here, which is the point.
"""

from __future__ import annotations

from manifold import (
    N, NOMINAL_HALF_LIVES, Manifold, Rng, identity, inv_spd, is_feasible,
    project_feasible, quad, quantile, rates_from_half_lives, symmetrize,
)


# Measurement resolution floor, in bits.
#
# A tail-sparse axis breaks the naive fit. Irreversibility is exactly zero for
# ~92% of benign requests, so its sample variance approaches zero, and the
# inverse covariance then assigns it an enormous eigenvalue — an unconfigured
# run produced c = 3.1e8 bits^2 with a diagonal spread of 3.9e8. The geometry
# was numerically valid and physically meaningless.
#
# The fix is dimensional rather than numerical. Every axis is a log-ratio of
# measured counts (docs/03), and those counts have finite resolution: you
# cannot resolve a displacement smaller than the smallest countable change.
# So the covariance cannot legitimately be smaller than the measurement
# quantum, and flooring it is a statement about the instrument, not a
# regularization knob. 0.01 bits is roughly a 0.7% change in a counted set.
RESOLUTION_BITS = 0.01


def covariance(samples):
    n = len(samples)
    if n <= N:
        raise ValueError(f"need more than {N} samples, got {n}")
    mean = [sum(s[i] for s in samples) / n for i in range(N)]
    cov = [[0.0] * N for _ in range(N)]
    for s in samples:
        for i in range(N):
            di = s[i] - mean[i]
            for j in range(N):
                cov[i][j] += di * (s[j] - mean[j])
    for i in range(N):
        for j in range(N):
            cov[i][j] /= n - 1
    # No axis may report variance below the measurement resolution.
    floor = RESOLUTION_BITS ** 2
    for i in range(N):
        if cov[i][i] < floor:
            cov[i][i] = floor
    return symmetrize(cov)


def ledoit_wolf_intensity(samples, cov):
    """Closed-form shrinkage intensity. Computed, never tuned."""
    n = len(samples)
    if n <= 1:
        return 1.0
    mean = [sum(s[i] for s in samples) / n for i in range(N)]
    mu = sum(cov[i][i] for i in range(N)) / N

    d2 = sum(
        (cov[i][j] - (mu if i == j else 0.0)) ** 2 for i in range(N) for j in range(N)
    )
    b2 = 0.0
    for s in samples:
        acc = 0.0
        for i in range(N):
            di = s[i] - mean[i]
            for j in range(N):
                acc += (di * (s[j] - mean[j]) - cov[i][j]) ** 2
        b2 += acc
    b2 /= n * n
    if d2 <= 0:
        return 1.0
    return min(max(b2 / d2, 0.0), 1.0)


def fit(samples, half_lives=NOMINAL_HALF_LIVES, quantile_level=0.999):
    """Fit (M, c) from benign displacements."""
    rates = tuple(rates_from_half_lives(half_lives))
    cov = covariance(samples)
    gamma = ledoit_wolf_intensity(samples, cov)
    mu = sum(cov[i][i] for i in range(N)) / N

    shrunk = [
        [(1 - gamma) * cov[i][j] + (gamma * mu if i == j else 0.0) for j in range(N)]
        for i in range(N)
    ]
    raw = inv_spd(shrunk)
    if raw is None:
        raise ValueError("benign covariance is singular; corpus lacks variation")

    metric = project_feasible(raw, rates)
    budget = quantile([quad(metric, s) for s in samples], quantile_level)
    return metric, budget, gamma, rates


def synth_benign(rng: Rng, n=4000):
    """A synthetic benign corpus with the correlation structure docs/04 predicts.

    Authority and reach are strongly correlated in benign traffic — that is the
    whole reason the fitted metric prices bridge resources as expensive without
    anyone encoding the concept. Irreversibility is tail-sparse: most benign
    requests destroy nothing.
    """
    out = []
    for _ in range(n):
        a = abs(rng.normal(0.0, 1.0))
        h = 0.85 * a + abs(rng.normal(0.0, 0.4))          # correlated with a
        iota = abs(rng.normal(0.0, 0.15)) if rng.uniform() < 0.08 else 0.0
        omega = abs(rng.normal(0.0, 0.2))
        kappa = abs(rng.normal(0.0, 0.3))
        tau = abs(rng.normal(0.0, 0.7))
        out.append([a, h, iota, omega, kappa, tau])
    return out


def variance_captured(samples, metric):
    """Fraction of benign trajectories inside the fitted ellipsoid at c.

    docs/07 F3: the axis-shopping argument holds only as far as the corpus
    represents reality, so under-representation must be visible rather than
    silent.
    """
    vs = [quad(metric, s) for s in samples]
    c = quantile(vs, 0.999)
    return sum(1 for v in vs if v <= c) / len(vs)


def check(label, ok, detail=""):
    print(f"  [{'PASS' if ok else 'FAIL'}] {label}" + (f" — {detail}" if detail else ""))
    return ok


def main() -> int:
    print("calibration (docs/03, docs/05 §5.2 §5.9)\n")
    ok = True
    rng = Rng(0xCA11B)
    samples = synth_benign(rng)

    metric, budget, gamma, rates = fit(samples)
    print(f"  samples={len(samples)}  shrinkage gamma={gamma:.4f}  c={budget:.4f} bits^2\n")

    ok &= check("fitted metric is positive definite and Lyapunov-feasible",
                is_feasible(metric, rates))
    ok &= check("budget is positive", budget > 0, f"c={budget:.4f}")

    frac = variance_captured(samples, metric)
    ok &= check("benign corpus is captured at the 0.999 quantile", frac >= 0.999,
                f"{frac:.4%} inside")

    # False-positive rate is ~0.1% by construction. That makes it a check that
    # calibration ran, not evidence the model is good — docs/07 says so, and
    # reporting it any other way would overclaim.
    fp = 1.0 - frac
    ok &= check("false-positive rate matches the calibration quantile", fp <= 0.0015,
                f"{fp:.4%} (expected ~0.1% by construction, not a quality result)")

    # The metric must have learned the a-h correlation as an off-diagonal term.
    off = metric[0][1]
    ok &= check("metric learned the authority-reach correlation",
                off < -0.05,
                f"M[a][h]={off:.4f} — negative off-diagonal makes violating the "
                f"benign correlation expensive")

    # An identity metric would price every direction alike, which docs/02 N1
    # rejects. Confirm the fit is genuinely anisotropic.
    diag = [metric[i][i] for i in range(N)]
    spread = max(diag) / max(min(diag), 1e-12)
    ok &= check("fitted geometry is anisotropic", spread > 2.0,
                f"diagonal spread {spread:.1f}x")

    # The resolution floor must keep the geometry physically meaningful. An
    # unfloored fit on this corpus produced c = 3.1e8 bits^2, which is not a
    # budget any deployment could reason about.
    ok &= check("resolution floor keeps the budget interpretable",
                budget < 1e4,
                f"c={budget:.2f} bits^2 (unfloored this corpus yields ~3.1e8)")
    ok &= check("resolution floor bounds the conditioning",
                spread < 1e6,
                f"diagonal spread {spread:.1f}x")

    # A bridge-like step (little authority, much reach) must cost more than a
    # benign-correlated step of the same raw length.
    along = [1.0, 0.85, 0, 0, 0, 0]
    across = [1.0, -0.85, 0, 0, 0, 0]
    v_along, v_across = quad(metric, along), quad(metric, across)
    ok &= check("moving against the benign correlation is expensive",
                v_across > 3 * v_along,
                f"V_across={v_across:.3f} vs V_along={v_along:.3f}")

    # Too few samples must fail loudly.
    try:
        fit(samples[:4])
        ok &= check("rejects an undersized corpus", False)
    except ValueError:
        ok &= check("rejects an undersized corpus", True)

    print("\n" + ("all calibration checks passed" if ok else "CALIBRATION CHECKS FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
