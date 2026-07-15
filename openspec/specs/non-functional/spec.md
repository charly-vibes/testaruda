# Non-Functional Requirements

## Purpose

Cross-cutting quality attributes that constrain the implementation but do not specify functional behaviour: performance, reliability, security, portability, and scalability.

## Requirements

### Requirement: TIA-PERF-001 — Change-set scaling

The core SHALL scale incremental selection time with the size of the change set rather than the size of the full test suite.

#### Scenario: Incremental time scaling
- **GIVEN** a full test suite of size N
- **WHEN** a change of size c is computed
- **THEN** selection time SHALL scale with c, not with N

### Requirement: TIA-PERF-002 — Interactive latency

While a warm local daemon is available, the CLI SHALL return selection for a single-file change within an interactive latency budget.

#### Scenario: Interactive selection
- **GIVEN** a warm local daemon
- **WHEN** a single-file change is evaluated
- **THEN** selection SHALL complete within an interactive latency budget (e.g. < 100ms)

### Requirement: TIA-PERF-003 — Overhead bound

The core SHALL bound its own selection overhead to a small fraction of the time saved by running fewer tests.

#### Scenario: Overhead vs savings
- **GIVEN** a selection that reduces the test run by T seconds
- **WHEN** selection overhead is measured
- **THEN** the overhead SHALL be a small fraction of T

### Requirement: TIA-REL-001 — Deterministic mode

While in deterministic mode, the core SHALL produce identical selections for identical inputs and store state.

#### Scenario: Deterministic behavior
- **GIVEN** identical inputs and store state in deterministic mode
- **WHEN** selection is computed twice
- **THEN** both selections SHALL be identical

### Requirement: TIA-REL-002 — Crash-safe ingestion

The core SHALL make ingestion crash-safe, leaving the store consistent after an interrupted operation. Each ingestion SHALL execute within a single database transaction (or equivalent write-ahead-log mechanism) so that a crash during ingestion leaves the store in its pre-ingestion state. Idempotency of re-ingestion is provided by the run-identity key (TIA-RUN-005).

#### Scenario: Crash during ingestion
- **GIVEN** an ingestion operation in progress
- **WHEN** a crash occurs midway
- **THEN** the store SHALL remain in its pre-ingestion state
- **AND** the run-identity key SHALL enable safe re-ingestion

### Requirement: TIA-SEC-001 — No code execution

The core SHALL NOT execute arbitrary repository code as part of selection.

#### Scenario: Safe selection
- **GIVEN** a repository with arbitrary code
- **WHEN** selection is computed
- **THEN** the core SHALL NOT execute any repository code

### Requirement: TIA-SEC-002 — Least privilege adapters

The core SHALL run adapters under least privilege with bounded resource limits.

#### Scenario: Sandboxed adapter
- **GIVEN** an adapter process
- **WHEN** it is spawned by the core
- **THEN** it SHALL run with least privilege
- **AND** SHALL have bounded resource limits

### Requirement: TIA-SEC-003 — Hashed environment representation

The core SHALL include only an allowlisted, hashed representation of environment variables in cache and environment keys, and SHALL NOT store raw secret values.

#### Scenario: Secret-free environment keys
- **GIVEN** environment variables containing secrets
- **WHEN** they are incorporated into cache or environment keys
- **THEN** only an allowlisted, hashed representation SHALL be used
- **AND** raw secret values SHALL NOT be stored

### Requirement: TIA-PORT-001 — Language-agnostic via adapters

The core SHALL be usable across languages and test frameworks solely through adapters, with no core changes required to add a language.

#### Scenario: New language via adapter
- **GIVEN** a new language to support
- **WHEN** an adapter is written for it
- **THEN** no core changes SHALL be needed

### Requirement: TIA-PORT-002 — Independent adapter versioning

The adapter protocol SHALL permit adapters to be implemented in any language and versioned independently of the core.

#### Scenario: Adapter in any language
- **GIVEN** a new adapter
- **WHEN** it is implemented
- **THEN** it MAY be written in any language
- **AND** versioned independently of the core

### Requirement: TIA-PORT-003 — Protocol compatibility policy

The core SHALL declare a protocol-compatibility policy and reject adapters outside the supported version range.

#### Scenario: Version range enforcement
- **GIVEN** an adapter outside the supported protocol version range
- **WHEN** the core attempts to use it
- **THEN** the core SHALL reject it

### Requirement: TIA-PORT-004 — CLI reference documentation freshness

The repository SHALL keep `docs/cli.md` (or equivalent human-facing CLI reference) in sync with the real `--help` output of the installed binary. The documentation SHALL NOT be considered up-to-date if it:
- Omits a real subcommand or flag, or
- Documents a subcommand or flag that does not exist in the real `--help` output.

> **Note:** Common approaches to satisfy this requirement include generating
> `docs/cli.md` from clap's help text (e.g. `clap-markdown`) or adding a CI
> check (e.g. `just doc-cli-sync`) that compares documented subcommands and
> flags against `--help` output and fails on drift.

#### Scenario: Missing subcommand detected
- **GIVEN** a real subcommand added to the CLI
- **WHEN** `docs/cli.md` is not updated
- **THEN** the CI doc-freshness check SHALL fail
- **AND** the failure SHALL identify the undocumented subcommand

#### Scenario: Generated reference
- **GIVEN** the CLI binary
- **WHEN** `docs/cli.md` is generated from `--help` output
- **THEN** all real subcommands and flags SHALL be present
- **AND** no phantom subcommands or flags SHALL appear

### Requirement: TIA-SCALE-001 — Monorepo scaling

The core SHALL support monorepos containing many components without recomputing the full graph per change.

#### Scenario: Monorepo efficiency
- **GIVEN** a monorepo with many components
- **WHEN** a change is made in a subset of components
- **THEN** the core SHALL NOT recompute the full graph

### Requirement: TIA-SCALE-002 — Federated repositories

The core SHALL support federation across multiple repositories without a single shared write bottleneck.

#### Scenario: No write bottleneck
- **GIVEN** multiple federated repositories
- **WHEN** they are composed
- **THEN** no single shared write bottleneck SHALL exist