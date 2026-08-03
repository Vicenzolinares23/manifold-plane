"""Reference implementation of the manifold-plane mathematics.

Pure standard library. No numpy, deliberately: this is a *second, independent*
implementation of what `crates/mp-core` and `crates/mp-barrier` do, written to
check them. An independent implementation that shares a numerical library with
the thing it checks is a weaker check, and one that needs an install is one
more reason for nobody to run it.

Everything here traces to `docs/`. Section references are given inline.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

N = 6

# docs/04. Order matters — it is the coordinate order everywhere.
AXES = ("authority", "reach", "irreversibility", "opacity", "coupling", "tempo")

# docs/03. Seconds. The nine-order spread is the argument against a scalar score.
NOMINAL_HALF_LIVES = (43_200.0, 172_800.0, 3.15e12, 3_600.0, 300.0, 30.0)


# --------------------------------------------------------------------------
# Linear algebra
# --------------------------------------------------------------------------

def zeros() -> list[float]:
    return [0.0] * N


def identity() -> list[list[float]]:
    return [[1.0 if i == j else 0.0 for j in range(N)] for i in range(N)]


def matvec(m, v):
    return [sum(m[i][j] * v[j] for j in range(N)) for i in range(N)]


def matmul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(N)) for j in range(N)] for i in range(N)]


def transpose(a):
    return [[a[j][i] for j in range(N)] for i in range(N)]


def quad(m, v):
    return sum(v[i] * sum(m[i][j] * v[j] for j in range(N)) for i in range(N))


def vadd(a, b):
    return [a[i] + b[i] for i in range(N)]


def vsub(a, b):
    return [a[i] - b[i] for i in range(N)]


def vscale(a, s):
    return [x * s for x in a]


def symmetrize(a):
    return [[0.5 * (a[i][j] + a[j][i]) for j in range(N)] for i in range(N)]


def eigh(a):
    """Cyclic Jacobi eigendecomposition. Returns (values, vectors-as-columns).

    Mirrors `mp_core::linalg::eigh` so the two can be compared directly.
    """
    m = [row[:] for row in symmetrize(a)]
    v = identity()

    for _ in range(100):
        off = sum(m[i][j] ** 2 for i in range(N) for j in range(i + 1, N))
        if off <= 1e-30:
            break
        for p in range(N):
            for q in range(p + 1, N):
                if abs(m[p][q]) < 1e-300:
                    continue
                theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q])
                sign = 1.0 if theta >= 0 else -1.0
                t = sign / (abs(theta) + math.sqrt(theta * theta + 1.0))
                c = 1.0 / math.sqrt(t * t + 1.0)
                s = t * c
                for k in range(N):
                    mkp, mkq = m[k][p], m[k][q]
                    m[k][p], m[k][q] = c * mkp - s * mkq, s * mkp + c * mkq
                for k in range(N):
                    mpk, mqk = m[p][k], m[q][k]
                    m[p][k], m[q][k] = c * mpk - s * mqk, s * mpk + c * mqk
                for k in range(N):
                    vkp, vkq = v[k][p], v[k][q]
                    v[k][p], v[k][q] = c * vkp - s * vkq, s * vkp + c * vkq

    return [m[i][i] for i in range(N)], v


def spectral_map(a, f):
    vals, vecs = eigh(a)
    d = [[f(vals[i]) if i == j else 0.0 for j in range(N)] for i in range(N)]
    return symmetrize(matmul(matmul(vecs, d), transpose(vecs)))


def min_eig(a):
    return min(eigh(a)[0])


def inv_spd(a, tol=1e-12):
    vals, _ = eigh(a)
    if min(vals) <= tol:
        return None
    return spectral_map(a, lambda l: 1.0 / l)


# --------------------------------------------------------------------------
# Metric — docs/05 §5.2, §5.6
# --------------------------------------------------------------------------

def rates_from_half_lives(half_lives):
    return [math.log(2.0) / h if h > 0 else 0.0 for h in half_lives]


def lyapunov(m, rates):
    """L(M) = ΛM + MΛ. Entrywise (λᵢ+λⱼ)·Mᵢⱼ."""
    return symmetrize([[(rates[i] + rates[j]) * m[i][j] for j in range(N)] for i in range(N)])


def lyapunov_inv(s, rates):
    out = [[0.0] * N for _ in range(N)]
    for i in range(N):
        for j in range(N):
            d = rates[i] + rates[j]
            out[i][j] = s[i][j] / d if abs(d) > 1e-300 else 0.0
    return symmetrize(out)


def project_feasible(raw, rates, floor=1e-9):
    """Project onto {M ≻ 0} ∩ {ΛM + MΛ ⪰ 0} by alternating projections.

    docs/05 §5.6. Without this, relaxation toward baseline can *increase* V and
    an asker can leave the safe set while issuing no requests at all.
    """
    m = symmetrize(raw)
    for _ in range(256):
        m = spectral_map(m, lambda l: max(l, floor))
        s = lyapunov(m, rates)
        if min_eig(s) >= 0.0 and min_eig(m) >= floor:
            break
        m = lyapunov_inv(spectral_map(s, lambda l: max(l, 0.0)), rates)
    return spectral_map(m, lambda l: max(l, floor))


def is_feasible(m, rates):
    return min_eig(lyapunov(m, rates)) >= -1e-9 and min_eig(m) > 0.0


# --------------------------------------------------------------------------
# The model
# --------------------------------------------------------------------------

@dataclass
class Manifold:
    """Metric, budget, and barrier aggressiveness."""

    metric: list[list[float]]
    budget: float
    alpha: float
    rates: tuple

    @classmethod
    def bootstrap(cls, budget=100.0, alpha=0.05, half_lives=NOMINAL_HALF_LIVES):
        rates = tuple(rates_from_half_lives(half_lives))
        return cls(metric=identity(), budget=budget, alpha=alpha, rates=rates)

    def potential(self, z) -> float:
        """V(z) = zᵀMz, in bits²."""
        return quad(self.metric, z)

    def margin(self, z) -> float:
        """h(z) = c − V(z)."""
        return self.budget - self.potential(z)

    def relax(self, z, dt):
        """R_Δt(z) = exp(−ΛΔt)·z. Δt clamped: docs/07 F6."""
        dt = max(dt, 0.0)
        return [z[i] * math.exp(-self.rates[i] * dt) for i in range(N)]

    def admits(self, z, step, residual=0.0) -> bool:
        """The barrier condition, docs/05 §5.5."""
        alpha_eff = self.alpha / (1.0 + max(residual, 0.0))
        h0 = self.margin(z)
        if h0 < 0.0:
            return False
        return self.margin(vadd(z, step)) >= (1.0 - alpha_eff) * h0

    def saturating_scale(self, z, direction) -> float:
        """Largest s ≥ 0 with s·direction saturating the barrier. docs/06 T4.

        Solves s²‖g‖²_M + 2s⟨z,g⟩_M − α·h = 0. This is the optimal adversary's
        step, unclamped — enforcement uses the clamped version.
        """
        a = quad(self.metric, direction)
        b = 2.0 * sum(
            z[i] * sum(self.metric[i][j] * direction[j] for j in range(N)) for i in range(N)
        )
        c = -self.alpha * self.margin(z)
        if c >= 0.0:
            return 0.0
        if abs(a) < 1e-18:
            return max(-c / b, 0.0) if abs(b) > 1e-18 else float("inf")
        disc = b * b - 4.0 * a * c
        if disc < 0.0:
            return 0.0
        return max((-b + math.sqrt(disc)) / (2.0 * a), 0.0)

    def adversary_bound(self, v0: float, n: int) -> float:
        """docs/06 T2: V_n ≤ c − (1−α)ⁿ(c − V₀)."""
        return self.budget - (1.0 - self.alpha) ** n * (self.budget - v0)

    def steps_to_reach(self, fraction: float, v0: float = 0.0) -> float:
        """docs/06 T2 corollary: N ≈ (1/α)·ln(h₀/h_target)."""
        h0 = self.budget - v0
        h_t = self.budget * (1.0 - fraction)
        if h_t <= 0 or h0 <= 0:
            return float("inf")
        return math.log(h_t / h0) / math.log(1.0 - self.alpha)


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else 0.0


def median(xs):
    xs = sorted(xs)
    if not xs:
        return 0.0
    k = len(xs)
    return xs[k // 2] if k % 2 else 0.5 * (xs[k // 2 - 1] + xs[k // 2])


def quantile(xs, q):
    xs = sorted(xs)
    if not xs:
        return 0.0
    idx = int(round((len(xs) - 1) * min(max(q, 0.0), 1.0)))
    return xs[idx]


class Rng:
    """Deterministic xorshift64*. Seeded runs so results are reproducible;
    a validation script whose numbers move between runs is not validation."""

    def __init__(self, seed=0x2545F4914F6CDD1D):
        self.s = seed & 0xFFFFFFFFFFFFFFFF

    def next_u64(self):
        s = self.s
        s ^= (s << 13) & 0xFFFFFFFFFFFFFFFF
        s ^= s >> 7
        s ^= (s << 17) & 0xFFFFFFFFFFFFFFFF
        self.s = s & 0xFFFFFFFFFFFFFFFF
        return self.s

    def uniform(self, lo=0.0, hi=1.0):
        return lo + (hi - lo) * (self.next_u64() / 2**64)

    def normal(self, mu=0.0, sigma=1.0):
        # Box-Muller.
        u1 = max(self.uniform(), 1e-12)
        u2 = self.uniform()
        return mu + sigma * math.sqrt(-2.0 * math.log(u1)) * math.cos(2.0 * math.pi * u2)
