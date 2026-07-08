# Confidence Scoring

## Purpose

Confidence computation from dependency evidence quality — Viterbi path confidence, invocation-level quality signals, and fallback gating.

## Requirements

### Requirement: TIA-CONF-001 — Confidence range

The core SHALL compute a selection confidence in the range [0, 1].

#### Scenario: Confidence output
- **GIVEN** a selected test
- **WHEN** its selection confidence is computed
- **THEN** the confidence SHALL be in the range [0, 1]

### Requirement: TIA-CONF-002 — Per-invocation confidence floor

The core SHALL apply a per-invocation confidence floor derived from at least coverage freshness, adapter resolution ratio, history depth, and environment match, such that the effective path confidence reported by TIA-CONF-001 reflects the quality of the dependency evidence for the current invocation. These are invocation-level quality signals and SHALL NOT mutate stored edge weights, ensuring TIA-REL-001 (deterministic selections for identical store state) is preserved.

#### Scenario: Invocation-level quality adjustment
- **GIVEN** stale coverage data, low adapter resolution, and poor environment match
- **WHEN** selection confidence is computed
- **THEN** the effective confidence SHALL be adjusted downward from stored edge weights
- **AND** stored edge weights SHALL remain unchanged

### Requirement: TIA-CONF-003 — Confidence reporting

The core SHALL report the confidence value to every consumer interface.

#### Scenario: Confidence in output
- **GIVEN** any consumer interface (CLI, JSON, agent output)
- **WHEN** selection results are emitted
- **THEN** the confidence value SHALL be included in the output

### Requirement: TIA-CONF-004 — Confidence meaning documentation

The core SHALL document, wherever confidence is reported, that confidence gates fallback and is not a probability that the suite is correct.

#### Scenario: Confidence disclaimer
- **GIVEN** confidence values reported in output
- **WHEN** documentation accompanies the output
- **THEN** the documentation SHALL state that confidence gates fallback and is not a probability of correctness