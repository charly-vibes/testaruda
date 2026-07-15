# LLM Agent Mode

## Purpose

Selection output format for consumption by LLM coding agents — structured JSON, byte-stable output, reason chains, and coverage gap surfacing.
## Requirements
### Requirement: TIA-AGENT-001 — Structured agent output

Where the agent output format is requested, the CLI SHALL emit a structured JSON object containing the selection, per-test reasons, confidence, changed units, and summary statistics. Each per-test entry SHALL include the test's numeric `id`, string `node_id` (file path), boolean `always_run`, optional `fallback_reason` string explaining why the test is always-run when dependency edges are missing, an array `reason_chain`, and numeric `confidence`.

#### Scenario: Agent JSON output with node_id and fallback_reason
- **GIVEN** a request for agent output format with missing dependency edges
- **WHEN** selection is computed
- **THEN** the CLI SHALL emit a JSON object with selection, per-test reasons, confidence, changed units, and summary statistics
- **AND** each per-test entry SHALL include `id`, `node_id`, `always_run`, `optional fallback_reason`, `reason_chain`, and `confidence`
- **AND** when `always_run` is `true` and edges are missing, `fallback_reason` SHALL explain why

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

### Requirement: TIA-AGENT-005 — Pre-edit blast radius (structured JSON)

Where pre-edit mode is requested, the CLI SHALL report the blast radius of a proposed change without requiring the edit to be applied. The output SHALL be a structured JSON object, following the same pattern as `--agent` output (`"format": "testaruda-agent-v1"`), containing at minimum: the set of changed files, the set of affected tests, and a summary of changed/affected counts.

#### Scenario: Pre-edit analysis
- **GIVEN** proposed file changes and a pre-edit mode request
- **WHEN** the CLI evaluates the blast radius
- **THEN** it SHALL report which tests would be affected without applying the edit
- **AND** the output SHALL be a structured JSON object with a versioned format field
- **AND** the output SHALL include changed files, affected tests, and summary counts

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

### Requirement: TIA-AGENT-008 — Schema and agent-facing documentation

The agent output format (`"format": "testaruda-agent-v1"`) SHALL have an accompanying JSON Schema file in the repository describing the full payload shape. The repository SHALL include a `docs/agent-mode.md` document explaining how AI coding agents should consume testaruda's agent output — including interpreting `has_coverage_gaps`, `fallback_reason`, `coverage_gaps` fields and integrating them into a decision loop (e.g. "write a test before merging if `has_coverage_gaps: true`").

#### Scenario: Schema shipped
- **GIVEN** the agent output format `"format": "testaruda-agent-v1"`
- **WHEN** the release artifact is built
- **THEN** a JSON Schema file SHALL exist in the repository at a canonical
  path (e.g. `schemas/agent-output-v1.json`)
- **AND** the schema SHALL describe all fields of the agent output payload

#### Scenario: Agent documentation
- **GIVEN** the repository
- **WHEN** looking for agent integration guidance
- **THEN** `docs/agent-mode.md` SHALL exist
- **AND** it SHALL explain each field's semantics
- **AND** it SHALL describe how an AI agent should use the tool

