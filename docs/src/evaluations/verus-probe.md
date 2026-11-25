# Verus functional-proof probe — selection Boolean core

> **Historical report:** scoped to subject commit `ffe7c19` on the
> `verus-probe` branch (2026-11-25). Findings describe the selection
> Boolean core as modeled in `verus-probe/model/selection_reachability.rs`
> and the pinned toolchain (Verus 0.2026.11.22.1ce6a59 / toolchain 1.100.0);
> a later model, engine, or toolchain may invalidate them.

**What this document is.** A self-contained record of the functional-verification
probe over testaruda's selection Boolean core — what was proved, what was
measured, and why the verdict is HOLD (probe kept, lane not built). Written to
the amnesia test: every term is defined here. The raw matrix, model source,
parity driver, and gate script live on the branch (`verus-probe/`,
`scripts/check-verus-probe.sh`, `docs/research/verus-probe.md`); this page is
the durable summary.

**Date:** 2026-11-25. **Subject:** the selection algorithm's Boolean core —
impacted (reverse-reachability closure of changed ∪ unresolved content units
over cu-dep edges) and affected (always-run ∪ impacted-dep tests ∪
fallback-component tests) — as a bounded dense-id model at `ffe7c19`.
**Beads:** `testaruda-rqvc` (probe recorded). **Provenance of the idea:** the
follow-up to the Kani BMC probe (`testaruda-jdpi`, verdict RETIRE): Kani could
not decide the family's safety properties; Verus attacks the complementary
question — full *functional* specs discharged by an SMT solver (Z3) rather
than symbolic execution over all inputs.

**Verdict in one line:** every proof obligation in the fragment discharged —
6 verified, 0 errors, ~1.2 s wall, no infrastructure — so the technique is
proven and the probe is kept (HOLD), but the verified artifact is the probe
model, not the shipped engine, and wiring it to the real algorithm is
lane-sized openspec work with the same cost/benefit wall that retired the
Kani lane.

## Terms

- **Verus** — a Rust verifier built on rustc/Z3: code annotated with
  `requires`/`ensures` contracts, loop `invariant`s, and `proof` blocks is
  discharged by an SMT solver. `VERIFIED` means the property holds for *all*
  inputs satisfying the preconditions — exhaustive, unlike sampled property
  tests. This distribution verifies only (`--crate-type lib`); it does not
  link binaries.
- **Functional spec** — not "doesn't panic" but "computes exactly X": here,
  `affected` returns the sorted ascending set of always-run ∪ impacted-dep ∪
  fallback-component tests, and `impacted` is exactly the k-step closure of
  the seeds.
- **Snapshot relaxation** — exec strategy where each fixpoint pass reads only
  the pre-pass vector and writes a fresh one. Pass k's output is then *by
  construction* the k-step closure, making `exec == spec` a direct induction
  with no cycle-shortening or cardinality lemmas. This is the design lesson
  applied from the Kani probe: the in-place worklist variants are where
  allocation-shape proofs explode.
- **Three-valued outcome** — per-unit VERIFIED / ERROR / EXHAUSTED; a run
  with no `verification results::` line is ERROR, never silently green, and
  the gate fails.
- **Provenance skeleton guard** — the gate reduces the verified model's exec
  functions and the parity driver's transcription to their sequence of loop
  guards + effectful statements (proof blocks brace-counted out, ghost
  bindings dropped, verus-only syntax mapped to plain Rust) and diffs them
  in order. The transcription cannot be linked (no binary emission), so this
  order-sensitive skeleton diff is the drift guard; it caught three drifts
  during development.

## Method

- Pinned release (`verus-version.toml`, same pin as catherine's
  `add-verus-verification-lane`): sha256-provisioned into
  `~/.local/share/verus/<release>`; the gate refuses any other installed
  release.
- One model proof unit (`verus-probe/model/selection_reachability.rs`, 6
  obligations: panic-freedom + exact closure membership for `impacted`,
  sorted-ascending + membership + composition contract for `affected`),
  verified with `--crate-type lib` under a 300 s wall cap.
- Parity gate (`verus-probe/parity/`, detached plain-cargo crate): the
  transcribed exec walk vs a plain-Rust reference of the shipped Boolean
  semantics, over 14 fixture groups (empty, seed-only, chain, diamond,
  cycle, unreachable, all-seeds, affected variants, end-to-end composition),
  with the `chain-contract` line (closure of {0} over a 4-chain = exactly
  {0,1,2,3}) as the non-vacuity witness.

## Results

| unit / check | outcome | wall |
|---|---|---|
| `selection_reachability.rs` (6 obligations) | VERIFIED (6 verified, 0 errors) | 1196 ms |
| provenance skeleton ×2 | match | — |
| parity: 14 fixture groups | PASS | <1 s |

The resource profile inverts the Kani probe: there 7/7 harnesses
undetermined at 80–250 s each under an 8 GB systemd scope; here all
obligations decide sub-second with no memory scope, no per-obligation
budget, and no solver tuning — the SMT framing decides vstd
sequence/bool obligations that CBMC's symbolic execution drowned in.

**Boundary of the claim.** The proof holds for the *model*: dense bounded
ids (`CU_CAP`/`T_CAP` = 8 — within which the k-step closure equals the
mathematical closure), snapshot-relaxation exec, out-of-domain inputs
excluded by `requires`. It is not a claim about the shipped
`src/selection` code, and no gate output says otherwise.

## Verdict: HOLD

Kept: the model, the parity driver, the skeleton guard, and the gate
(`just verify-verus`) — proven machinery at ~1.2 s per run. Not built: a
permanent verification lane. Wiring the proof to the shipped algorithm
requires extracting the core dependency-light (catherine-engine interning
pattern) or proving ascent!-generated code directly (vstd-over-hashbrown
cost unmapped) — the same restructure-vs-ascent wall that retired the
Kani lane, recorded as the re-open trigger: if the selection core is ever
extracted dependency-light, move this model beside it as its spec crate
and propose the lane via openspec.

Sampled-property complement: the reachable invariants (closure
membership, affected contract) are written down in the model and are
property-test candidates for hegel (`testaruda-lsz1`).
