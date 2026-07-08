# Adapter Protocol

## Purpose

Language/framework adapter interface — JSON request/response protocol over stdin/stdout with handshake, discovery, dependency analysis, and ingestion commands.

## Requirements

### Requirement: TIA-ADAPT-001 — JSON protocol

The core SHALL communicate with adapters using JSON request/response over standard input and output, with diagnostics on standard error and status via exit code.

#### Scenario: Adapter communication
- **GIVEN** the core and an adapter process
- **WHEN** the core sends a command
- **THEN** the adapter SHALL receive JSON on stdin
- **AND** SHALL respond with JSON on stdout
- **AND** SHALL emit diagnostics on stderr
- **AND** SHALL indicate status via exit code

### Requirement: TIA-ADAPT-002 — Adapter handshake

When the core starts an adapter, the adapter SHALL return a handshake declaring its name, version, supported protocol version, languages, granularity, and capability flags. Capability flags SHALL include at minimum: `symbol_model_complete` (boolean — used by TIA-CHG-004 to permit sub-file granularity narrowing).

#### Scenario: Handshake response
- **GIVEN** an adapter is started
- **WHEN** the core requests its capabilities
- **THEN** the adapter SHALL return name, version, protocol version, languages, granularity, and capability flags
- **AND** `symbol_model_complete` SHALL be included among the capability flags

### Requirement: TIA-ADAPT-003 — Required commands

An adapter SHALL implement the commands `discover`, `static-deps`, `fingerprint`, `run-args`, and `ingest`.

#### Scenario: Required command set
- **GIVEN** an adapter implementation
- **WHEN** the core sends any of the required commands
- **THEN** the adapter SHALL respond appropriately for `discover`, `static-deps`, `fingerprint`, `run-args`, and `ingest`

### Requirement: TIA-ADAPT-004 — Discover command

When invoked with `discover`, an adapter SHALL enumerate test items in scope with their node id, suite kind, and file.

#### Scenario: Test discovery
- **GIVEN** an adapter with access to test files
- **WHEN** the `discover` command is invoked
- **THEN** it SHALL return test items with node id, suite kind, and file path

### Requirement: TIA-ADAPT-005 — Static-deps command

When invoked with `static-deps` and a changed-file set, an adapter SHALL return candidate test items, K-valued edges, and a list of files it could not resolve. When `symbol_model_complete` is `true`, the adapter SHALL also return per-symbol edges; otherwise the core treats all edges as file-level (TIA-CHG-004).

#### Scenario: Static dependency analysis
- **GIVEN** a changed-file set
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return candidate tests, K-valued edges, and unresolved files
- **AND** if `symbol_model_complete` is true, SHALL also return per-symbol edges

### Requirement: TIA-ADAPT-006 — Fingerprint command

When invoked with `fingerprint`, an adapter SHALL return content fingerprints at its declared granularity.

#### Scenario: Content fingerprinting
- **GIVEN** files or symbols to fingerprint
- **WHEN** the `fingerprint` command is invoked
- **THEN** the adapter SHALL return content fingerprints at its declared granularity

### Requirement: TIA-ADAPT-007 — Run-args command

When invoked with `run-args` and a selected set, an adapter SHALL return the native runner arguments and a collection path, and SHALL NOT execute the tests.

#### Scenario: Run arguments generation
- **GIVEN** a selected set of tests
- **WHEN** the `run-args` command is invoked
- **THEN** the adapter SHALL return native runner arguments and a collection path
- **AND** SHALL NOT execute the tests

### Requirement: TIA-ADAPT-008 — Ingest command

When invoked with `ingest` and a run's output, an adapter SHALL return runtime edges, per-test results, and observed external inputs.

#### Scenario: Run output ingestion
- **GIVEN** a test run's output
- **WHEN** the `ingest` command is invoked
- **THEN** the adapter SHALL return runtime edges, per-test results, and observed external inputs

### Requirement: TIA-ADAPT-009 — Semiring edge values

An adapter SHALL emit dependency edges as semiring values, defaulting to the multiplicative identity where it has no finer weight.

#### Scenario: Default semiring weight
- **GIVEN** a dependency edge with no specific weight
- **WHEN** the adapter emits it
- **THEN** it SHALL use the multiplicative identity as the default weight

### Requirement: TIA-ADAPT-010 — Graceful degradation

If an adapter does not declare a capability, then the core SHALL degrade gracefully for that capability rather than fail, applying conservative defaults.

#### Scenario: Missing capability
- **GIVEN** an adapter that does not declare a capability
- **WHEN** the core needs that capability
- **THEN** the core SHALL NOT fail
- **AND** SHALL apply conservative defaults instead

### Requirement: TIA-ADAPT-011 — Protocol incompatibility

If an adapter's protocol version is incompatible with the core, then the core SHALL refuse to use it and report the mismatch.

#### Scenario: Version mismatch
- **GIVEN** an adapter with an incompatible protocol version
- **WHEN** the core attempts to use it
- **THEN** the core SHALL refuse to use it
- **AND** SHALL report the version mismatch

### Requirement: TIA-ADAPT-012 — Adapter failure fallback

If an adapter fails, times out, or returns malformed output, then the core SHALL fall back to selecting all tests in the affected component and record the failure.

#### Scenario: Adapter timeout
- **GIVEN** an adapter that times out
- **WHEN** the core is waiting for a response
- **THEN** the core SHALL fall back to selecting all tests in the affected component
- **AND** SHALL record the adapter failure

### Requirement: TIA-ADAPT-013 — Least privilege and timeout

The core SHALL invoke adapters with least privilege and a configurable timeout.

#### Scenario: Adapter sandboxing
- **GIVEN** an adapter process
- **WHEN** it is invoked
- **THEN** it SHALL be run with least privilege
- **AND** a configurable timeout SHALL be enforced