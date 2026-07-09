# Engine

## Purpose

Reference implementation design constraints — the Ascent-embedded selection engine, its rule set, incrementality mechanism, and scale-up path.

## Requirements

### Requirement: TIA-ENG-001 — Single static binary

The reference implementation SHALL be distributed as a single statically linked binary that embeds the selection engine in-process.

#### Scenario: Binary distribution
- **GIVEN** the reference implementation
- **WHEN** it is built
- **THEN** it SHALL be a single statically linked binary
- **AND** the selection engine SHALL be embedded in-process

### Requirement: TIA-ENG-002 — No separate engine process

The reference implementation SHALL NOT require a separate engine process to compute a selection.

#### Scenario: Self-contained selection
- **GIVEN** the reference implementation
- **WHEN** a selection is computed
- **THEN** no separate engine process SHALL be required

### Requirement: TIA-ENG-003 — Ascent engine

The reference implementation SHALL use Ascent as the embedded logic engine for the selection query.

#### Scenario: Ascent dependency
- **GIVEN** the reference implementation's Cargo.toml
- **WHEN** inspecting dependencies
- **THEN** ascent SHALL be listed as a dependency
- **AND** the selection query SHALL be embedded via the `ascent!` macro

### Requirement: TIA-ENG-004 — Lattice columns for semirings

The reference implementation SHALL represent each semiring as a lattice column so that selection, confidence, and distance are produced by the same rule set under different lattice types.

#### Scenario: Lattice-based semirings
- **GIVEN** the Ascent rule set
- **WHEN** examining relation types
- **THEN** each semiring SHALL be represented as a lattice column
- **AND** the same rules SHALL produce selection, confidence, and distance under different lattice types

### Requirement: TIA-ENG-005 — Union-find transitive closure

The reference implementation SHALL compute transitive dependency closure using a union-find–backed relation.

#### Scenario: Union-find for closure
- **GIVEN** a dependency graph
- **WHEN** computing transitive closure
- **THEN** the implementation SHALL use a union-find–backed relation

### Requirement: TIA-ENG-006 — Data-parallel per-component selection

The reference implementation SHALL evaluate per-component selection using the engine's data-parallel mode.

#### Scenario: Data-parallel engine evaluation
- **GIVEN** multiple components to evaluate
- **WHEN** selection is computed
- **THEN** the implementation SHALL use the engine's data-parallel mode (e.g. `ascent_par!`)

### Requirement: TIA-ENG-007 — Change scoping and component cache

The reference implementation SHALL provide cross-invocation incrementality through change scoping and the content-addressed component cache (TIA-COMP-010) rather than a persistent streaming engine.

#### Scenario: Cache-based incrementality
- **GIVEN** repeated invocations with overlapping inputs
- **WHEN** a new selection is computed
- **THEN** the implementation SHALL reuse cached component decisions when fingerprints match
- **AND** SHALL NOT use a persistent streaming engine

### Requirement: TIA-ENG-008 — Scoped residual evaluation

The reference implementation SHALL evaluate the selection query over only the change-scoped residual subgraph.

#### Scenario: Scoped query
- **GIVEN** a change set Δ
- **WHEN** the Ascent program runs
- **THEN** it SHALL only load facts for the scope-bounded subgraph
- **AND** SHALL NOT load the full graph

### Requirement: TIA-ENG-009 — Minimal-witness explanation

For explanation, the reference implementation SHALL compute a minimal-witness derivation (a single shortest reason chain) as a lattice value sufficient to satisfy TIA-PROV-002 and TIA-AGENT-003, without requiring full why-provenance for routine selection.

#### Scenario: Shortest-path witness
- **GIVEN** a selected test
- **WHEN** its reason is requested
- **THEN** the implementation SHALL return a minimal-witness derivation (shortest reason chain)
- **AND** SHALL NOT require full why-provenance

### Requirement: TIA-ENG-010 — Soufflé oracle for full provenance

Where full why-provenance or independent validation is required, the reference implementation SHALL be able to evaluate the same rule set through an out-of-process Soufflé oracle.

#### Scenario: Soufflé validation
- **GIVEN** a need for full why-provenance
- **WHEN** the oracle command is invoked
- **THEN** the same rule set SHALL be evaluated through Soufflé
- **AND** results SHALL be comparable to the Ascent evaluation

### Requirement: TIA-ENG-011 — Shadow mode cross-check

While operating in shadow mode (TIA-VER-001), the reference implementation SHALL be able to cross-check its selections against the Soufflé oracle and flag divergences.

#### Scenario: Shadow mode divergence detection
- **GIVEN** shadow mode is active
- **WHEN** the Ascent selection completes
- **THEN** it MAY be cross-checked against the Soufflé oracle
- **AND** any divergences SHALL be flagged

### Requirement: TIA-ENG-012 — Engine-independent rule set

The selection rule set SHALL be engine-independent, such that it can be re-targeted to an alternative engine without changing the dependency model or any `TIA-ARCH-*` behaviour.

#### Scenario: Rule set portability
- **GIVEN** the selection rule set
- **WHEN** targeting a different engine (e.g. Soufflé, DBSP)
- **THEN** the dependency model and ARCH behaviours SHALL remain unchanged
- **AND** only the rule syntax SHALL need adaptation

### Requirement: TIA-ENG-013 — DBSP scale-up path

Where long-running, fully-incremental selection at monorepo scale with retractions is required, the system SHALL be re-targetable to a streaming incremental engine (e.g. DBSP) as a documented scale-up path.

#### Scenario: Scale-up documentation
- **GIVEN** a need for streaming IVM at scale
- **WHEN** evaluating scale-up options
- **THEN** DBSP/Feldera SHALL be the documented scale-up path

### Requirement: TIA-ENG-014 — Engine migration path

If a selected engine becomes unmaintained, then the system SHALL be re-targetable to an alternative engine through the engine-independent rule set without loss of `TIA-ARCH-*` behaviour.

#### Scenario: Engine migration
- **GIVEN** an unmaintained engine
- **WHEN** migrating to a replacement
- **THEN** the ARCH behaviours SHALL be preserved
- **AND** only the rule syntax SHALL need updating

### Requirement: TIA-ENG-015 — Store independence

The reference implementation SHALL persist the dependency graph and statistics in the store of §5.10 independently of the engine's in-memory working state.

#### Scenario: Independent persistence
- **GIVEN** the engine's in-memory state
- **WHEN** the process exits
- **THEN** the dependency graph and statistics SHALL have been persisted in the store
- **AND** the in-memory state SHALL be ephemeral

### Requirement: TIA-ENG-016 — Memory safety and subprocess isolation

The reference implementation core SHALL be memory-safe and SHALL isolate untrusted adapter execution to subprocesses (cf. TIA-SEC-002), keeping the engine free of repository code execution.

#### Scenario: Subprocess isolation
- **GIVEN** untrusted adapter code
- **WHEN** it is executed
- **THEN** it SHALL run in a separate subprocess
- **AND** the engine process SHALL remain free of repository code execution