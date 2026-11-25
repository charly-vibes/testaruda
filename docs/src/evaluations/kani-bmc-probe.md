# Kani bounded-model-checking probe — selection pipeline

> **Historical report:** scoped to subject commit `9c0793251c` on the
> `kani-probe` branch (2026-11-25). Findings describe the selection
> pipeline as of that commit and the pinned toolchain (Kani 0.68.0 /
> CBMC 6.11.0); a later engine or toolchain may invalidate them.

**What this document is.** A self-contained record of the bounded-model-checking
(BMC) probe over testaruda's selection algorithm — what was tried, what was
measured, and why the verdict is RETIRE. Written to the amnesia test: every
term is defined here. The raw matrix, harness source, and gate script live on
the branch (`kani-probe/`, `scripts/check-kani-probe.sh`,
`docs/research/kani-probe.md`); this page is the durable summary.

**Date:** 2026-11-25. **Subject:** selection pipeline at `9c0793251c`
(`src/engine.rs`: the `ascent!` Datalog program — reverse reachability, Viterbi
confidence in ppm space, tropical min-plus distance, minimal-witness
predecessors — plus the pure pipeline around it). **Beads:** `testaruda-jdpi`
(closed). **Provenance of the idea:** catherine's `add-kani-verification-lane`
(the family's mature Kani lane) and specodelic's `spk compile`/`model-check`
(TLA+/proptest/stateright) surveyed first; neither uses Kani, so a Kani lane
here would have been a new assurance mode.

**Verdict in one line:** Kani cannot check this algorithm as written — 0 of 7
harnesses proved across two harness layers and three distinct formula shapes;
the wall is allocation modeling, not the solver bounds — so the BMC lane
proposal is RETIRED with a recorded re-open trigger.

## Terms

- **Kani** — AWS's Rust model checker: it symbolically executes real Rust
  over *all* inputs within declared bounds and discharges safety/property
  assertions with a SAT solver (CBMC). `PROVED` means exhaustively-checked
  within the bounds — a stronger mode than sampled property tests.
- **Unwind bound** — BMC cannot run unbounded loops; loops are cut at N
  iterations (`--default-unwind 16` here). Cut loops surface as *unwinding
  assertions*; a proof is only as sound as the bound.
- **Ascent** — the Datalog-macro library generating the selection query. Its
  fixpoint engine uses `hashbrown::HashMap` internally.
- **Three-valued outcome** — per-harness `proved` / `counterexample` /
  `undetermined`; a timeout, budget kill, or unwind cut is UNDETERMINED,
  never silently green, and requires a recorded disposition.

## Method

- Probe crate `kani-probe/`: a **verbatim copy** of the `ascent!` program,
  the four pipeline functions, and the result structs from `src/engine.rs`
  (drift-flagged in its header — drift invalidates proofs, not the
  tractability measurement). The copy was forced by wall #1 below.
- Harnesses in two layers: **Layer A** over the full pipeline including the
  ascent fixpoint (panic-freedom, selection soundness/no-phantom-tests,
  confidence ceiling ≤ 1.0 in the valid weight domain, fallback
  completeness); **Layer B** over the non-ascent shipped logic
  (`compute_confidence_floor` semantics two-sided — the post-`testaruda-2bxl`
  grouping; result-collection consistency; their panic-freedom) fed
  nondeterministic well-formed program relations. Inputs are nondeterministic
  `kani::any()` integers only: ids masked into a 4-element domain (8 for
  grouping), ≤ 2 entries per relation, weights in the valid `0..=ONE` ppm
  domain.
- Gate (`just verify-kani`): pinned Kani release (`kani-version.toml`),
  every harness inside a systemd user scope capped at 8 GB with swap off
  (an uncapped CBMC run OOM-killed catherine's workstation at 15.4 GB —
  catherine-amh), per-harness solver budget 240 s, unwind 16, three-valued
  outcomes with a disposition rule enforced against
  `docs/research/kani-probe.md`.

## Results

**0 proved / 0 counterexamples / 7 undetermined** (full run 2026-11-25):

| harness | layer | outcome | wall (s) | peak RSS |
|---|---|---|---|---|
| pipeline_panic_freedom | A | UNDETERMINED (formula explosion) | 88 | 2.28 GB |
| selection_soundness_no_phantom_tests | A | UNDETERMINED (formula explosion) | 85 | 2.28 GB |
| confidence_ceiling_invariant | A | UNDETERMINED (formula explosion) | 78 | 2.28 GB |
| fallback_completeness | A | UNDETERMINED (formula explosion) | 82 | 2.28 GB |
| floor_collect_panic_freedom | B | UNDETERMINED (solver budget exhausted) | 251 | 2.94 GB |
| floor_contract_two_sided | B | UNDETERMINED (solver budget exhausted) | 251 | 2.98 GB |
| collect_lookup_consistency | B | UNDETERMINED (solver budget exhausted) | 244 | 2.35 GB |

Layer A fails fast (~80 s) on an unsupported-construct verdict: the formula
explodes inside ascent-generated `hashbrown` internals (FxHasher tables,
`RawIter`, `new_uninitialized` arithmetic) over ~574 checks, with foreign
`read`/`futex`/`Timespec::now` syscalls reachable through std noise paths
before any engine property is decided. Layer B — the fixpoint *removed* —
still exhausts the 240 s solver budget: the surviving surface is
allocation-bearing (the floor's `HashMap<u32, Vec<f64>>` grouping; witness
`Vec` construction with symbolic capacity), and allocation modeling alone
blows the formula at unwind 16.

## Walls

1. **Kani 0.68.0 cannot compile testaruda's real dependency closure.** Its
   rebuilt std prelude collides with `backtrace`'s `#[macro_use]` (pulled by
   `miette` → `backtrace-ext`): E0659 "`unreachable` is ambiguous" at
   backtrace 0.3.75 *and* 0.3.76; `--no-assert-overrides` — the documented
   escape hatch for exactly this error class — does not clear it. Any
   workspace pulling miette is blocked; a real lane would need a
   dependency-light kernel crate (the catherine-engine pattern).
2. **Allocation modeling is the wall, not the bounds.** Catherine measured
   the same wall (all 11 of their harnesses UNDETERMINED at unwind 16 /
   300 s / 8 GB) and fixed it with the interning arc — dense id tables,
   allocation-free proven paths. That is an engine rewrite. Here the
   containers are *generated by ascent*, the engine's core abstraction, and
   the shipped pipeline allocates throughout; removing them would mean
   replacing the Datalog substrate, which defeats the point.

## Verdict

**RETIRE** (recorded 2026-11-25, bd `testaruda-jdpi`). A viable Kani lane
requires an allocation-free execution path in the selection core, which
would fight ascent — the cost/benefit does not clear.

Kept from the probe:

- **Gate machinery**, reusable: pinned release, scoped run, three-valued
  outcomes + disposition rule (`scripts/check-kani-probe.sh`,
  `just verify-kani`, `kani-version.toml`).
- **The invariants as executable specs**: confidence ceiling (TIA-CONF-002),
  floor semantics two-sided (post-2bxl), fallback completeness
  (TIA-SAFE-003), no-phantom-selection (SEL-001/SAFE-007). They are
  property-test candidates for a sampled harness (hegel) — the complement
  mode when exhaustive checking is out of reach.
- **Re-open trigger:** an allocation-free execution path in the selection
  core lands → port `kani-probe/` to it and rerun `just verify-kani`.

Raw evidence: matrix in `docs/research/kani-probe.md` (this report's
companion on the same branch), per-harness logs and timings under
`target/kani-probe-*` (gitignored).
