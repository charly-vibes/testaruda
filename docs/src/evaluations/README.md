# testaruda — Evaluation Reports

Independent evaluation reports covering implementation state, specification
correctness, and release readiness for testaruda.

These are historical, commit-scoped engineering records. Findings in an early
round may be resolved by a later retest or by commits after the report's subject
commit; they do not describe the current release unless explicitly restated.

| Document | Scope |
|---|---|
| [`testaruda-evaluation.md`](testaruda-evaluation.md) | Full evaluation across three rounds (2026-07-07 → 2026-07-10 → 2026-07-14). Covers implementation findings I1–I12, specification defects S1–S9, retest rounds, and deep evaluation of the ticket backlog. |
| [`kani-bmc-probe.md`](kani-bmc-probe.md) | Kani bounded-model-checking probe of the selection pipeline (2026-11-25, subject `9c0793251c`, beads `testaruda-jdpi`). 0/7 harnesses proved; verdict RETIRE with re-open trigger; documents the std-prelude E0659 dependency wall and the allocation-modeling wall. |

## Key findings (short version)

### Implementation
- **I11 (P0)** — Adapter handshake envelope mismatch: `AdapterIO::spawn` expects top-level fields but adapters wrap responses in `{"ok":..., "result":...}`. This single bug blocks all adapter communication.
- **I12 (P1)** — Backlog divergence: tracker reports adapter-protocol tickets as done but the core loop is still broken.
- **I1–I2 (P0)** — Data ingress gap and FK violation; both are downstream symptoms of I11.

### Specification (all fixed in SRS v0.3)
- S1: Homomorphism claim too strong (scoped to positive reachability)
- S2: Confidence inversion for unresolved nodes (fixed to zero confidence)
- S3: Two incompatible confidence definitions (resolved by subordination)
- S4: Exit-20 contradicts always-run (fixed to union condition)
- S5: Derivative-incrementality overclaim (softened to scope-bounded)
- S6: AGENT/SEL/COMP unwired (fixed with implicit ordering)
- S7: CHG-004 missing precondition (gated on capability flag)
- S8: RUN-005/REL-002 no enabling mechanism (run-identity key and WAL boundary)
