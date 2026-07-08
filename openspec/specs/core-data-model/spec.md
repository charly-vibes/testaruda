# Core Data Model

## Purpose

Entities and relationships that form the dependency graph data model — content units, test items, dependency edges, and run history.

## Requirements

### Requirement: TIA-CORE-001 — Content unit identification

The store SHALL identify each content unit by the tuple `(component, path, symbol?)`.

#### Scenario: Content unit lookup
- **GIVEN** a content unit in a component
- **WHEN** it is stored or retrieved
- **THEN** its identity SHALL be the tuple (component, path, symbol?)
- **AND** the symbol field SHALL be optional (nullable)

### Requirement: TIA-CORE-002 — Content fingerprint

The store SHALL record, for each content unit, a content fingerprint computed from the normalized unit content.

#### Scenario: Fingerprint storage
- **GIVEN** a content unit with known content
- **WHEN** it is inserted into the store
- **THEN** a content fingerprint SHALL be computed from the normalized content
- **AND** the fingerprint SHALL be recorded alongside the content unit

### Requirement: TIA-CORE-003 — Content unit kind classification

The store SHALL classify each content unit by kind ∈ {source, config, fixture, lockfile, external}.

#### Scenario: Kind classification
- **GIVEN** a content unit being inserted
- **WHEN** the store records it
- **THEN** it SHALL be classified as one of: source, config, fixture, lockfile, or external

### Requirement: TIA-CORE-004 — Test item identification

The store SHALL identify each test item by the tuple `(component, adapter, node_id)`.

#### Scenario: Test item lookup
- **GIVEN** a test item in a component
- **WHEN** it is stored or retrieved
- **THEN** its identity SHALL be the tuple (component, adapter, node_id)

### Requirement: TIA-CORE-005 — Dependency edge record

The store SHALL record each dependency edge as `(test_item, content_unit, environment, origin, K-value)`.

#### Scenario: Edge recording
- **GIVEN** a dependency between a test and a content unit
- **WHEN** the dependency is recorded
- **THEN** it SHALL include test_item, content_unit, environment, origin, and K-value

### Requirement: TIA-CORE-006 — Coexisting edges

The store SHALL permit static, runtime, and manual edges to coexist for the same `(test_item, content_unit, environment)` triple.

#### Scenario: Multiple origins per triple
- **GIVEN** a test_item, content_unit, and environment triple
- **WHEN** edges of different origins exist
- **THEN** the store SHALL permit all three origins (static, runtime, manual) to coexist

### Requirement: TIA-CORE-007 — Reverse index

The store SHALL maintain a reverse index from content unit to dependent test items.

#### Scenario: Reverse lookup
- **GIVEN** a content unit
- **WHEN** querying for dependent tests
- **THEN** the store SHALL return all test items that depend on it via the reverse index

### Requirement: TIA-CORE-008 — Environment partition

The store SHALL partition all edges and statistics by environment fingerprint.

#### Scenario: Environment-scoped queries
- **GIVEN** multiple environments with different fingerprints
- **WHEN** querying edges or statistics
- **THEN** the store SHALL return results scoped to the specified environment fingerprint

### Requirement: TIA-CORE-009 — Run history

The store SHALL record per-test run history including outcome, attempt count, duration, and error signature.

#### Scenario: Run history recording
- **GIVEN** a completed test run
- **WHEN** results are ingested
- **THEN** the store SHALL record the outcome, attempt count, duration, and error signature for each test