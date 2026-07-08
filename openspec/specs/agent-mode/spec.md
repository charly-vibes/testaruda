# LLM Agent Mode

## Purpose

Selection output format for consumption by LLM coding agents — structured JSON, byte-stable output, reason chains, and coverage gap surfacing.

## Requirements

### Requirement: TIA-AGENT-001 — Structured agent output

Where the agent output format is requested, the CLI SHALL emit a structured JSON object containing the selection, per-test reasons, confidence, changed units, and summary statistics.

#### Scenario: Agent JSON output
- **GIVEN** a request for agent output format
- **WHEN** selection is computed
- **THEN** the CLI SHALL emit a JSON object with selection, per-test reasons, confidence, changed units, and summary statistics

### Requirement: TIA-AGENT-002 — Byte-stable agent output

Given the same change set and store state, the CLI in agent mode SHALL produce byte-stable output. Agent mode SHALL implicitly enforce deterministic output ordering (TIA-SEL-005) regardless of parallel evaluation (TIA-COMP-008), without requiring a separate flag from the caller.

#### Scenario: Deterministic agent output
- **GIVEN** identical inputs and store state
- **WHEN** agent mode is used twice
- **THEN** both outputs SHALL be byte-identical

### Requirement: TIA-AGENT-003 — Reason chains for selected and skipped

When the agent requests an explanation, the CLI SHALL include for each selected test its reason chain and for each skipped test its exclusion reason.

#### Scenario: Full explanation
- **GIVEN** an agent explanation request
- **WHEN** selection output is produced
- **THEN** each selected test SHALL include its reason chain
- **AND** each skipped test SHALL include its exclusion reason

### Requirement: TIA-AGENT-004 — Specific test query

When the agent queries a specific test, the CLI SHALL answer why that test was or was not selected.

#### Scenario: Single test query
- **GIVEN** an agent query for a specific test ID
- **WHEN** the CLI processes the query
- **THEN** it SHALL return whether the test was selected
- **AND** SHALL explain why

### Requirement: TIA-AGENT-005 — Pre-edit blast radius

Where pre-edit mode is requested, the CLI SHALL report the blast radius of a proposed change without requiring the edit to be applied.

#### Scenario: Pre-edit analysis
- **GIVEN** proposed file changes and a pre-edit mode request
- **WHEN** the CLI evaluates the blast radius
- **THEN** it SHALL report which tests would be affected without applying the edit

### Requirement: TIA-AGENT-006 — Coverage gap surfacing

When a changed symbol has no covering test, the CLI SHALL surface that coverage gap in agent output.

#### Scenario: Coverage gap detection
- **GIVEN** a changed symbol with no covering tests
- **WHEN** agent output is produced
- **THEN** the CLI SHALL surface the coverage gap

### Requirement: TIA-AGENT-007 — Deterministic default for agent/gate

While serving an agent or a merge gate, the CLI SHALL default to deterministic selection and SHALL NOT apply non-deterministic predictive ranking unless explicitly enabled.

#### Scenario: Deterministic agent mode
- **GIVEN** agent or merge gate mode
- **WHEN** selection is computed
- **THEN** the CLI SHALL default to deterministic selection
- **AND** SHALL NOT apply predictive ranking unless explicitly enabled