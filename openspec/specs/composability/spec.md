# Composability

## Purpose

Multi-component and multi-repository composition — bottom-up resolution, manifest export, parallel evaluation, and caching.

## Requirements

### Requirement: TIA-COMP-001 — Component graph

The store SHALL maintain a component graph distinct from the fine-grained test-to-content-unit graph.

#### Scenario: Separate graph structures
- **GIVEN** the store
- **WHEN** querying the component graph
- **THEN** it SHALL be distinct from the test-to-content-unit graph

### Requirement: TIA-COMP-002 — Inter-component edges

The store SHALL record inter-component dependency edges with an origin.

#### Scenario: Cross-component recording
- **GIVEN** a dependency between components
- **WHEN** it is stored
- **THEN** the edge SHALL include its origin (static, runtime, or manual)

### Requirement: TIA-COMP-003 — Bottom-up component resolution

When computing selection in a monorepo, the core SHALL first resolve affected components bottom-up from the change set, then select within them.

#### Scenario: Bottom-up resolution
- **GIVEN** a change set in a monorepo with multiple components
- **WHEN** selection is computed
- **THEN** affected components SHALL be resolved bottom-up from the change set
- **AND** selection SHALL be computed within resolved components

### Requirement: TIA-COMP-004 — Multi-repo manifest

Where multi-repo operation is configured, each repository SHALL be able to export a manifest of its components, their public-interface fingerprints, and a version.

#### Scenario: Manifest export
- **GIVEN** multi-repo operation configured
- **WHEN** a repository is queried
- **THEN** it SHALL export a manifest containing its components, their public-interface fingerprints, and version

### Requirement: TIA-COMP-005 — Cross-repo edge recording

When a consumer repository records a dependency on another repository, the core SHALL record an edge to that repository's published interface fingerprint and version.

#### Scenario: Cross-repo edge
- **GIVEN** a consumer repository depending on another repository
- **WHEN** the dependency is recorded
- **THEN** the edge SHALL reference the published interface fingerprint and version

### Requirement: TIA-COMP-006 — Interface change propagation

When a published interface fingerprint changes, the core SHALL mark dependent tests in consumer repositories as affected.

#### Scenario: Cross-repo change propagation
- **GIVEN** a repository whose published interface fingerprint changes
- **WHEN** selection is computed in a consumer repository
- **THEN** the dependent tests SHALL be marked as affected

### Requirement: TIA-COMP-007 — Aggregation without global lock

The core SHALL aggregate manifests across repositories without requiring a global lock or single shared database.

#### Scenario: Decentralized aggregation
- **GIVEN** manifests from multiple repositories
- **WHEN** they are aggregated
- **THEN** the core SHALL NOT require a global lock or shared database

### Requirement: TIA-COMP-008 — Parallel per-component selection

The core SHALL compute per-component selection in parallel.

#### Scenario: Parallel evaluation
- **GIVEN** multiple components to evaluate
- **WHEN** selection runs
- **THEN** per-component selection SHALL be computed in parallel

### Requirement: TIA-COMP-009 — Order-independent results

The core SHALL produce identical affected sets regardless of the order in which components or repositories are composed.

#### Scenario: Order invariance
- **GIVEN** the same dependency data processed in different component orders
- **WHEN** selection is computed
- **THEN** the affected set SHALL be identical regardless of composition order

### Requirement: TIA-COMP-010 — Cached selection decisions

The core SHALL key a component's cached selection decision on its dependency fingerprint, and SHALL reuse the cached decision when the fingerprint is unchanged.

#### Scenario: Cache reuse
- **GIVEN** a component whose dependency fingerprint matches a cached value
- **WHEN** selection is computed
- **THEN** the cached selection decision SHALL be reused

### Requirement: TIA-COMP-011 — Remote cache

Where a remote cache is configured, the core SHALL share and retrieve cached selection decisions across machines through a local-then-remote lookup.

#### Scenario: Remote cache lookup
- **GIVEN** a configured remote cache
- **WHEN** a selection decision is needed
- **THEN** the core SHALL look up the local cache first, then the remote cache

### Requirement: TIA-COMP-012 — Shard plan

Where sharding is requested, the core SHALL emit a balanced shard plan computed over recorded test durations.

#### Scenario: Duration-balanced sharding
- **GIVEN** a request for sharding
- **WHEN** the shard plan is computed
- **THEN** it SHALL balance shards based on recorded test durations