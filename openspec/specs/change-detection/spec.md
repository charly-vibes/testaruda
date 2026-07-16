# Change Detection

## Purpose

How changes are detected and the change set Δ is computed from VCS diffs, explicit file lists, and content fingerprints.
## Requirements
### Requirement: TIA-CHG-001 — VCS diff

When invoked with a base and head revision, the core SHALL derive the changed file set from the version-control diff between them.

#### Scenario: Git diff between revisions
- **GIVEN** a base revision `main` and a head revision `feature-branch`
- **WHEN** the core is invoked with both revisions
- **THEN** it SHALL run `git diff --name-only main feature-branch` (or equivalent)
- **AND** use the output as the changed file set

### Requirement: TIA-CHG-002 — Explicit file list

Where the caller supplies an explicit changed-file list, the core SHALL use that list as the change source in place of a diff.

#### Scenario: Explicit file override
- **GIVEN** an explicit list of changed files
- **WHEN** selection is invoked
- **THEN** the core SHALL use that list directly
- **AND** SHALL NOT run a VCS diff

### Requirement: TIA-CHG-003 — Fingerprint-based change set

The core SHALL compute the change set Δ as the content units whose fingerprint differs between base and head.

#### Scenario: Fingerprint comparison
- **GIVEN** base and head fingerprints for the same content unit
- **WHEN** the fingerprints differ
- **THEN** the content unit SHALL be included in Δ
- **AND** when fingerprints match, it SHALL be excluded from Δ

### Requirement: TIA-CHG-004 — Symbol-level granularity

Where the responsible adapter declares symbol granularity **and** sets the `symbol_model_complete` capability flag to `true` in its handshake (TIA-ADAPT-002) (asserting that no dependency paths exist beyond those it enumerates for the file), the core SHALL fingerprint only the changed symbols/blocks so that whitespace-only or unrelated edits within a file yield no affected tests for unchanged symbols. If the adapter does not set `symbol_model_complete` (e.g. due to reflection, macros, or dynamic dispatch), the core SHALL fall back to file-level granularity for that file to preserve TIA-SAFE-001.

#### Scenario: Symbol-level with complete model
- **GIVEN** an adapter with `symbol_model_complete: true`
- **WHEN** a symbol within a file changes but other symbols do not
- **THEN** only tests depending on the changed symbol SHALL be selected
- **AND** tests depending only on unchanged symbols SHALL NOT be selected

#### Scenario: Fallback to file-level
- **GIVEN** an adapter without `symbol_model_complete` or with `symbol_model_complete: false`
- **WHEN** any change occurs in a file
- **THEN** the core SHALL treat the entire file as changed (file-level granularity)

### Requirement: TIA-CHG-005 — Dependency fingerprint check

The core SHALL compute each test item's dependency fingerprint as a function of its dependency content-unit fingerprints and its environment fingerprint, and SHALL select a test item if and only if its dependency fingerprint changed or it is in the always-run set.

#### Scenario: Dependency fingerprint change
- **GIVEN** a test whose dependency content fingerprints have changed
- **WHEN** selection is computed
- **THEN** the test SHALL be selected
- **AND** a test whose dependency fingerprints are unchanged and is not in always-run SHALL NOT be selected

### Requirement: TIA-CHG-006 — External resource change detection

The core SHALL treat lockfiles, configuration, and adapter-declared external resources as content units subject to change detection.

#### Scenario: Lockfile change
- **GIVEN** a changed lockfile
- **WHEN** selection is computed
- **THEN** the lockfile SHALL be treated as a changed content unit
- **AND** tests depending on it SHALL be selected

### Requirement: TIA-CHG-007 — Unknown file kind fallback

If a changed file has a kind the core cannot model and no known edges, then the core SHALL raise a fallback signal for the affected component.

#### Scenario: Unmodeled file kind
- **GIVEN** a changed file of an unknown kind with no known dependency edges
- **WHEN** selection is computed
- **THEN** the core SHALL raise a fallback signal for the affected component

### Requirement: TIA-CHG-008 — CI mode working tree exclusion

While operating in CI mode, the core SHALL NOT read the local working tree as a change source.

#### Scenario: CI mode change source
- **GIVEN** CI mode is active
- **WHEN** selection is invoked
- **THEN** the core SHALL only use the provided base/head revisions or explicit file list
- **AND** SHALL NOT read the local working tree

### Requirement: TIA-CHG-009 — Init-time language detection

When the `init` command is invoked, the core SHALL probe the current directory for well-known project files to detect the project's primary language. The detected language SHALL determine the default adapter in `testaruda.toml`. When no known project file is found, the core SHALL fall back to `testaruda-adapter-rust`. A user-supplied adapter configuration SHALL always take precedence over auto-detection.

Well-known project files to probe:
- Python: `pyproject.toml`, `setup.py`, `setup.cfg`
- Rust: `Cargo.toml`
- JavaScript/TypeScript: `package.json`
- Go: `go.mod`

#### Scenario: Python project detection
- **GIVEN** a directory with `pyproject.toml` but no `Cargo.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-python"`

#### Scenario: Rust project detection
- **GIVEN** a directory with `Cargo.toml` but no `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-rust"`

#### Scenario: Unknown project fallback
- **GIVEN** a directory with no known project files
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-rust"`

#### Scenario: User override takes precedence
- **GIVEN** a Python project
- **WHEN** the user explicitly specifies adapter config during `init`
- **THEN** the user's configuration SHALL take precedence over auto-detection

### Requirement: TIA-CHG-010 — Cold-start content-unit classification

A content unit with no prior fingerprint of record (i.e., first observed by the
system) SHALL be classified as `unresolved`, with confidence set to zero.
Repeated invocations with no intervening fingerprint change SHALL produce the
same classification as the first observation.

#### Scenario: Cold-start content unit classified as unresolved
- **GIVEN** a content unit with no prior fingerprint of record in the store
- **WHEN** the change set Δ is computed
- **THEN** the content unit SHALL be classified as `unresolved`
- **AND** its confidence SHALL be set to zero, triggering the confidence-based
  fallback (TIA-SAFE-002)

#### Scenario: Idempotent classification across repeated invocations
- **GIVEN** a content unit that was previously classified as `unresolved` on
  first observation
- **AND** no intervening change has occurred
- **WHEN** the change set Δ is computed again
- **THEN** the content unit SHALL receive the same classification (`unresolved`)
  as on the first invocation
- **AND** its confidence SHALL remain zero

