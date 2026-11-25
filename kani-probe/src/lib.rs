//! Purpose: Kani bounded-model-checking probe over testaruda's selection
//! pipeline (beads `testaruda-jdpi`) — tractability measurement with an
//! extend/hold/retire gate, NOT an assurance lane. Outcomes and
//! dispositions live in docs/research/kani-probe.md; the gate script is
//! scripts/check-kani-probe.sh (invoked as `just verify-kani`).
//!
//! Responsibilities: harnesses are partitioned per probe family
//! (panic/overflow-freedom, selection soundness, confidence ceiling,
//! fallback completeness), each over nondeterministic `kani::any()`
//! INTEGER inputs only — no strings, no map-bearing domain types on the
//! solver path (catherine-amh: container modeling is the measured wall).
//!
//! PROVENANCE (read before citing anything here): the ascent! program,
//! the four pipeline functions, and the result structs below are a
//! VERBATIM COPY of src/engine.rs @ main (ffe7c19) — 'SelectionContext'
//! re-declared from src/store.rs (its unused `current_environment`
//! String field omitted; the pipeline never reads it and a string on
//! the solver path is the measured wall). The whole-crate `#![cfg(kani)]`
//! exists because Kani 0.68.0 cannot compile testaruda's real dependency
//! closure (backtrace/miette E0659 against Kani's std prelude); a real
//! lane must extract this code as a dependency-light kernel crate (the
//! catherine-engine pattern). DRIFT INVALIDATES PROOFS, NOT THE
//! TRACTABILITY MEASUREMENT — no result here may be cited as assurance
//! about the shipped engine.
#![cfg(kani)]

use ascent::{ascent, Dual};


/// Origin of a dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Origin {
    Static,
    Runtime,
    Manual,
}

/// Confidence constant: represents 1.0 in ppm (parts-per-million) integer space.
pub const ONE: u32 = 1_000_000;

ascent! {
    // ===== EDB: facts loaded from the store =====

    /// Changed content units (from diff)
    relation changed(u32);
    /// Unresolved files (over-approximate — TIA-SAFE-004)
    relation unresolved(u32);
    /// Content-unit dependency: (dependent, dependency, origin, confidence)
    relation cu_dep(u32, u32, Origin, u32);
    /// Test-to-content-unit dependency: (test, content_unit, origin, confidence)
    relation test_dep(u32, u32, Origin, u32);
    /// Tests that must always run (TIA-SAFE-007)
    relation always_run(u32);
    /// Components with forced fallback
    relation comp_fallback(u32);
    /// Test-to-component mapping
    relation test_comp(u32, u32);
    /// Quarantined tests (TIA-SAFE-010)
    relation quarantined(u32);
    /// Invocation-level quality multiplier in ppm (TIA-CONF-002)
    relation invocation_quality(u32);

    // ===== Boolean selection = reverse reachability (ARCH-004/005, SEL-001) =====

    /// Content units impacted (directly or transitively) by the change set
    relation impacted(u32);
    /// Tests affected by the change
    relation affected(u32);

    impacted(c) <-- changed(c);
    impacted(c) <-- unresolved(c);                          // over-approximate (SAFE-004)
    impacted(a) <-- cu_dep(a, b, _, _), impacted(b);
    affected(t) <-- test_dep(t, c, _, _), impacted(c);
    affected(t) <-- always_run(t);                          // SAFE-007 union
    affected(t) <-- comp_fallback(k), test_comp(t, k);      // fallback

    // ===== Confidence (Viterbi: lub = max, product along path) =====
    // Invocation-level quality factor multiplies the base confidence
    // (TIA-CONF-002: effective confidence reflects dependency evidence quality).

    lattice impact_conf(u32, u32);
    lattice test_conf(u32, u32);
    impact_conf(c, q) <-- changed(c), invocation_quality(q);
    impact_conf(c, q) <-- unresolved(c), invocation_quality(q);
    impact_conf(a, ((*w as u64 * *d as u64) / ONE as u64) as u32) <-- cu_dep(a, b, _, w), impact_conf(b, d);
    test_conf(t, ((*w as u64 * *d as u64) / ONE as u64) as u32) <-- test_dep(t, c, _, w), impact_conf(c, d);
    test_conf(t, ONE) <-- always_run(t);

    // ===== Distance (tropical min-plus: Dual flips lub to min) =====

    lattice impact_dist(u32, Dual<u32>);
    lattice test_dist(u32, Dual<u32>);
    impact_dist(c, Dual(0)) <-- changed(c);
    impact_dist(c, Dual(0)) <-- unresolved(c);
    impact_dist(a, Dual(d.saturating_add(1))) <-- cu_dep(a, b, _, _), impact_dist(b, ?Dual(d));
    test_dist(t, Dual(d.saturating_add(1))) <-- test_dep(t, c, _, _), impact_dist(c, ?Dual(d));

    // ===== Minimal-witness predecessors (TIA-ENG-009) =====

    relation cu_pred(u32, u32, Origin);
    relation test_pred(u32, u32, Origin);
    cu_pred(a, b, o) <-- cu_dep(a, b, o, _), impact_dist(a, ?Dual(da)), impact_dist(b, ?Dual(db)), if *da == db.saturating_add(1);
    test_pred(t, c, o) <-- test_dep(t, c, o, _), test_dist(t, ?Dual(dt)), impact_dist(c, ?Dual(dc)), if *dt == dc.saturating_add(1);
}


/// Build an AscentProgram from the selection context.
fn build_ascent_program(
    ctx: &SelectionContext,
    comp_fallback: &[(u32,)],
) -> AscentProgram {
    AscentProgram {
        changed: ctx.changed.iter().map(|&c| (c,)).collect(),
        unresolved: ctx.unresolved.iter().map(|&c| (c,)).collect(),
        cu_dep: ctx.cu_deps.clone(),
        test_dep: ctx.test_deps.clone(),
        always_run: ctx.always_run.iter().map(|&t| (t,)).collect(),
        comp_fallback: comp_fallback.to_vec(),
        test_comp: ctx.test_comp.clone(),
        quarantined: ctx.quarantined.iter().map(|&t| (t,)).collect(),
        invocation_quality: vec![(ctx.invocation_quality,)],
        ..Default::default()
    }
}

/// Compute which components need fallback based on confidence floor.
fn compute_confidence_floor(
    ctx: &SelectionContext,
    prog: &AscentProgram,
) -> Vec<u32> {
    let always_run_ids: std::collections::HashSet<u32> = ctx.always_run.iter().copied().collect();
    let mut comp_confs: std::collections::HashMap<u32, Vec<f64>> = std::collections::HashMap::new();

    for &(t, c) in &prog.test_conf {
        if always_run_ids.contains(&t) {
            continue;
        }
        if !prog.test_pred.iter().any(|&(tid, _, _)| tid == t) {
            continue;
        }
        // Bind the component key (second element) — the tuple is
        // (test_id, component_key); binding (k, _) wrongly keyed the floor
        // map by test id, so membership never resolved (testaruda-2bxl).
        if let Some(&(_, k)) = prog.test_comp.iter().find(|&&(tid, _)| tid == t) {
            let conf = c as f64 / ONE as f64;
            comp_confs.entry(k).or_default().push(conf);
        }
    }

    let mut needs_fallback = Vec::new();
    let threshold = ctx.confidence_threshold as f64 / ONE as f64;
    for (comp, confs) in &comp_confs {
        let min_conf = confs.iter().cloned().fold(f64::MAX, f64::min);
        if min_conf < threshold {
            needs_fallback.push(*comp);
        }
    }
    needs_fallback
}

/// Re-run the Ascent program with extended comp_fallback.
fn rerun_with_fallback(
    ctx: &SelectionContext,
    comp_fallback: &mut Vec<(u32,)>,
    needs_fallback: &[u32],
) -> AscentProgram {
    for comp in needs_fallback {
        if !comp_fallback.iter().any(|&(k,)| k == *comp) {
            comp_fallback.push((*comp,));
        }
    }
    let mut prog2 = build_ascent_program(ctx, comp_fallback);
    prog2.run();
    prog2
}

/// Collect selection results from the Ascent program's `affected` relation.
fn collect_selection_results(prog: &AscentProgram) -> Vec<SelectedTest> {
    let mut affected: Vec<SelectedTest> = Vec::new();

    for &(t,) in &prog.affected {
        let conf = prog
            .test_conf
            .iter()
            .find(|&&(tid, _)| tid == t)
            .map(|&(_, c)| c as f64 / ONE as f64)
            .unwrap_or(0.0);

        let dist = prog
            .test_dist
            .iter()
            .find(|&&(tid, _)| tid == t)
            .map(|&(_, Dual(d))| d);

        let witness: Vec<WitnessEdge> = prog
            .test_pred
            .iter()
            .filter(|&&(tid, _, _)| tid == t)
            .map(|&(_, c, o)| WitnessEdge {
                content_unit: c,
                origin: o,
            })
            .collect();

        let is_quarantined = prog.quarantined.iter().any(|&(qt,)| qt == t);

        affected.push(SelectedTest {
            id: t,
            confidence: conf,
            distance: dist,
            witness: if witness.is_empty() {
                None
            } else {
                Some(witness)
            },
            quarantined: is_quarantined,
        });
    }
    affected
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WitnessEdge {
    pub content_unit: u32,
    pub origin: Origin,
}

/// A single selected test with its metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectedTest {
    pub id: u32,
    pub confidence: f64,
    pub distance: Option<u32>,
    pub witness: Option<Vec<WitnessEdge>>,
    /// Whether this test is quarantined (TIA-SAFE-010).
    /// Quarantined tests are selected-and-run but their outcome is
    /// excluded from pass/fail trust calculations.
    pub quarantined: bool,
}

// ===== probe scaffolding (NOT part of the verbatim copy) =====

/// The selection input shape, re-declared from src/store.rs with the
/// unused `current_environment` String field omitted (see header).
pub struct SelectionContext {
    pub changed: Vec<u32>,
    pub unresolved: Vec<u32>,
    pub cu_deps: Vec<(u32, u32, Origin, u32)>,
    pub test_deps: Vec<(u32, u32, Origin, u32)>,
    pub always_run: Vec<u32>,
    pub comp_fallback: Vec<u32>,
    pub test_comp: Vec<(u32, u32)>,
    pub quarantined: Vec<u32>,
    pub invocation_quality: u32,
    pub confidence_threshold: u32,
}

/// Upper bound on any single content-unit/test id in a harness input
/// (ids are masked into `0..=MAX_ID` — a 4-element integer domain).
const MAX_ID: u32 = 3;

/// Nondeterministic id in `0..=MAX_ID`.
fn any_id() -> u32 {
    kani::any::<u32>() % (MAX_ID + 1)
}

/// Nondeterministic confidence weight in the valid domain `0..=ONE`
/// (ppm space — the store guarantees weights never exceed `ONE`).
fn any_weight() -> u32 {
    kani::any::<u32>() % (ONE + 1)
}

/// Nondeterministic edge origin.
fn any_origin() -> Origin {
    match kani::any::<u8>() % 3 {
        0 => Origin::Static,
        1 => Origin::Runtime,
        _ => Origin::Manual,
    }
}

/// Nondeterministic bounded `SelectionContext`: at most 2 changed units,
/// 1 unresolved, 2 cu deps, 2 test deps, 1 always-run, 1 fallback comp,
/// 2 test-comp pairs, 1 quarantine.
fn nondet_ctx() -> SelectionContext {
    let n_changed = kani::any::<usize>() % 3;
    let mut changed = Vec::new();
    for _ in 0..n_changed {
        changed.push(any_id());
    }

    let mut unresolved = Vec::new();
    if kani::any::<bool>() {
        unresolved.push(any_id());
    }

    let n_cu = kani::any::<usize>() % 3;
    let mut cu_deps = Vec::new();
    for _ in 0..n_cu {
        cu_deps.push((any_id(), any_id(), any_origin(), any_weight()));
    }

    let n_td = kani::any::<usize>() % 3;
    let mut test_deps = Vec::new();
    for _ in 0..n_td {
        test_deps.push((any_id(), any_id(), any_origin(), any_weight()));
    }

    let mut always_run = Vec::new();
    if kani::any::<bool>() {
        always_run.push(any_id());
    }

    let mut comp_fallback = Vec::new();
    if kani::any::<bool>() {
        comp_fallback.push(any_id());
    }

    let n_tc = kani::any::<usize>() % 3;
    let mut test_comp = Vec::new();
    for _ in 0..n_tc {
        test_comp.push((any_id(), any_id()));
    }

    let mut quarantined = Vec::new();
    if kani::any::<bool>() {
        quarantined.push(any_id());
    }

    SelectionContext {
        changed,
        unresolved,
        cu_deps,
        test_deps,
        always_run,
        comp_fallback,
        test_comp,
        quarantined,
        invocation_quality: any_weight(),
        confidence_threshold: any_weight(),
    }
}

/// Run the pure pipeline exactly as `Engine::select_with_context` does,
/// minus the store-backed ordering step.
fn run_pipeline(ctx: &SelectionContext) -> (Vec<SelectedTest>, Vec<u32>) {
    let mut comp_fallback: Vec<(u32,)> = ctx.comp_fallback.iter().map(|&k| (k,)).collect();

    let mut prog = build_ascent_program(ctx, &comp_fallback);
    prog.run();

    let needs_fallback = compute_confidence_floor(ctx, &prog);
    if !needs_fallback.is_empty() {
        prog = rerun_with_fallback(ctx, &mut comp_fallback, &needs_fallback);
    }

    let selected = collect_selection_results(&prog);
    // Drop glue dominates solver cost on ascent programs (catherine-amh);
    // the probe asserts over owned data only.
    std::mem::forget(prog);
    (selected, needs_fallback)
}

/// Panic/overflow-freedom of the whole pure pipeline over the bounded
/// input space. No explicit assertion — CBMC's default safety checks
/// (arithmetic overflow, index bounds, panics) are the obligation.
#[kani::proof]
fn pipeline_panic_freedom() {
    let ctx = nondet_ctx();
    let (selected, _) = run_pipeline(&ctx);
    // Non-vacuity: the space admits selections of any of its sizes; the
    // bound itself is trivially true — the obligation is CBMC's built-in
    // safety checks over the whole path.
    assert!(selected.len() <= ctx.test_deps.len() + ctx.always_run.len() + ctx.test_comp.len() + 2 * MAX_ID as usize);
}

/// Selection soundness: every selected test is JUSTIFIED — an always-run
/// test, the source of a declared test dependency, or a member of a
/// fallback component. Nothing outside the three `affected` rules' input
/// sets may be selected (TIA-SEL-001/SAFE-007 plus the fallback rule).
#[kani::proof]
fn selection_soundness_no_phantom_tests() {
    let ctx = nondet_ctx();
    let (selected, needs_fallback) = run_pipeline(&ctx);

    let justified = |t: u32| -> bool {
        ctx.always_run.iter().any(|&x| x == t)
            || ctx.test_deps.iter().any(|&(tid, _, _, _)| tid == t)
            || (ctx.test_comp.iter().any(|&(tid, k)| tid == t && needs_fallback.contains(&k)))
    };

    for s in &selected {
        assert!(justified(s.id), "phantom test selected: {}", s.id);
    }
}

/// Confidence ceiling: within the valid weight domain (every weight ≤
/// ONE, quality ≤ ONE) every selected test's Viterbi confidence stays in
/// [0.0, 1.0] — the ppm product `(w*d)/ONE` can never exceed ONE, and
/// the f64 conversion preserves that (TIA-CONF-002 invariant).
#[kani::proof]
fn confidence_ceiling_invariant() {
    let ctx = nondet_ctx();
    let (selected, _) = run_pipeline(&ctx);

    for s in &selected {
        assert!(s.confidence >= 0.0, "negative confidence: {}", s.confidence);
        assert!(s.confidence <= 1.0, "confidence above ONE ppm ceiling: {}", s.confidence);
    }
}

/// Fallback completeness: when the confidence floor marks a component,
/// the rerun selects EVERY test mapped into that component by
/// `test_comp` — the TIA-SAFE-003 component-scoped fallback contract.
#[kani::proof]
fn fallback_completeness() {
    let ctx = nondet_ctx();
    let (selected, needs_fallback) = run_pipeline(&ctx);

    for &(t, k) in &ctx.test_comp {
        if needs_fallback.contains(&k) {
            assert!(
                selected.iter().any(|s| s.id == t),
                "fallback component {} test {} not selected",
                k,
                t
            );
        }
    }
}

// ===== layer B: the tractable surface (no prog.run()) =====
//
// Layer A harnesses above include the ascent fixpoint and hit the
// measured wall (hashbrown/std internals — see docs/research/
// kani-probe.md). These harnesses check the NON-ascent shipped logic
// directly over nondeterministic, well-formed program relations: the
// confidence-floor grouping (regression surface for testaruda-2bxl),
// result collection, and their panic-freedom.

/// Nondeterministic test id for layer-B relation entries (8-element
/// domain — enough to exercise grouping collisions).
fn any_tid() -> u32 {
    kani::any::<u32>() % 8
}

/// Nondeterministic `AscentProgram` with populated layer-B relations
/// (test_conf, test_comp, test_pred, test_dist, affected, quarantined —
/// ≤2 entries each, confidences in the valid `0..=ONE` weight domain);
/// the input relations (changed, cu_dep, …) stay empty: the floor and
/// collect functions never read them.
fn nondet_prog() -> AscentProgram {
    let n_conf = kani::any::<usize>() % 3;
    let mut test_conf = Vec::new();
    for _ in 0..n_conf {
        test_conf.push((any_tid(), any_weight()));
    }

    let n_tc = kani::any::<usize>() % 3;
    let mut test_comp = Vec::new();
    for _ in 0..n_tc {
        test_comp.push((any_tid(), kani::any::<u32>() % 4));
    }

    let n_pred = kani::any::<usize>() % 3;
    let mut test_pred = Vec::new();
    for _ in 0..n_pred {
        test_pred.push((any_tid(), any_id(), any_origin()));
    }

    let n_dist = kani::any::<usize>() % 3;
    let mut test_dist = Vec::new();
    for _ in 0..n_dist {
        test_dist.push((any_tid(), Dual(kani::any::<u32>() % 16)));
    }

    let n_aff = kani::any::<usize>() % 3;
    let mut affected = Vec::new();
    for _ in 0..n_aff {
        affected.push((any_tid(),));
    }

    let mut quarantined = Vec::new();
    if kani::any::<bool>() {
        quarantined.push((any_tid(),));
    }

    AscentProgram {
        test_conf,
        test_comp,
        test_pred,
        test_dist,
        affected,
        quarantined,
        ..Default::default()
    }
}

/// Minimal context for the floor: only `always_run` and
/// `confidence_threshold` are read.
fn nondet_floor_ctx() -> SelectionContext {
    let mut always_run = Vec::new();
    if kani::any::<bool>() {
        always_run.push(any_tid());
    }
    SelectionContext {
        changed: vec![],
        unresolved: vec![],
        cu_deps: vec![],
        test_deps: vec![],
        always_run,
        comp_fallback: vec![],
        test_comp: vec![],
        quarantined: vec![],
        invocation_quality: ONE,
        confidence_threshold: any_weight(),
    }
}

/// Panic/overflow-freedom of the confidence floor and result collection
/// over nondeterministic well-formed program relations (no ascent
/// fixpoint on the path).
#[kani::proof]
fn floor_collect_panic_freedom() {
    let ctx = nondet_floor_ctx();
    let prog = nondet_prog();
    let needs = compute_confidence_floor(&ctx, &prog);
    let selected = collect_selection_results(&prog);
    assert!(needs.len() <= 4);
    assert!(selected.len() <= prog.affected.len());
}

/// The confidence-floor contract, two-sided against the documented
/// grouping (skip always-run, skip tests without a witness predecessor,
/// group by the test→component key — the post-testaruda-2bxl semantics):
/// a component is flagged ⟺ its minimum grouped confidence is below the
/// threshold.
#[kani::proof]
fn floor_contract_two_sided() {
    let ctx = nondet_floor_ctx();
    let prog = nondet_prog();
    let needs = compute_confidence_floor(&ctx, &prog);

    // Re-derive the documented grouping from the program relations.
    let mut groups: std::collections::HashMap<u32, Vec<f64>> = std::collections::HashMap::new();
    for &(t, c) in &prog.test_conf {
        if ctx.always_run.iter().any(|&x| x == t) {
            continue;
        }
        if !prog.test_pred.iter().any(|&(tid, _, _)| tid == t) {
            continue;
        }
        if let Some(&(_, k)) = prog.test_comp.iter().find(|&&(tid, _)| tid == t) {
            groups.entry(k).or_default().push(c as f64 / ONE as f64);
        }
    }
    let threshold = ctx.confidence_threshold as f64 / ONE as f64;

    // Completeness: every documented-low component is flagged.
    for (k, confs) in &groups {
        let min_conf = confs.iter().cloned().fold(f64::MAX, f64::min);
        if min_conf < threshold {
            assert!(needs.contains(k), "low-confidence component {} missed by floor", k);
        }
    }
    // Soundness: every flagged component is documented-low.
    for k in &needs {
        let confs = groups
            .get(k)
            .expect("flagged component has no grouped confidences");
        let min_conf = confs.iter().cloned().fold(f64::MAX, f64::min);
        assert!(min_conf < threshold, "component {} flagged though min conf ≥ threshold", k);
    }
}

/// Result collection consistency: each selected test's fields are
/// exactly the relation lookups — confidence from `test_conf` (0.0 when
/// absent), distance from `test_dist`, quarantine from `quarantined`,
/// and the witness from ALL `test_pred` edges of the test (None only
/// when there are none).
#[kani::proof]
fn collect_lookup_consistency() {
    let prog = nondet_prog();
    let selected = collect_selection_results(&prog);

    for s in &selected {
        let t = s.id;
        let conf_expect = prog
            .test_conf
            .iter()
            .find(|&&(tid, _)| tid == t)
            .map(|&(_, c)| c as f64 / ONE as f64)
            .unwrap_or(0.0);
        assert!(s.confidence == conf_expect, "confidence mismatch for {}", t);

        let dist_expect = prog
            .test_dist
            .iter()
            .find(|&&(tid, _)| tid == t)
            .map(|&(_, Dual(d))| d);
        assert!(s.distance == dist_expect, "distance mismatch for {}", t);

        let q_expect = prog.quarantined.iter().any(|&(qt,)| qt == t);
        assert!(s.quarantined == q_expect, "quarantine mismatch for {}", t);

        let edges: Vec<(u32, Origin)> = prog
            .test_pred
            .iter()
            .filter(|&&(tid, _, _)| tid == t)
            .map(|&(_, c, o)| (c, o))
            .collect();
        match &s.witness {
            Some(w) => {
                assert!(w.len() == edges.len(), "witness length mismatch for {}", t);
                for e in w {
                    assert!(
                        edges.iter().any(|&(c, o)| c == e.content_unit && o == e.origin),
                        "witness edge not in test_pred for {}",
                        t
                    );
                }
            }
            None => assert!(edges.is_empty(), "witness dropped for {}", t),
        }
    }
}

