## MODIFIED Requirements

### Requirement: TIA-AGENT-001 — Structured agent output

Where the agent output format is requested, the CLI SHALL emit a structured JSON object containing the selection, per-test reasons, confidence, changed units, and summary statistics. Each per-test entry SHALL include the test's numeric `id`, string `node_id` (file path), boolean `always_run`, optional `fallback_reason` string explaining why the test is always-run when dependency edges are missing, an array `reason_chain`, and numeric `confidence`.

#### Scenario: Agent JSON output with node_id and fallback_reason
- **GIVEN** a request for agent output format with missing dependency edges
- **WHEN** selection is computed
- **THEN** the CLI SHALL emit a JSON object with selection, per-test reasons, confidence, changed units, and summary statistics
- **AND** each per-test entry SHALL include `id`, `node_id`, `always_run`, `optional fallback_reason`, `reason_chain`, and `confidence`
- **AND** when `always_run` is `true` and edges are missing, `fallback_reason` SHALL explain why