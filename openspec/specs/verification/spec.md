# Verification and Rollout

## Purpose

Validation of the soundness invariant, shadow mode gating, seeded-fault recall testing, and ranking calibration before promotion.

## Requirements

### Requirement: TIA-VER-001 — Shadow mode gating

The system SHALL be verifiable in shadow mode, computing selections without gating, before it is permitted to gate.

#### Scenario: Shadow mode before enforcement
- **GIVEN** a fresh installation of testaruda
- **WHEN** it is first deployed
- **THEN** it SHALL operate in shadow mode (compute but not gate)
- **AND** SHALL NOT gate until promoted

### Requirement: TIA-VER-002 — Zero-missed-selection gate

The system SHALL record zero missed-selection incidents over a defined evaluation window as the precondition for enabling enforcing mode.

#### Scenario: Enabling enforcement
- **GIVEN** an evaluation window with zero missed-selection incidents
- **WHEN** the evaluation window completes
- **THEN** enforcing mode MAY be enabled
- **AND** if any incident occurred, enforcing mode SHALL remain disabled

### Requirement: TIA-VER-003 — Full-run reconciliation

The system SHALL use periodic full-run reconciliation as a continuous verification mechanism for the over-approximation invariant.

#### Scenario: Periodic reconciliation
- **GIVEN** a scheduled full run
- **WHEN** it completes
- **THEN** selections from the period SHALL be compared against the full run results
- **AND** any missed-selection incidents SHALL be recorded

### Requirement: TIA-VER-004 — Seeded-fault recall test

The soundness invariant (TIA-SAFE-001) SHALL be verified by a seeded-fault recall test in which every seeded regression's fault-revealing test is selected.

#### Scenario: Seeded fault evaluation
- **GIVEN** a set of seeded regressions with known fault-revealing tests
- **WHEN** selection is computed for each regression
- **THEN** every fault-revealing test SHALL be selected
- **AND** any missed selection SHALL be recorded as a soundness violation

### Requirement: TIA-VER-005 — Predictive ranking calibration gate

Where predictive ranking is enabled, it SHALL pass a calibration gate meeting defined test-failure-recall and change-recall targets on a held-out recent window before promotion.

#### Scenario: Ranking calibration
- **GIVEN** predictive ranking training is complete
- **WHEN** the model is evaluated on a held-out window
- **THEN** it SHALL meet defined recall targets for test failures and changes
- **AND** SHALL NOT be promoted if targets are not met