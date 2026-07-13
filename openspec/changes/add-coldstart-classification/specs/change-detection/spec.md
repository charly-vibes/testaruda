## ADDED Requirements

### Requirement: TIA-CHG-009 — Cold-start content-unit classification

A content unit with no prior fingerprint of record (i.e., first observed by the system) SHALL be classified as `unresolved`, with confidence set to zero. Repeated invocations with no intervening fingerprint change SHALL produce the same classification as the first observation.

#### Scenario: Cold-start content unit classified as unresolved
- **GIVEN** a content unit with no prior fingerprint of record in the store
- **WHEN** the change set Δ is computed
- **THEN** the content unit SHALL be classified as `unresolved`
- **AND** its confidence SHALL be set to zero, triggering the confidence-based fallback (TIA-SAFE-002)

#### Scenario: Idempotent classification across repeated invocations
- **GIVEN** a content unit that was previously classified as `unresolved` on first observation
- **AND** no intervening change has occurred
- **WHEN** the change set Δ is computed again
- **THEN** the content unit SHALL receive the same classification (`unresolved`) as on the first invocation
- **AND** its confidence SHALL remain zero
