# Provenance and Explainability

## Purpose

Computing reason chains for selections, enabling query-time explanation of why each test was or was not selected.

## Requirements

### Requirement: TIA-PROV-001 — Provenance derivation

The core SHALL compute, for each selection, the provenance expression that derived it.

#### Scenario: Provenance per test
- **GIVEN** a change set and dependency graph
- **WHEN** a test is selected
- **THEN** the core SHALL have derived a provenance expression for it

### Requirement: TIA-PROV-002 — Reason chain

When a test item is selected, the core SHALL be able to produce its reason chain as the set of edges and changed content units that caused its selection, each annotated with origin.

#### Scenario: Reason chain output
- **GIVEN** a selected test item
- **WHEN** its reason is requested
- **THEN** the core SHALL return the chain of edges and changed content units that caused selection
- **AND** each edge SHALL be annotated with its origin (static, runtime, or manual)

### Requirement: TIA-PROV-003 — Exclusion reason

When a test item is excluded, the core SHALL be able to produce an explicit exclusion reason.

#### Scenario: Exclusion explanation
- **GIVEN** a test item that was not selected
- **WHEN** its exclusion reason is requested
- **THEN** the core SHALL produce an explicit reason (e.g. no change reaches it, no edges exist)

### Requirement: TIA-PROV-004 — Explain a specific test

When queried about a specific test, the core SHALL report whether and why that test was or was not selected for the current change set.

#### Scenario: Explain endpoint
- **GIVEN** a test ID and a change set
- **WHEN** the explain command is used
- **THEN** the core SHALL report whether the test was or was not selected
- **AND** SHALL explain why

### Requirement: TIA-PROV-005 — Persisted provenance

The store SHALL persist provenance such that a past selection can be re-explained without re-running selection.

#### Scenario: Historical explanation
- **GIVEN** a past selection with persisted provenance
- **WHEN** its explanation is requested later
- **THEN** the store SHALL provide the explanation without re-running the selection query