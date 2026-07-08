# Persistence

## Purpose

Database storage, blob store, schema management, and concurrent access — the store layer.

## Requirements

### Requirement: TIA-STORE-001 — Embedded transactional database

The store SHALL persist the queryable index in an embedded transactional database.

#### Scenario: Embedded database
- **GIVEN** the store
- **WHEN** persisting the queryable index
- **THEN** it SHALL use an embedded transactional database (e.g. SQLite)

### Requirement: TIA-STORE-002 — Content-addressed blob store

The store SHALL persist large per-run payloads in a content-addressed blob store, deduplicated by content hash.

#### Scenario: Deduplicated blob storage
- **GIVEN** a per-run payload
- **WHEN** it is persisted
- **THEN** it SHALL be stored in a content-addressed blob store
- **AND** duplicates SHALL be deduplicated by content hash

### Requirement: TIA-STORE-003 — Export and import

The store SHALL support export and import of the dependency graph and provenance in a documented interchange format.

#### Scenario: Graph export
- **GIVEN** a populated store
- **WHEN** export is requested
- **THEN** the dependency graph and provenance SHALL be exported in a documented interchange format

### Requirement: TIA-STORE-004 — Schema migration

When the store schema version differs from the running core, the core SHALL migrate or refuse with a clear diagnostic rather than corrupt data.

#### Scenario: Schema version mismatch
- **GIVEN** a store with an older schema version than the running core
- **WHEN** the core attempts to use it
- **THEN** the core SHALL either migrate the schema or refuse with a clear diagnostic
- **AND** SHALL NOT corrupt data

### Requirement: TIA-STORE-005 — Concurrent reads during write

The store SHALL support concurrent reads during an in-progress write.

#### Scenario: Read during write
- **GIVEN** an in-progress write to the store
- **WHEN** a read request arrives
- **THEN** the store SHALL allow the read to proceed concurrently