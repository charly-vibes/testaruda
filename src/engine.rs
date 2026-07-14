//! Core engine — Ascent-embedded selection query.
//!
//! See TIA-ARCH-001 through TIA-ARCH-010 and TIA-ENG-001 through TIA-ENG-016
//! for the full design constraints.

use ascent::{ascent, Dual};

use crate::change::ChangeSet;
use crate::store::Store;

/// Selection ordering mode.
///
/// Controls how the selected test set is ordered before being returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestOrdering {
    /// No specific ordering — results in Ascent's internal iteration order.
    #[default]
    Default,
    /// Byte-stable ordering: sort by test ID (TIA-SEL-005).
    ///
    /// Guarantees identical output for identical inputs and store state.
    Deterministic,
    /// Order by descending recorded mean duration (TIA-SEL-006).
    ///
    /// Tests with no recorded history are placed at the end.
    ByDuration,
}

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

    lattice impact_conf(u32, u32);
    lattice test_conf(u32, u32);
    impact_conf(c, ONE) <-- changed(c);
    impact_conf(c, ONE) <-- unresolved(c);
    impact_conf(a, ((*w as u64 * *d as u64) / ONE as u64) as u32) <-- cu_dep(a, b, _, w), impact_conf(b, d);
    test_conf(t, ((*w as u64 * *d as u64) / ONE as u64) as u32) <-- test_dep(t, c, _, w), impact_conf(c, d);
    test_conf(t, ONE) <-- always_run(t);

    // ===== Distance (tropical min-plus: Dual flips lub to min) =====

    lattice impact_dist(u32, Dual<u32>);
    lattice test_dist(u32, Dual<u32>);
    impact_dist(c, Dual(0)) <-- changed(c);
    impact_dist(c, Dual(0)) <-- unresolved(c);
    impact_dist(a, Dual(d + 1)) <-- cu_dep(a, b, _, _), impact_dist(b, ?Dual(d));
    test_dist(t, Dual(d + 1)) <-- test_dep(t, c, _, _), impact_dist(c, ?Dual(d));

    // ===== Minimal-witness predecessors (TIA-ENG-009) =====

    relation cu_pred(u32, u32, Origin);
    relation test_pred(u32, u32, Origin);
    cu_pred(a, b, o) <-- cu_dep(a, b, o, _), impact_dist(a, ?Dual(da)), impact_dist(b, ?Dual(db)), if *da == *db + 1;
    test_pred(t, c, o) <-- test_dep(t, c, o, _), test_dist(t, ?Dual(dt)), impact_dist(c, ?Dual(dc)), if *dt == *dc + 1;
}

/// The reference selection engine, borrowing a store.
pub struct Engine<'a> {
    store: &'a Store,
}

impl<'a> Engine<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Run the selection query against the given change set with default ordering.
    pub fn select(&self, delta: &ChangeSet) -> miette::Result<Selection> {
        self.select_with_ordering(delta, TestOrdering::Default)
    }

    /// Run the selection query against the given change set with a specific ordering.
    pub fn select_with_ordering(
        &self,
        delta: &ChangeSet,
        ordering: TestOrdering,
    ) -> miette::Result<Selection> {
        let ctx = self.store.load_selection_context(delta)?;
        self.select_with_context(ctx, ordering)
    }

    /// Run the selection query with a pre-loaded selection context.
    ///
    /// Useful when the caller needs the selection context for additional
    /// processing (e.g., agent output format) and wants to avoid a second
    /// `load_selection_context` call that would see stale fingerprints.
    pub fn select_with_context(
        &self,
        ctx: crate::store::SelectionContext,
        ordering: TestOrdering,
    ) -> miette::Result<Selection> {

        let mut prog = AscentProgram {
            changed: ctx.changed.iter().map(|&c| (c,)).collect(),
            unresolved: ctx.unresolved.iter().map(|&c| (c,)).collect(),
            cu_dep: ctx.cu_deps.clone(),
            test_dep: ctx.test_deps.clone(),
            always_run: ctx.always_run.iter().map(|&t| (t,)).collect(),
            comp_fallback: ctx.comp_fallback.iter().map(|&k| (k,)).collect(),
            test_comp: ctx.test_comp.clone(),
            ..Default::default()
        };

        // Run the Ascent program
        prog.run();

        // Collect results — iterate over the `affected` relation
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

            affected.push(SelectedTest {
                id: t,
                confidence: conf,
                distance: dist,
                witness: if witness.is_empty() {
                    None
                } else {
                    Some(witness)
                },
            });
        }

        // Apply ordering (TIA-SEL-005, TIA-SEL-006, TIA-SEL-007)
        match ordering {
            TestOrdering::Default => {}
            TestOrdering::Deterministic => {
                affected.sort_by_key(|t| t.id);
            }
            TestOrdering::ByDuration => {
                let durations = self.store.load_mean_durations()?;
                affected.sort_by(|a, b| {
                    let da = durations.get(&a.id).copied().unwrap_or(0);
                    let db = durations.get(&b.id).copied().unwrap_or(0);
                    // Descending: higher duration first
                    db.cmp(&da).then_with(|| a.id.cmp(&b.id))
                });
            }
        }

        Ok(Selection {
            changed_count: ctx.changed.len(),
            selected_count: affected.len(),
            tests: affected,
        })
    }
}

/// A node in the witness chain.
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
}

/// The full selection result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    pub changed_count: usize,
    pub selected_count: usize,
    pub tests: Vec<SelectedTest>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascent_program_compiles() {
        let prog = AscentProgram::default();
        assert!(prog.changed.is_empty());
        assert!(prog.affected.is_empty());
    }
}
