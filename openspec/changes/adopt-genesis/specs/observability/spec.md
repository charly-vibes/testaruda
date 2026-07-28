# observability spec delta: adopt genesis

## MODIFIED Requirements

### Requirement: TIA-OBS-001 — Dependency graph export

testaruda's `--json` output for dependency-graph export and selection explanation SHALL wrap its payload in `genesis::envelope::Envelope`, so its JSON shape matches wai/dont/pretender/espectacular across the suite.

#### Scenario: select emits shared envelope

- **WHEN** `testaruda select --json` is run after adopting genesis
- **THEN** the emitted JSON SHALL have top-level keys `ok`, `envelope_version`, `cli_version`, `envelope_kind`, `data`, `warnings`, `hints`, `meta`
- **AND** the selected-test set SHALL be nested under `data`.