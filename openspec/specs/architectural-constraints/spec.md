# Architectural Constraints

## Purpose

Foundational, engine-independent architectural constraints that bind the design of the entire system. These are the unifying abstraction that all other requirements instantiate.

## Requirements

### Requirement: TIA-ARCH-001 — K-relation representation

The core SHALL represent all dependency data as a single K-relation valued in a configured commutative semiring.

#### Scenario: Dependency data as K-relation
- **GIVEN** dependency data loaded from the store
- **WHEN** the core processes it for selection
- **THEN** it SHALL represent it as a single K-relation valued in a configured commutative semiring
- **AND** the semiring SHALL be configurable per invocation

### Requirement: TIA-ARCH-002 — Provenance semiring as master

The core SHALL treat the provenance (polynomial) semiring as the master representation from which all other semiring values are derived.

#### Scenario: Derived semiring values
- **GIVEN** dependency edges stored in the store
- **WHEN** any semiring value is needed
- **THEN** the provenance (polynomial) semiring SHALL be the master representation
- **AND** all other semiring values SHALL be derived from it

### Requirement: TIA-ARCH-003 — Positive reachability by homomorphism

The core SHALL derive the positive reachability results (selection (Boolean), distance (tropical), cost/scheduling, and explanation (provenance polynomial — the master)) from the provenance representation by semiring homomorphism, such that no positively-derived result can disagree with the master computation. The always-run set (TIA-SAFE-007), confidence-threshold fallback (TIA-SAFE-002), and duration ranking (TIA-SEL-006) involve negation-as-failure or aggregation over external signals; they are not derived by semiring closure but contributed as input facts to the selection relation, and are not required to be homomorphic images of the provenance column. Path confidence (Viterbi) annotates the same paths but is likewise excluded from the homomorphism guarantee.

#### Scenario: Homomorphism correctness
- **GIVEN** a provenance polynomial for a test item
- **WHEN** the Boolean selection is derived via semiring homomorphism
- **THEN** the Boolean result SHALL NOT be false if the provenance polynomial has any positive term
- **AND** the tropical distance SHALL be consistent with the shortest path in the provenance polynomial

#### Scenario: Always-run exclusion from homomorphism
- **GIVEN** a test in the always-run set
- **WHEN** selection is computed
- **THEN** its selection SHALL NOT be required to be a homomorphic image of the provenance column
- **AND** it SHALL be contributed as an input fact to the selection relation

### Requirement: TIA-ARCH-004 — Selection as transpose of closure

The selector SHALL compute the affected set by evaluating the change set against the transpose of the transitive-closure relation.

#### Scenario: Transpose evaluation
- **GIVEN** a change set Δ and a transitive-closure relation D*
- **WHEN** computing the affected set
- **THEN** the selector SHALL evaluate Δ · (D*)ᵀ

### Requirement: TIA-ARCH-005 — Least fixpoint for transitive dependency

The core SHALL compute transitive dependency as the least fixpoint (semiring star) of the one-step dependency relation.

#### Scenario: Fixpoint computation
- **GIVEN** a one-step dependency relation
- **WHEN** computing transitive closure
- **THEN** the core SHALL compute it as the least fixpoint of the one-step relation

### Requirement: TIA-ARCH-006 — Scope-bounded recomputation

The core SHALL compute selection as a scope-bounded recomputation restricted to the subgraph from which the change set is backward-reachable (i.e. the transitive reverse-closure of the changed content units), rather than the full graph.

#### Scenario: Scope bounding
- **GIVEN** a change set Δ in a large dependency graph
- **WHEN** computing selection
- **THEN** the core SHALL restrict recomputation to the transitive reverse-closure of the changed content units
- **AND** SHALL NOT traverse the full graph

### Requirement: TIA-ARCH-007 — Fusion by semiring addition

The core SHALL fuse static, runtime, and manual edges by semiring addition over a common edge set, without origin-specific merge logic.

#### Scenario: Edge fusion
- **GIVEN** static, runtime, and manual edges for the same (test_item, content_unit, environment) triple
- **WHEN** computing selection
- **THEN** the core SHALL fuse them by semiring addition
- **AND** SHALL NOT use origin-specific merge logic

### Requirement: TIA-ARCH-008 — Language-agnostic core

The core SHALL contain no language- or framework-specific logic; all such logic SHALL reside behind the adapter interface.

#### Scenario: Adapter isolation
- **GIVEN** requirements for a new language
- **WHEN** adding support for it
- **THEN** no changes SHALL be required to the core
- **AND** all language-specific logic SHALL be implemented in an adapter

### Requirement: TIA-ARCH-009 — No test execution

The core SHALL NOT execute tests, compile, or build; it SHALL only select tests and emit native-runner arguments for an external executor.

#### Scenario: Selection-only operation
- **GIVEN** an invocation of the selection command
- **WHEN** execution completes
- **THEN** the core SHALL NOT have executed any tests
- **AND** SHALL only have emitted native-runner arguments

### Requirement: TIA-ARCH-010 — Associative composition

The core SHALL compute selection across components and repositories by associative composition of per-unit K-relations, such that the result is independent of composition order.

#### Scenario: Composition order independence
- **GIVEN** components A, B, and C with dependency edges between them
- **WHEN** computing selection by composing (A·B)·C and A·(B·C)
- **THEN** both compositions SHALL produce identical affected sets