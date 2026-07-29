# Observability

## Purpose

Metrics, structured logs, and graph introspection — exporting the dependency graph, explaining selections, and emitting operational metrics.
## Requirements
### Requirement: TIA-OBS-001 — Dependency graph export

testaruda's `--json` output for dependency-graph export and selection explanation SHALL wrap its payload in `genesis::envelope::Envelope`, so its JSON shape matches wai/dont/pretender/espectacular across the suite.

#### Scenario: select emits shared envelope

- **WHEN** `testaruda select --json` is run after adopting genesis
- **THEN** the emitted JSON SHALL have top-level keys `ok`, `envelope_version`, `cli_version`, `envelope_kind`, `data`, `warnings`, `hints`, `meta`
- **AND** the selected-test set SHALL be nested under `data`.

### Requirement: TIA-OBS-002 — Selection explanation

The core SHALL be able to explain any selection it produced.

#### Scenario: Post-hoc explanation
- **GIVEN** a prior selection
- **WHEN** explanation is requested
- **THEN** the core SHALL be able to explain it

### Requirement: TIA-OBS-003 — Metrics emission

The core SHALL emit metrics including selection rate, estimated time saved, fallback rate, flakiness rate, and missed-selection count.

#### Scenario: Metrics output
- **GIVEN** a selection is computed
- **WHEN** metrics are requested or logged
- **THEN** the core SHALL emit selection rate, estimated time saved, fallback rate, flakiness rate, and missed-selection count

### Requirement: TIA-OBS-004 — Structured logs

The core SHALL emit structured logs for each selection and ingestion.

#### Scenario: Logging on operations
- **GIVEN** a selection or ingestion operation
- **WHEN** it completes
- **THEN** structured logs SHALL be emitted for the operation

### Requirement: TIA-OBS-005 — Log routing to stderr in machine-readable modes

When a machine-readable output mode (`--json`, `--agent`, or `--pre-edit`) is active, the core SHALL route all structured log output to stderr, leaving stdout clean for parseable JSON. When no machine-readable mode is active, the core MAY emit logs to either stderr or stdout; stderr is recommended.

#### Scenario: Machine-readable mode log routing
- **GIVEN** a machine-readable output mode (`--json`, `--agent`, or `--pre-edit`) is active
- **WHEN** the core emits structured logs
- **THEN** all log output SHALL appear on stderr
- **AND** stdout SHALL contain only the requested machine-readable output

#### Scenario: Human mode log placement
- **GIVEN** no machine-readable output mode is active
- **WHEN** the core emits structured logs
- **THEN** it MAY emit logs to stderr or stdout
- **AND** stderr is the recommended target

