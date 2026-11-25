# Kani bounded-model-checking probe — selection pipeline (testaruda-jdpi)

Status: **probe complete, disposition recorded below**. This is a
tractability measurement with an extend/hold/retire gate — **no result
here may be cited as assurance about the shipped engine** (the harness
crate compiles a verbatim copy, see `kani-probe/src/lib.rs` provenance
header).

## Question

Is Kani bounded model checking tractable over testaruda's selection
algorithm (`src/engine.rs`'s ascent! Datalog program + the pure
pipeline around it), worth a permanent verification lane following
catherine's `add-kani-verification-lane` pattern?

## Method

- Probe crate `kani-probe/`: verbatim copy of the ascent! program, the
  four pipeline functions, and the result structs from `src/engine.rs`
  (selection context re-declared from `src/store.rs`, unused
  `current_environment` String omitted). Drift invalidates proofs, not
  the measurement.
- Gate: `scripts/check-kani-probe.sh` (`just verify-kani`) — pinned
  Kani release (kani-version.toml), systemd user scope capped at 8 GB
  (swap off; the catherine-amh OOM incident is the reason), per-harness
  solver budget 240 s (`--harness-timeout`), unwind 16, three-valued
  outcome + disposition rule.
- Bounds: ids in a 4-element domain (8 for the layer-B grouping
  surface), ≤2 entries per relation, weights in the valid `0..=ONE`
  ppm domain.

## Why a separate crate

Kani 0.68.0 cannot compile testaruda's real dependency closure: its
rebuilt std prelude collides with `backtrace`'s `#[macro_use]` (pulled
by miette → backtrace-ext) — E0659 `unreachable` is ambiguous, at
backtrace 0.3.75 AND 0.3.76; `--no-assert-overrides` (the documented
escape hatch) does not clear it. A real lane must extract the algorithm
into a dependency-light kernel crate (the catherine-engine pattern).

## Harnesses — two layers

- **Layer A (full pipeline, includes the ascent fixpoint `prog.run()`)**:
  `pipeline_panic_freedom`, `selection_soundness_no_phantom_tests`,
  `confidence_ceiling_invariant`, `fallback_completeness`.
- **Layer B (the non-ascent shipped logic over nondeterministic
  well-formed program relations — no `prog.run()`)**:
  `floor_collect_panic_freedom`, `floor_contract_two_sided` (the
  post-testaruda-2bxl floor semantics, two-sided),
  `collect_lookup_consistency`.

## Matrix

Full run 2026-11-25 (`target/kani-probe-timings.txt`; peak RSS is the
max single process; wall includes per-harness relink; solver budget
240 s, unwind 16, 8 GB scope cap).

| harness | layer | outcome | wall (s) | peak RSS |
|---|---|---|---|---|
| pipeline_panic_freedom | A | UNDETERMINED (formula explosion) | 88 | 2.28 GB |
| selection_soundness_no_phantom_tests | A | UNDETERMINED (formula explosion) | 85 | 2.28 GB |
| confidence_ceiling_invariant | A | UNDETERMINED (formula explosion) | 78 | 2.28 GB |
| fallback_completeness | A | UNDETERMINED (formula explosion) | 82 | 2.28 GB |
| floor_collect_panic_freedom | B | UNDETERMINED (solver budget exhausted) | 251 | 2.94 GB |
| floor_contract_two_sided | B | UNDETERMINED (solver budget exhausted) | 251 | 2.98 GB |
| collect_lookup_consistency | B | UNDETERMINED (solver budget exhausted) | 244 | 2.35 GB |

**0 proved / 0 counterexamples / 7 undetermined.** Layer A fails fast
(~80 s) on an UNSUPPORTED-CONSTRUCT reachable
(`__rust_alloc_error_handler`, foreign `read`/`futex`/
`Timespec::now` syscalls via std noise) over ~574 checks — the formula
explodes inside ascent-generated `hashbrown::HashMap` internals
(FxHasher tables, `RawIter`, `new_uninitialized` arithmetic) before
any engine property is decided. Layer B, with the ascent fixpoint
removed, still exhausts the 240 s solver budget: the remaining surface
is allocation-bearing (symbolic `Vec` growth, the floor's
`HashMap<u32, Vec<f64>>` grouping), and allocation modeling alone
blows the formula at unwind 16.

This is the same container-modeling wall catherine measured — reached
even without their BTreeMap aggravation, because ascent GENERATES the
containers and the shipped pipeline allocates throughout. Catherine's
fix (the interning arc: dense id tables, allocation-free proven paths)
is an engine rewrite, not a harness exercise.

## Dispositions (script-enforced: `<harness>: undetermined — ...`)

pipeline_panic_freedom: undetermined — layer A: ascent fixpoint drags hashbrown/std internals (OOM/panic/futex paths) into the formula; undecidable at unwind 16 within 8 GB/240 s; the tractable surface is covered by layer B harnesses.

selection_soundness_no_phantom_tests: undetermined — layer A: same wall (see pipeline_panic_freedom); the soundness claim over the real pipeline needs a solver-friendly engine core, not a bigger budget.

confidence_ceiling_invariant: undetermined — layer A: same wall; measured 574 checks, 56.6 s verification, 2.3 GB peak on the cold run.

fallback_completeness: undetermined — layer A: same wall; the TIA-SAFE-003 rerun contract rides on the same ascent fixpoint.

floor_collect_panic_freedom: undetermined — layer B: even without the ascent fixpoint, the floor's HashMap grouping and symbolic Vec growth exhaust the 240 s solver budget at unwind 16; allocation modeling is the wall, not the fixpoint.

floor_contract_two_sided: undetermined — layer B: same wall (HashMap grouping on both the implementation and the harness-rederived contract).

collect_lookup_consistency: undetermined — layer B: pure Vec-scan surface still exhausts the budget (2.35 GB / 244 s) — witness Vec construction with symbolic capacity alone is undecidable at unwind 16.

## Verdict

**RETIRE** (for now — recorded 2026-11-25, bd `testaruda-jdpi`).

- Kani 0.68.0 cannot check this algorithm as written: 0/7 harnesses
  proved, uniform undetermined outcomes across two harness layers and
  three distinct formula shapes (ascent fixpoint, HashMap grouping,
  plain Vec scans). The wall is allocation modeling, not the bounds —
  a bigger budget buys a bigger formula (catherine's measured
  conclusion, independently reproduced here).
- A viable lane requires restructuring the core to be allocation-free
  on proven paths (catherine-engine interning pattern). That is an
  engine rewrite that would fight ascent — the Datalog substrate IS
  the engine's core abstraction. The cost/benefit does not clear.
- Lessons kept: (1) Kani's std-prelude E0659 blocks any workspace
  pulling `backtrace`/miette — a real lane needs a dependency-light
  kernel crate; (2) the pinned-release + scoped-run + disposition gate
  (scripts/check-kani-probe.sh) is proven machinery, reusable if the
  core ever becomes allocation-free; (3) the reachable invariants
  (confidence ceiling, floor semantics post-2bxl, fallback
  completeness) are written down as harnesses — they are property-test
  candidates for hegel, the sampled complement.
- Re-open trigger: the selection core gets an allocation-free
  execution path (dense ids over fixed tables) — then port this probe
  crate to it and rerun `just verify-kani`.
