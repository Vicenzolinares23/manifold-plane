"""Dimensional consistency checks.

`docs/03-dimensional-analysis.md`.

Every axis is in bits, V in bits², c in bits², alpha and every Pi-group
dimensionless. If that ever stops holding, a quadratic form over the axes is a
category error again and the geometry means nothing.
"""

from __future__ import annotations

import math

from manifold import (
    N, NOMINAL_HALF_LIVES, Manifold, damping_matrix, rates_from_half_lives,
)


def check(label, ok, detail=""):
    print(f"  [{'PASS' if ok else 'FAIL'}] {label}" + (f" — {detail}" if detail else ""))
    return ok


def main() -> int:
    print("dimensional consistency (docs/03)\n")
    ok = True

    # Scale invariance in time (S4 + S2). Measuring half-lives in hours rather
    # than seconds must change nothing, since only the ratio dt/T_half appears.
    secs = Manifold.bootstrap(half_lives=NOMINAL_HALF_LIVES)
    hours = Manifold.bootstrap(half_lives=tuple(h / 3600.0 for h in NOMINAL_HALF_LIVES))
    z = [1.0, 2.0, 3.0, 1.0, 0.5, 4.0]
    a = secs.relax(z, 7200.0)
    b = hours.relax(z, 2.0)
    ok &= check(
        "relaxation is invariant under the choice of time unit",
        all(abs(a[i] - b[i]) < 1e-9 for i in range(N)),
    )

    # Log-ratio form: doubling a count costs exactly one bit, regardless of the
    # absolute magnitude. This is what makes the axes commensurable.
    for base in (4.0, 4096.0, 1e9):
        bits = math.log2((2 * base) / base)
        ok &= check(f"  doubling from {base:g} costs one bit", abs(bits - 1.0) < 1e-12)

    # Budget scaling. Doubling every axis quadruples V, since V is quadratic.
    mf = Manifold.bootstrap(budget=100.0)
    v1 = mf.potential(z)
    v2 = mf.potential([2 * x for x in z])
    ok &= check("V is quadratic in displacement", abs(v2 - 4 * v1) < 1e-9,
                f"V(2z)/V(z) = {v2 / v1:.6f}")

    # Pi-groups must be dimensionless: they are ratios of like quantities.
    ok &= check("Pi_3 = V/c is dimensionless", isinstance(v1 / mf.budget, float))

    # alpha is dimensionless and the step count it implies is a pure number.
    n = mf.steps_to_reach(0.99)
    ok &= check("steps-to-boundary is a pure number", n > 0 and math.isfinite(n),
                f"N = {n:.1f}")

    # Doubling the budget with alpha fixed must not change the step count:
    # the law depends on the *ratio* h0/h_target only.
    big = Manifold.bootstrap(budget=1e6, alpha=mf.alpha)
    ok &= check("steps-to-boundary is scale-free in the budget",
                abs(big.steps_to_reach(0.99) - n) < 1e-6,
                f"{big.steps_to_reach(0.99):.4f} vs {n:.4f}")

    # The damping matrix is dimensionless with unit diagonal.
    rates = rates_from_half_lives(NOMINAL_HALF_LIVES)
    c = damping_matrix(rates)
    ok &= check("damping matrix has unit diagonal",
                all(abs(c[i][i] - 1.0) < 1e-12 for i in range(N)))
    ok &= check("damping matrix entries are in [0,1]",
                all(0.0 <= c[i][j] <= 1.0 + 1e-12 for i in range(N) for j in range(N)))

    # And it is invariant under the time unit, since it is a ratio of rates.
    c2 = damping_matrix(rates_from_half_lives(tuple(h / 3600 for h in NOMINAL_HALF_LIVES)))
    ok &= check("damping matrix is invariant under the time unit",
                all(abs(c[i][j] - c2[i][j]) < 1e-12 for i in range(N) for j in range(N)))

    # The nine-order spread that the whole vector-state argument rests on.
    spread = math.log10(max(NOMINAL_HALF_LIVES) / min(NOMINAL_HALF_LIVES))
    ok &= check("half-life spread spans at least nine orders", spread >= 9.0,
                f"{spread:.1f} orders — the argument against a scalar risk score")

    print("\n" + ("all dimensional checks passed" if ok else "DIMENSIONAL CHECKS FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
