# Observability

## Purpose

Metrics, structured logs, and graph introspection — exporting the dependency graph, explaining selections, and emitting operational metrics.

## Requirements

### Requirement: TIA-OBS-001 — Dependency graph export

When requested, the core SHALL export the current dependency graph in a documented format.

#### Scenario: Graph export request
- **GIVEN** a populated dependency graph
- **WHEN** export is requested
- **THEN** the core SHALL export the graph in a documented format

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