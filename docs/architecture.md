# Architecture

testaruda is built on three layers:

1. **Core engine** — Ascent-embedded Datalog selection query
2. **Store** — SQLite-backed persistence for the dependency graph
3. **CLI** — User-facing commands

## Selection Pipeline

```
┌────────────┐     ┌──────────────┐     ┌───────────┐
│  git diff  │────▶│  Change Set  │────▶│   Store   │
└────────────┘     └──────────────┘     └─────┬─────┘
                                              │
                                              ▼
                                     ┌──────────────────┐
                                     │  Ascent Engine   │
                                     │  (40 lines DL)   │
                                     │  lattice columns │
                                     └────────┬─────────┘
                                              │
                                              ▼
                                     ┌──────────────────┐
                                     │   Selection      │
                                     │  (affected tests)│
                                     └──────────────────┘
```

## Semiring Instantiations

| Quantity | Semiring | ⊕ | ⊗ | 0 | 1 |
|----------|----------|---|---|---|---|
| Affected set | Boolean | ∨ | ∧ | false | true |
| Confidence | Viterbi [0,1] | max | × | 0 | 1 |
| Distance | Tropical ℝ⁺∪∞ | min | + | ∞ | 0 |
| Explanation | Provenance ℕ[Edges] | + | × | 0 | 1 |

## Engine

The reference implementation uses **Ascent** (embedded Datalog with lattice
support). The selection rule set is 40 lines and produces affected tests,
confidence, distance, and minimal-witness chains from a single evaluation.

Soufflé serves as an out-of-process provenance oracle for validation.

## Incrementality

The active selection path uses **change scoping**: it evaluates the dependency
subgraph reachable from changed content units. The store contains
content-addressed component-cache primitives, but the current selection path
does not yet reuse cached selection decisions across invocations.

See `docs/tia-srs-ears.md` for the full specification.
