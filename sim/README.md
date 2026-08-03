# sim — independent validation

A second implementation of the mathematics, in pure Python standard library,
written to check `crates/`. No numpy: an independent implementation that shares
a numerical library with the thing it checks is a weaker check, and one that
needs an install is one more reason nobody runs it.

```bash
python3 sim/run_all.py
```

No dependencies. Runs in a few seconds.

| Script | What it establishes |
|---|---|
| `manifold.py` | The reference model — linear algebra, metric, barrier, dynamics |
| `units.py` | Every axis in bits, `V` and `c` in bits², all Π-groups dimensionless |
| `calibration.py` | `M` and `c` fit from a benign corpus; geometry learns the escalation structure |
| `adversarial.py` | The pre-registered falsification test from `docs/03` |

## The result that matters

`docs/03` predicted the time-to-boundary scaling law from dimensional analysis
alone, before any dynamics had been written:

```
N ~ (1/α) · ln(h₀ / h_target)
```

`docs/06` T2 derived the same expression from the barrier condition. This
measures it with an optimizer free to choose direction and magnitude at every
step, told the metric, the budget, and `α`.

All three agree, and the measured scaling is logarithmic — 45 steps per decade
of approach at both decades tested. Had it come out polynomial, the formulation
would have been wrong, and `docs/03` said so in advance rather than leaving it
to be explained away afterward.

## Findings that came out of running these

Both were invisible against the identity metric and only appeared once a real
one was fit:

- The iterative Lyapunov projection is numerically unusable at a nine-order
  half-life spread — it produced `c = 3.1e8 bits²`. Replaced with a closed form.
- A tail-sparse axis (irreversibility is exactly zero for ~92% of benign
  requests) drives its sample variance toward zero and explodes the inverse
  covariance. Fixed with a measurement-resolution floor.

Details in the commit history and in `crates/mp-core/src/metric.rs`.
