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

### Requirement: TIA-LOCAL-006 — Store-readiness precondition

Before performing any store-dependent operation (`select`, `discover`, `ingest`, `explain`, `graph`), the CLI SHALL verify that the testaruda store is initialized (`.testaruda/store.db` exists with the expected schema). If the store is not initialized, the CLI SHALL emit a human-actionable error message suggesting the user run `testaruda init` first, and SHALL NOT attempt any SQL operation.

#### Scenario: Select before init
- **GIVEN** a directory with no `testaruda init` run
- **WHEN** `select` is invoked
- **THEN** the CLI SHALL detect the missing store
- **AND** SHALL emit an error message indicating the store is missing
  and suggesting `testaruda init` (e.g. `Error: no testaruda store found
  — run 'testaruda init' first`)
- **AND** SHALL NOT attempt any SQL operation

#### Scenario: Other commands before init
- **GIVEN** a directory with no `testaruda init` run
- **WHEN** `discover`, `ingest`, `explain`, or `graph` is invoked
- **THEN** the CLI SHALL detect the missing store
- **AND** SHALL emit an actionable error message

#### Scenario: Corrupted store
- **GIVEN** a directory where `.testaruda/store.db` exists but is corrupted
  or has an incompatible schema version
- **WHEN** any store-dependent command is invoked
- **THEN** the CLI SHALL detect the corruption or version mismatch
- **AND** SHALL emit an error message explaining the issue
- **AND** SHALL suggest re-running `testaruda init` or using a migration tool