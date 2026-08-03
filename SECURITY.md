# Security

## Status

Research software. The mathematics is proved and the implementation is tested,
but nothing here has run against production traffic. Three findings in
`docs/07-adversarial-validation.md` are open, one of them (F4) structural.

Do not make this the only control in a path that matters. It is designed to
compose with RBAC, seccomp, network policy, and audit — it bounds trajectories,
which those do not, and it does not replace what those do.

## Threat model

Assumed: the adversary knows the model, the code, the metric `M`, the budget
`c`, and `α`. Kerckhoffs applies. A barrier whose safety depended on the
attacker not knowing `α` would be worthless, and `sim/adversarial.py` tells the
optimizer all of it.

Out of scope, and stated plainly rather than defended badly:

- An adversary that can modify the engine's carried state directly.
- An adversary that can forge audit events for actions it did not take.
- An adversary that can move the host clock forward (free relaxation).

Any of these means the host is compromised, and this system's guarantees are
gone by a route it does not address.

## Known-weakening configurations

The daemon reports each of these at startup and will run anyway. It should be a
deliberate choice, not a discovery during an incident.

| Condition | Effect |
|---|---|
| `budget_is_calibrated: false` | Deciding against an arbitrary boundary. `/readyz` returns 503. |
| `fail_open: true` | An attacker who can crash the process disables admission control. |
| Singleton symmetry classes | No orbit residual; every asker falls back on its own baseline (F2). |
| Nominal half-lives | Using `docs/03` defaults rather than values fitted to your environment. |
| `max_coalition < 4` | Misses most real coalitions. |

## Reporting

Open an issue at
<https://github.com/Vicenzolinares23/manifold-plane/issues>. There is no
embargo process; this is not deployed anywhere that would need one.

Findings that would be most valuable:

- A concrete attack sequence that stays inside `Ω` and achieves a real
  objective. That means the axes are incomplete and `docs/04` needs reopening.
- Benign traffic whose displacement distribution no `c` separates from attack
  traffic. That means the ellipsoid is the wrong shape and the potential needs
  replacing, not the constant.
- Progress on F4 — bounding an adversary that partitions itself across
  deliberately uncorrelated identities.
