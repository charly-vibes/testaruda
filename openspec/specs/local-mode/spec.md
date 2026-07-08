# Local Developer Mode

## Purpose

Selection behavior when used locally against the working tree — uncommitted changes, daemon mode, watch mode, and offline operation.

## Requirements

### Requirement: TIA-LOCAL-001 — Working tree selection

When invoked locally against the working tree, the CLI SHALL compute selection from uncommitted changes.

#### Scenario: Local uncommitted changes
- **GIVEN** uncommitted changes in the working tree
- **WHEN** the CLI is invoked locally
- **THEN** it SHALL compute selection from those uncommitted changes

### Requirement: TIA-LOCAL-002 — Daemon mode

Where a daemon is running, the CLI SHALL reuse a cached in-memory graph to return selection with low latency.

#### Scenario: Warm daemon cache
- **GIVEN** a running daemon with a cached in-memory graph
- **WHEN** selection is requested
- **THEN** the CLI SHALL reuse the cached graph for low-latency selection

### Requirement: TIA-LOCAL-003 — Watch mode

Where watch mode is enabled, the CLI SHALL recompute the affected set on each saved change.

#### Scenario: File save triggers recomputation
- **GIVEN** watch mode is enabled
- **WHEN** a file is saved
- **THEN** the CLI SHALL recompute the affected set

### Requirement: TIA-LOCAL-004 — Local failure rerun

The CLI SHALL always re-run tests that failed in the developer's most recent local run.

#### Scenario: Local failure rerun
- **GIVEN** tests that failed in the most recent local run
- **WHEN** a new local selection is computed
- **THEN** those tests SHALL be included in the always-run set

### Requirement: TIA-LOCAL-005 — Offline operation

The CLI SHALL operate without network access in local mode.

#### Scenario: Offline selection
- **GIVEN** no network access
- **WHEN** the CLI is used in local mode
- **THEN** it SHALL still compute selection successfully