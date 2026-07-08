# Safety

## Purpose

Soundness invariant, fallback mechanisms, and forced-selection rules that ensure recall — the over-approximation guarantee.

## Requirements

### Requirement: TIA-SAFE-001 — Over-approximation invariant

The core SHALL maintain the invariant that the modeled dependency relation over-approximates the true semantic dependency relation, such that any test that could be affected by a change is selected.

#### Scenario: Over-approximation guarantee
- **GIVEN** a change that semantically affects a test
- **WHEN** selection is computed
- **THEN** the test SHALL be selected (over-approximation ensures recall)

### Requirement: TIA-SAFE-002 — Confidence fallback

If the minimum Viterbi path confidence (TIA-CONF-001) across the **reachability-selected** tests in a component falls below the configured threshold, then the core SHALL fall back to selecting all tests in that component. If no reachability-selected tests exist for a component (i.e. the component's selection consists solely of always-run members from TIA-SAFE-007), TIA-SAFE-002 does not apply — TIA-SAFE-007 already provides sufficient recall for that component.

#### Scenario: Confidence below threshold
- **GIVEN** a component where min reachability-selected test confidence < configured threshold
- **WHEN** selection is computed
- **THEN** the core SHALL fall back to selecting all tests in that component

#### Scenario: Always-run-only component
- **GIVEN** a component whose selection consists solely of always-run members
- **WHEN** confidence is checked
- **THEN** TIA-SAFE-002 SHALL NOT apply
- **AND** no fallback is triggered

### Requirement: TIA-SAFE-003 — Scoped fallback

The core SHALL scope confidence-driven fallback to the affected component(s) and SHALL NOT escalate to a global full run unless every affected component is below threshold.

#### Scenario: Component-scoped fallback
- **GIVEN** one affected component below threshold and another above threshold
- **WHEN** fallback is triggered
- **THEN** only the below-threshold component SHALL fall back to a full run
- **AND** the above-threshold component SHALL NOT be affected

### Requirement: TIA-SAFE-004 — Unresolved file treatment

If an adapter reports unresolved files it cannot statically analyze, then the core SHALL treat the dependents of those files conservatively by force-including them or raising a component fallback.

#### Scenario: Unresolved file fallback
- **GIVEN** an adapter reporting files it cannot analyze
- **WHEN** selection is computed
- **THEN** the core SHALL force-include dependents of unresolved files or raise a component fallback

### Requirement: TIA-SAFE-005 — Environment change full run

When the environment fingerprint or a lockfile changes, the core SHALL schedule a full run for the affected environment.

#### Scenario: Environment change
- **GIVEN** a changed environment fingerprint or lockfile
- **WHEN** selection is computed
- **THEN** the core SHALL schedule a full run for the affected environment

### Requirement: TIA-SAFE-006 — Periodic full run

The core SHALL support a configurable periodic full-run schedule independent of change-based selection.

#### Scenario: Periodic full run configured
- **GIVEN** a configured periodic full-run schedule
- **WHEN** the schedule triggers
- **THEN** the core SHALL select all tests regardless of change-based selection

### Requirement: TIA-SAFE-007 — Always-run set

The core SHALL include in the always-run set every test that failed in its last recorded run, every newly added test, every test with no recorded history, and every quarantined test.

#### Scenario: Always-run composition
- **GIVEN** a test that failed in its last run, a newly added test, a test with no history, and a quarantined test
- **WHEN** selection is computed
- **THEN** all four tests SHALL be in the always-run set
- **AND** SHALL be unconditionally selected

### Requirement: TIA-SAFE-008 — Missed-selection incident

When a full run fails a test that the most recent selection would have skipped, the core SHALL record a missed-selection incident and create a `manual` edge that forces the test on the implicated change in future.

#### Scenario: Missed selection recording
- **GIVEN** a full run reveals a test failure that selection would have skipped
- **WHEN** the incident is processed
- **THEN** the core SHALL record a missed-selection incident
- **AND** create a manual edge forcing the test on the implicated change

### Requirement: TIA-SAFE-009 — Must-run rules

Where the user defines must-run rules (e.g. path globs mapped to tests), the core SHALL force-select the mapped tests when matching files change.

#### Scenario: Must-run pattern match
- **GIVEN** a must-run rule mapping `*.config` to test `config-test`
- **WHEN** a file matching `*.config` changes
- **THEN** the test `config-test` SHALL be force-selected

### Requirement: TIA-SAFE-010 — Quarantine semantics

The core SHALL treat a quarantined test as selected-and-run while excluding its outcome from pass/fail trust calculations; quarantine SHALL NOT mean skip.

#### Scenario: Quarantined test run
- **GIVEN** a quarantined test
- **WHEN** selection is computed
- **THEN** the test SHALL be selected and run
- **BUT** its outcome SHALL NOT affect pass/fail trust calculations

### Requirement: TIA-SAFE-011 — Flaky detection

When a test produces inconsistent outcomes across retried attempts in one run, the core SHALL record the outcome as flaky and update its flakiness score.

#### Scenario: Inconsistent outcomes
- **GIVEN** a test with inconsistent outcomes across retries in one run
- **WHEN** results are ingested
- **THEN** the outcome SHALL be recorded as flaky
- **AND** the test's flakiness score SHALL be updated

### Requirement: TIA-SAFE-012 — Flaky exclusion from training

Where predictive ranking training is enabled, the core SHALL exclude flaky-labeled outcomes from the training labels.

#### Scenario: Flaky exclusion
- **GIVEN** predictive ranking training is enabled
- **WHEN** training labels are computed
- **THEN** flaky-labeled outcomes SHALL be excluded from the label set