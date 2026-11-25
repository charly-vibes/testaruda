# Verus functional-proof probe — selection Boolean core (testaruda-rqvc)
Status: **probe complete, disposition recorded below**. This is a
tractability measurement with an extend/hold/retire gate — **no result
here may be cited as assurance about the shipped engine** (the proof
discharges over the probe model; the parity driver's exec walk is a
transcription, see the provenance guard section).

## Question

Can Verus discharge *functional correctness* (not just panic-freedom)
over the selection algorithm's Boolean core — impacted (reverse
reachability) and affected marking — the family the Kani probe
(testaruda-jdpi) wrote harnesses for but could not symbolically
execute? And at what cost: is a permanent verification lane tractable,
following catherine's `add-verus-verification-lane` pattern?

## Method

- Probe model `verus-probe/model/selection_reachability.rs`: a bounded
  dense-id model of the core — `impacted` as the CU_CAP-step closure
  of the seed set over cu-dep edges, `affected` = always-run ∪
  impacted-dep tests ∪ fallback-component tests — each with a full
  functional spec (sorted/ascending affected, exact closure membership)
  plus panic-freedom. Exec strategy: **snapshot relaxation** — each
  pass reads only the pre-pass vector and writes a fresh one, making
  `exec == spec` a direct induction (the allocation-shape wall the
  Kani probe hit is avoided by construction, not out-argued).
- Gate: `scripts/check-verus-probe.sh` (`just verify-verus`) — pinned
  Verus release (verus-version.toml, sha256-provisioned), per-unit
  three-valued outcome (VERIFIED / ERROR / EXHAUSTED; a run with no
  `verification results::` line is ERROR, never silently green),
  wall-capped 300 s, wall times recorded to
  `target/verus-probe-timings.txt`.
- Parity: `verus-probe/parity/` — a detached plain-cargo driver
  comparing the model's exec walk against a plain-Rust reference of
  the shipped Boolean semantics over 14 fixture groups (chain, diamond,
  cycle, unreachable, fallback, composition), with the
  `chain-contract` line as the non-vacuity witness. This verus
  distribution verifies only (`--crate-type lib`, no binary emission),
  so the exec side is a **transcription**; drift between transcription
  and model is guarded by a skeleton check in the gate script — both
  files are reduced to their sequence of loop guards + effectful
  statements (proof blocks brace-counted out, ghost bindings dropped,
  `.set(x, y)` mapped to `[x] = y`) and diffed in order. Silent
  transcription drift fails the gate.

## Matrix

Full run 2026-11-25 (`target/verus-probe-timings.txt`; wall includes
the SMT phase under the release's own rlimits; budget 300 s, unused).

| unit / check | outcome | wall |
|---|---|---|
| model/selection_reachability.rs (6 proof obligations) | VERIFIED (6 verified, 0 errors) | 1196 ms |
| provenance skeleton: impacted_exec ↔ transcribed | match | — |
| provenance skeleton: affected_exec ↔ transcribed | match | — |
| parity: 14 fixture groups | PASS (0 mismatches) | <1 s |
| parity: chain-contract non-vacuity witness | match | — |

**6 verified / 0 errors / 0 undetermined.** Every obligation in the
fragment — panic-freedom, exact k-step closure membership for
`impacted`, sorted-ascending + membership + minimality-shaped contract
for `affected` — discharged on the first verification pass after the
snapshot-relaxation restructuring, sub-second, with no solver tuning,
no memory scope (the kani lane's 8 GB systemd cap is not needed), and
no per-obligation budget. The resource profile is the inverse of the
Kani probe's: there, 7/7 harnesses undetermined at 80–250 s each;
here, all obligations decide in ~1.2 s total.

## Why the fragment is shaped this way

The model does not verify the shipped `src/selection` code and the
gate does not pretend to. Three deliberate boundaries:

1. **Bounded dense ids** (`CU_CAP`/`T_CAP` = 8): the proof is exact
   *within the bound* (≤ 8 units, the k-step closure equals the
   mathematical closure there); unbounded inputs are outside the
   fragment (`requires`). Sizing the bound is a lane-design question,
   not a probe question.
2. **Snapshot relaxation instead of an in-place worklist**: the
   in-place variants are exactly where allocation-shape proofs
   explode (the Kani probe's wall, applied as a design lesson). The
   relaxation is O(CU_CAP²) in copies — irrelevant at these bounds.
3. **Transcription parity** (not linked parity): verus here emits no
   binaries, so the exec walk cannot be linked into the parity driver;
   the skeleton guard is the honest substitute — coarse (statements,
   not expressions) but order-sensitive, and it already caught three
   transcription drifts during development.

## Dispositions (script-enforced: gate fails on any non-VERIFIED unit,
missing results line, skeleton drift, parity mismatch, or missing
non-vacuity witness)

model/selection_reachability.rs: verified — fragment discharged; the
boundary is stated in the model header and this file, not hidden.

## Verdict

**HOLD** (recorded 2026-11-25, bd `testaruda-rqvc`).

- The technique is proven and cheap: functional specs over the Boolean
  core discharge in ~1.2 s wall with zero infrastructure (no memory
  scope, no budgets, no per-harness tuning). The Kani probe's wall
  (allocation modeling) does not exist here — Verus's SMT framing over
  vstd sequences/bools decides these obligations outright.
- A lane is **not** built now: the verified artifact is the probe
  model, not the shipped engine. Wiring the proof to the shipped
  algorithm requires either (a) extracting the core into a
  dependency-light kernel crate (the catherine-engine interning
  pattern) so the model and the code are the same code — an engine
  restructure that would fight ascent, the same cost/benefit that
  retired the Kani lane; or (b) proving the ascent!-generated program
  directly, where vstd's container specs over generated hashbrown
  code is an unmapped cost. Both are lane-sized openspec work, not
  probe follow-ups.
- Lessons kept: (1) snapshot relaxation makes fixpoint-family exec
  proofs direct inductions — reuse it for any future selection-core
  proof; (2) the pinned-release + three-valued-outcome +
  provenance-skeleton + parity gate (scripts/check-verus-probe.sh) is
  proven machinery, portable to a lane crate; (3) this verus
  distribution verifies but does not link binaries — any lane's parity
  driver must be plain cargo against a verified lib boundary, or the
  lane must emit metadata instead.
- Re-open trigger: the selection core is extracted dependency-light
  (dense ids over fixed tables, catherine-engine pattern) — then move
  this model beside it as its spec crate, wire the drift guard to the
  real source, and propose the lane via openspec.
