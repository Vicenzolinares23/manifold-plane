"""Adversarial validation: measure what an optimal attacker actually achieves.

`docs/07-adversarial-validation.md`.

The headline check. `docs/03` predicted the time-to-boundary scaling law from
dimensional analysis alone, before any dynamics existed. `docs/06` T2 derived
the same expression from the barrier condition. This measures it with an
optimizer that is free to choose direction and magnitude at every step, and is
told the model, the metric, the budget, and alpha — Kerckhoffs applies, and a
barrier whose safety depended on the attacker not knowing alpha would be
worthless.

Three independent routes to one law. If the measurement disagrees, the
formulation is wrong, and `docs/03` pre-registered that as the falsification
test rather than leaving it to be explained away afterward.
"""

from __future__ import annotations

import math

from manifold import N, Manifold, Rng, quad, vadd, vscale, zeros


def greedy_adversary(mf: Manifold, steps: int, direction=None):
    """An attacker that saturates the barrier every step along one direction.

    This is the optimal strategy against the rule: T4 shows the admissible set
    is a ball whose radius shrinks with the margin, so taking the largest
    admissible step every time is the fastest approach available.
    """
    if direction is None:
        direction = [0.0] * N
        direction[2] = 1.0  # irreversibility: the axis that does not decay

    z = zeros()
    trajectory = []
    for _ in range(steps):
        s = mf.saturating_scale(z, direction)
        if not math.isfinite(s) or s <= 0.0:
            break
        z = vadd(z, vscale(direction, s))
        trajectory.append(mf.potential(z))
    return z, trajectory


def coordinate_search_adversary(mf: Manifold, steps: int, rng: Rng, restarts: int = 24):
    """An attacker that also searches over *direction* at every step.

    The greedy adversary is confined to one axis. This one probes many
    directions per step and takes whichever saturating step maximizes V. If the
    barrier could be beaten by a cleverer choice of direction, this finds it.
    """
    z = zeros()
    trajectory = []

    for _ in range(steps):
        best_z, best_v = None, mf.potential(z)

        for _ in range(restarts):
            d = [rng.normal() for _ in range(N)]
            norm = math.sqrt(sum(x * x for x in d)) or 1.0
            d = [x / norm for x in d]

            s = mf.saturating_scale(z, d)
            if not math.isfinite(s) or s <= 0.0:
                continue
            cand = vadd(z, vscale(d, s))
            v = mf.potential(cand)
            if v > best_v:
                best_z, best_v = cand, v

        if best_z is None:
            break
        z = best_z
        trajectory.append(mf.potential(z))

    return z, trajectory


def patient_adversary(mf: Manifold, steps: int, dt_between: float):
    """An attacker that waits between steps, letting relaxation restore margin.

    Waiting is a real strategy: decay refunds margin on the fast axes. What it
    cannot refund is irreversibility, whose half-life is effectively infinite —
    so the patient attacker buys back tempo and opacity and nothing else. This
    measures how much that is worth.
    """
    direction = [0.0] * N
    direction[2] = 1.0
    z = zeros()
    for _ in range(steps):
        z = mf.relax(z, dt_between)
        s = mf.saturating_scale(z, direction)
        if not math.isfinite(s) or s <= 0.0:
            break
        z = vadd(z, vscale(direction, s))
    return z


def measure_scaling_law(mf: Manifold, fractions=(0.5, 0.9, 0.99, 0.999)):
    """Measure steps-to-reach against the analytic prediction."""
    rows = []
    for f in fractions:
        target = mf.budget * f
        z = zeros()
        n = 0
        direction = [0.0] * N
        direction[2] = 1.0
        while mf.potential(z) < target and n < 10_000_000:
            s = mf.saturating_scale(z, direction)
            if not math.isfinite(s) or s <= 0.0:
                break
            z = vadd(z, vscale(direction, s))
            n += 1
        rows.append((f, n, mf.steps_to_reach(f)))
    return rows


def check(label: str, ok: bool, detail: str = "") -> bool:
    print(f"  [{'PASS' if ok else 'FAIL'}] {label}" + (f" — {detail}" if detail else ""))
    return ok


def main() -> int:
    print("adversarial validation (docs/07)\n")
    ok = True

    mf = Manifold.bootstrap(budget=100.0, alpha=0.05)
    rng = Rng(0xC0FFEE)

    # T1 — forward invariance under the greedy adversary.
    print("T1 forward invariance")
    z, traj = greedy_adversary(mf, 200_000)
    ok &= check(
        "greedy adversary never leaves the safe set",
        mf.margin(z) >= -1e-9,
        f"h={mf.margin(z):.6e} after 200k saturating steps",
    )
    ok &= check(
        "potential is monotone non-decreasing",
        all(traj[i] <= traj[i + 1] + 1e-9 for i in range(len(traj) - 1)),
    )

    # T2 — the analytic bound.
    print("\nT2 adversary bound")
    worst = max(abs(traj[i] - mf.adversary_bound(0.0, i + 1)) for i in range(min(500, len(traj))))
    ok &= check(
        "measured approach matches V_n = c − (1−α)ⁿ(c − V₀)",
        worst < 1e-6,
        f"max deviation {worst:.3e} over 500 steps",
    )
    ok &= check(
        "budget is never reached in finite steps",
        mf.potential(z) < mf.budget,
        f"V={mf.potential(z):.6f} < c={mf.budget}",
    )

    # The direction-searching adversary must do no better than the greedy one.
    print("\ndirection search")
    z2, traj2 = coordinate_search_adversary(mf, 400, rng)
    ok &= check(
        "searching over direction beats nothing",
        mf.potential(z2) <= mf.adversary_bound(0.0, len(traj2)) + 1e-6,
        f"V={mf.potential(z2):.4f} vs bound {mf.adversary_bound(0.0, len(traj2)):.4f}",
    )
    ok &= check("direction search stays inside Ω", mf.margin(z2) >= -1e-9)

    # The scaling law — the pre-registered falsification test.
    print("\nscaling law: N ~ (1/α)·ln(h₀/h_target)   [docs/03 → docs/06 T2]")
    print(f"  {'fraction of c':>14} {'measured':>10} {'predicted':>11} {'abs err':>9}")
    for f, measured, predicted in measure_scaling_law(mf):
        # Compare in *steps*, not as a relative error. Step counts are integers
        # and the prediction is continuous, so a correct model still lands up
        # to one step off — and at small N that rounding alone is several
        # percent, which a relative-error test would report as a failure of the
        # theory rather than of arithmetic. The real claim is that measurement
        # and prediction agree to within the discretization, so that is what
        # gets tested.
        err = abs(measured - predicted)
        print(f"  {f:>14.3f} {measured:>10d} {predicted:>11.1f} {err:>8.2f}")
        ok &= check(
            f"  agreement at {f:.3f} of budget",
            err <= 1.0,
            f"{err:.2f} steps (discretization allows 1.0)",
        )

    # Logarithmic, not polynomial. This is the shape claim, and it is the one
    # that would have falsified the formulation had it come out otherwise.
    rows = measure_scaling_law(mf, (0.9, 0.99, 0.999))
    d1 = rows[1][1] - rows[0][1]
    d2 = rows[2][1] - rows[1][1]
    ok &= check(
        "\n  scaling is logarithmic: each decade of approach costs a constant",
        abs(d2 - d1) / max(d1, 1) < 0.1,
        f"decade costs {d1} then {d2} steps",
    )

    # Patience buys back the fast axes and nothing else.
    print("\npatient adversary")
    impatient = patient_adversary(mf, 500, 0.0)
    patient = patient_adversary(mf, 500, 86_400.0)
    ok &= check(
        "waiting a day between steps does not defeat the bound",
        mf.margin(patient) >= -1e-9,
        f"h={mf.margin(patient):.4e}",
    )
    ok &= check(
        "irreversibility is not refunded by waiting",
        abs(mf.potential(patient) - mf.potential(impatient)) / max(mf.potential(impatient), 1e-9)
        < 0.05,
        "patience gains <5% on the non-decaying axis",
    )

    # Robustness of the law across alpha.
    print("\nalpha sensitivity")
    for a in (0.01, 0.05, 0.2, 0.5):
        m2 = Manifold.bootstrap(budget=100.0, alpha=a)
        _, t2 = greedy_adversary(m2, 5000)
        worst2 = max(
            abs(t2[i] - m2.adversary_bound(0.0, i + 1)) for i in range(min(200, len(t2)))
        )
        ok &= check(f"  bound holds at α={a}", worst2 < 1e-6, f"max dev {worst2:.2e}")

    print("\n" + ("all adversarial checks passed" if ok else "ADVERSARIAL CHECKS FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
