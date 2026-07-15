//! testaruda — Language-agnostic test selection engine.
//!
//! From a code change, compute the set of tests that must run — modelled as the
//! transpose of a provenance-semiring dependency relation, evaluated incrementally,
//! under a recall-first soundness invariant.
//!
//! Binary: `testaruda` · Config: `testaruda.toml` · License: Apache 2.0
//!
//! ## Architecture
//!
//! This is the **reference implementation** per TIA-SRS v0.2 (the spec formerly
//! titled `tia`, renamed to `testaruda` in this repo). The engine uses:
//!
//! - **Ascent** (embedded Datalog with lattice support) for the selection query
//! - **SQLite** for persistence of the dependency graph
//! - **Content-addressed blob store** for per-run payloads
//! - **JSON over stdin/stdout** for the language-adapter protocol
//!
//! ## Quick Start
//!
//! ```bash
//! testaruda init          # Create store and config
//! testaruda select        # Compute affected tests from changes
//! testaruda ingest        # Ingest run results to update the model
//! ```

pub mod adapter;
mod change;
mod cli;
pub mod config;
mod engine;
pub mod provenance;
pub mod selector;
mod store;

pub mod agent;

pub use change::ChangeSet;
pub use engine::{Engine, Origin, SelectedTest, Selection, TestOrdering, WitnessEdge, ONE};
pub use provenance::{
    BooleanSemiring, ProvenanceSemiring, SemiringValue, TropicalSemiring, ViterbiSemiring,
};
pub use selector::Selector;
pub use store::Store;
