//! Selection engine — bridges store context to the Ascent query.
//!
//! See TIA-SEL-001 through TIA-SEL-007.

use crate::change::ChangeSet;
use crate::engine::{Engine, Selection, TestOrdering};
use crate::store::Store;

/// A thin wrapper around the engine that provides a simplified API.
pub struct Selector;

impl Selector {
    /// Compute the affected set for the given change with default ordering.
    pub fn select(store: &Store, delta: &ChangeSet) -> miette::Result<Selection> {
        let engine = Engine::new(store);
        engine.select(delta)
    }

    /// Compute the affected set with a specific ordering (TIA-SEL-005, TIA-SEL-006).
    pub fn select_with_ordering(
        store: &Store,
        delta: &ChangeSet,
        ordering: TestOrdering,
    ) -> miette::Result<Selection> {
        let engine = Engine::new(store);
        engine.select_with_ordering(delta, ordering)
    }

    /// Compute the affected set, transferring store ownership.
    pub fn select_with_store(store: Store, delta: &ChangeSet) -> miette::Result<Selection> {
        let engine = Engine::new(&store);
        engine.select(delta)
    }
}
