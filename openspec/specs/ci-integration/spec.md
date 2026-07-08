# CI Integration

## Purpose

Selection behavior in CI pipeline mode — exit codes, shadow mode, machine-readable plan output, and result ingestion.

## Requirements

### Requirement: TIA-CI-001 — Success exit code

When selection completes successfully, the CLI SHALL exit with code `0`.

#### Scenario: Successful selection
- **GIVEN** a selection that completes without errors
- **WHEN** the CLI exits
- **THEN** the exit code SHALL be 0

### Requirement: TIA-CI-002 — Full-run signal

If confidence requires a full run, then the CLI SHALL exit with code `10` to signal "run everything."

#### Scenario: Low confidence full run
- **GIVEN** confidence below threshold requires a full run
- **WHEN** the CLI exits
- **THEN** the exit code SHALL be 10

### Requirement: TIA-CI-003 — Empty selection signal

When the full selection set (reachability ∪ always-run ∪ fallback, where fallback is scoped per affected component per TIA-SAFE-003) is empty, the CLI SHALL exit with code `20` to signal "safe to skip."

#### Scenario: Empty selection
- **GIVEN** no tests are selected (reachability, always-run, and fallback all empty)
- **WHEN** the CLI exits
- **THEN** the exit code SHALL be 20

### Requirement: TIA-CI-004 — Error exit code

If a non-recoverable error occurs, then the CLI SHALL exit with a non-zero code distinct from `10` and `20`.

#### Scenario: Hard error
- **GIVEN** a non-recoverable error (e.g. database corruption)
- **WHEN** the CLI exits
- **THEN** the exit code SHALL be non-zero
- **AND** SHALL NOT be 10 or 20

### Requirement: TIA-CI-005 — Fallback default action

The CLI SHALL treat exit code `10` and any unknown condition as "run all tests," never as "run no tests."

#### Scenario: Unknown condition safety
- **GIVEN** an unknown or unexpected exit condition
- **WHEN** the CI pipeline interprets the result
- **THEN** it SHALL run all tests
- **AND** SHALL NOT skip any tests

### Requirement: TIA-CI-006 — Machine-readable plan

Where a machine-readable format is requested, the CLI SHALL emit the selection plan as JSON or a runner-native plan.

#### Scenario: JSON plan output
- **GIVEN** a request for machine-readable output
- **WHEN** selection is computed
- **THEN** the CLI SHALL emit the plan as JSON or a runner-native format

### Requirement: TIA-CI-007 — Shadow mode

Where shadow mode is enabled, the CLI SHALL compute and record the selection but report that all tests should run.

#### Scenario: Shadow mode selection
- **GIVEN** shadow mode is enabled
- **WHEN** selection is computed
- **THEN** the CLI SHALL compute and record the selection
- **BUT** SHALL report that all tests should run

### Requirement: TIA-CI-008 — Result ingestion

When a CI run finishes, the CLI SHALL accept ingestion of its results to update the model.

#### Scenario: CI results ingestion
- **GIVEN** a completed CI run with results
- **WHEN** the ingest command is invoked
- **THEN** the CLI SHALL accept and process the results to update the model