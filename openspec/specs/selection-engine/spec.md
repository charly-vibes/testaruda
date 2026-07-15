# Selection Engine

## Purpose

The selection computation — evaluating the change set against the transitive-closure transpose to produce the affected test set.

## Requirements

### Requirement: TIA-SEL-001 — Affected set computation

When a change set is supplied, the selector SHALL compute the affected set as the union of the transpose-closure of Δ and the always-run set.

#### Scenario: Basic selection
- **GIVEN** a change set Δ and an always-run set
- **WHEN** selection is invoked
- **THEN** the selector SHALL compute affected = Δ · (D*)ᵀ ∪ always_run

### Requirement: TIA-SEL-002 — Component scoped traversal

The selector SHALL scope reverse traversal to components reachable from the change set before enumerating candidate tests.

#### Scenario: Scoped traversal
- **GIVEN** a change set in component A
- **WHEN** computing selection
- **THEN** the selector SHALL only traverse components reachable from A
- **AND** SHALL NOT traverse unrelated components

### Requirement: TIA-SEL-003 — Semiring-specific selection

Where a semiring is specified, the selector SHALL evaluate the same selection query in that semiring (Boolean → affected set, Viterbi → confidence, tropical → change-to-test distance, cost → expected duration).

#### Scenario: Semiring evaluation
- **GIVEN** a specified semiring type
- **WHEN** the selector evaluates the query
- **THEN** it SHALL produce results in that semiring (e.g. Boolean for affected set, Viterbi for confidence)

### Requirement: TIA-SEL-004 — Static edge retention

The selector SHALL never remove a static edge from consideration on the grounds that runtime evidence did not confirm it.

#### Scenario: Static edge always considered
- **GIVEN** a static edge and runtime evidence that does not confirm it
- **WHEN** selection is computed
- **THEN** the static edge SHALL remain in consideration
- **AND** SHALL NOT be removed

### Requirement: TIA-SEL-005 — Deterministic ordering

Where deterministic ordering is requested, the selector SHALL emit the affected set in a stable, reproducible order.

#### Scenario: Stable ordering
- **GIVEN** identical inputs and store state
- **WHEN** deterministic ordering is requested
- **THEN** the selector SHALL produce an identical ordering of affected tests

### Requirement: TIA-SEL-006 — Duration ordering

Where a duration ordering is requested, the selector SHALL order selected tests by descending recorded mean duration.

#### Scenario: Duration-based sort
- **GIVEN** a request for duration ordering
- **WHEN** the selector emits the affected set
- **THEN** it SHALL order tests by descending recorded mean duration

### Requirement: TIA-SEL-007 — Predictive ranking constraints

Where predictive ranking is enabled, the selector SHALL apply ranking only as a re-ordering or cap over the already-computed recall-safe affected set, and SHALL NOT remove any always-run member.

#### Scenario: Ranking preserves recall
- **GIVEN** predictive ranking is enabled
- **WHEN** selection is computed
- **THEN** ranking SHALL only re-order or cap the affected set
- **AND** SHALL NOT remove any always-run member

### Requirement: TIA-SEL-008 — Ordering flag value validation

The CLI SHALL validate the value of the `--ordering` flag against an enumerated set of recognized values. If an unrecognized value is supplied, the CLI SHALL emit an error listing the valid values and SHALL NOT silently default to any behavior. The CLI SHALL use the same validation mechanism used for other enum-style flags (e.g. mutual-exclusivity groups for `--agent`/`--json`/`--pre-edit`).

#### Scenario: Recognized ordering value
- **GIVEN** an ordering value `"deterministic"`
- **WHEN** `--ordering deterministic` is passed
- **THEN** the CLI SHALL accept the value
- **AND** SHALL apply the corresponding ordering

#### Scenario: Unrecognized ordering value
- **GIVEN** an ordering value `"banana"`
- **WHEN** `--ordering banana` is passed
- **THEN** the CLI SHALL emit an error
- **AND** SHALL list valid values in the error message
- **AND** SHALL NOT silently fall back to a default ordering