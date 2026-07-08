# Runtime Feedback

## Purpose

Ingesting test results and observed dependency evidence to update the model — runtime edges, run history, and idempotent ingestion.

## Requirements

### Requirement: TIA-RUN-001 — Runtime edge creation

When test results and coverage are ingested, the ingestor SHALL create or update runtime edges with origin `runtime`.

#### Scenario: Runtime edge ingestion
- **GIVEN** test results with coverage data
- **WHEN** they are ingested
- **THEN** the ingestor SHALL create or update edges with origin `runtime`

### Requirement: TIA-RUN-002 — Coverage-discovered edges

When coverage indicates a test exercised a content unit not linked by any static edge, the ingestor SHALL record that runtime edge so the dependency becomes selectable on future changes.

#### Scenario: New runtime dependency
- **GIVEN** coverage data showing a test exercised a content unit not in its static edges
- **WHEN** results are ingested
- **THEN** the ingestor SHALL record a new runtime edge between them

### Requirement: TIA-RUN-003 — External input recording

When an adapter reports external inputs read at runtime (config files, env vars, fixture files, reflectively loaded modules), the ingestor SHALL record them as runtime edges to the corresponding content units.

#### Scenario: External input edge
- **GIVEN** an adapter reporting external inputs consumed at runtime
- **WHEN** results are ingested
- **THEN** the ingestor SHALL record runtime edges to those external inputs

### Requirement: TIA-RUN-004 — Run history update

When results are ingested, the ingestor SHALL update per-test run history, timing, and failure-rate statistics.

#### Scenario: History update
- **GIVEN** ingested test results
- **WHEN** ingestion completes
- **THEN** per-test run history, timing, and failure-rate statistics SHALL be updated

### Requirement: TIA-RUN-005 — Idempotent ingestion

The ingestor SHALL be idempotent with respect to re-ingestion of the same run. Each run payload SHALL carry a unique run-identity key (a caller-supplied or adapter-generated opaque string, e.g. a UUID). Before performing any write, the ingestor SHALL check whether that key has already been recorded and skip ingestion if so. If no run-identity key is present in the payload, the ingestor SHALL reject the request with a diagnostic rather than proceed without dedup protection.

#### Scenario: Duplicate ingestion skipped
- **GIVEN** a run payload with a run-identity key already recorded
- **WHEN** the payload is ingested again
- **THEN** the ingestor SHALL skip all writes
- **AND** report the duplicate

#### Scenario: Missing run-identity key rejection
- **GIVEN** a run payload without a run-identity key
- **WHEN** ingestion is attempted
- **THEN** the ingestor SHALL reject the request with a diagnostic

### Requirement: TIA-RUN-006 — Environment recording

While ingesting a run, the ingestor SHALL record the environment fingerprint under which the run executed.

#### Scenario: Environment fingerprint on ingest
- **GIVEN** a run payload with environment metadata
- **WHEN** ingestion is performed
- **THEN** the environment fingerprint SHALL be recorded alongside the results