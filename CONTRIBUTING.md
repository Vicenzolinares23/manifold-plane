# Contributing

## The one rule

**No constant without a measurement procedure.**

If a change adds a number that a deployment would have to tune by hand until
results looked right, it will be rejected — not for style, but because that is
the fudge-term diagnostic: a coefficient that exists to absorb model error means
the *structure* is wrong, and the fix is to find the structure that makes it
unnecessary.

Every existing constant has a procedure in `docs/03-dimensional-analysis.md`.
New ones need theirs in the same place.

## Order of work

`docs/` was written before `crates/`, deliberately, and changes should follow
the same order. A change to behavior that does not touch `docs/` is either
trivial or is quietly editing the model without saying so.

If a change contradicts something in `docs/`, update the document and say what
was wrong. The rejected candidates in those files are as load-bearing as the
accepted ones — several of them exist because an implementation attempt failed
and the reason was worth keeping.

## Adding an axis

The state space is six-dimensional because six candidates passed the tests in
`docs/04`, not because six was chosen. A seventh is welcome if it passes all of
them:

1. **Sufficiency** — `(x, r) → x'` closes without extra history.
2. **Independence** — not recoverable from the other six.
3. **Distinct timescale** — its own half-life, measurably different.
4. **Measurability** — a bit-valued procedure, per `docs/03`.
5. **Adversarial relevance** — an attacker objective expressible as moving it.

Five candidates were rejected; `docs/04` lists them with reasons, so check
whether yours is already there.

## Adding an adapter

Adapters are the softest part of the system: the kernel's guarantee is
conditional on `g` being a faithful measurement of what a request confers. A
wrong `g` yields an impeccable bound on the wrong quantity.

- Classify by **effect**, never by name. A tool called `safe_helper` that makes
  outbound requests prices as an outbound request.
- Cite the `docs/03` rule each displacement implements, so a reviewer can check
  the mapping rather than take it on faith.
- Unknown inputs must be **refused**, not defaulted to something cheap.
  Otherwise an attacker picks a spelling the parser does not recognize.

## Tests

Tests are named as claims (`a_denied_request_leaves_position_untouched_but_costs_tempo`),
not as method names. They document behavior, and several of them exist because
they caught something real — the slow-walk test found the F1 invariance escape.

Changes touching the kernel must keep the theorem tests green:

```bash
cargo test --all
python3 sim/run_all.py
```

`sim/` is an independent reimplementation. If a change makes the two disagree,
at least one of them is wrong and finding out which is the work.
