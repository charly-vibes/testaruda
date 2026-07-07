//! Selection engine — bridges store context to the Ascent query.
//!
//! See TIA-SEL-001 through TIA-SEL-007.

use crate::engine::{Engine, Selection};
use crate::store::Store;
use crate::change::ChangeSet;

/// A thin wrapper around the engine that provides a simplified API.
pub struct Selector;

impl Selector {
    /// Compute the affected set for the given change.
    pub fn select(store: &Store, delta: &ChangeSet) -> miette::Result<Selection> {
        let engine = Engine::new(store);
        engine.select(delta)
    }

    /// Compute the affected set, transferring store ownership.
    pub fn select_with_store(store: Store, delta: &ChangeSet) -> miette::Result<Selection> {
        let engine = Engine::new(&store);
        engine.select(delta)
    }
}