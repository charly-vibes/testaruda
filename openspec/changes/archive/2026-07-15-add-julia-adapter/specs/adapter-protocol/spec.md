## ADDED Requirements

### Requirement: TIA-ADAPT-014 — Julia adapter handshake language

A Julia adapter SHALL declare `"julia"` as one of its supported languages in the handshake.

#### Scenario: Julia handshake language
- **GIVEN** a Julia adapter binary
- **WHEN** the handshake command is invoked
- **THEN** the adapter SHALL include `"julia"` in its `languages` array
- **AND** SHALL declare `symbol_model_complete: false` and `runtime_edges: false` in its capabilities

#### Scenario: Missing Julia runtime
- **GIVEN** a system without Julia installed
- **WHEN** the core attempts to spawn the adapter binary
- **THEN** the core SHALL report "adapter binary not found"
- **AND** SHALL fall back to selecting all tests

### Requirement: TIA-ADAPT-015 — Julia discover scope

A Julia adapter SHALL discover test items by walking the project's `test/` directory, parsing `.jl` files for top-level `@testset` blocks. Each top-level `@testset` SHALL be one test item; leaf `@test` assertions SHALL NOT be individual items.

#### Scenario: Discover Julia tests from @testset blocks
- **GIVEN** a Julia project with `test/runtests.jl` containing top-level `@testset` blocks
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return test items
- **AND** each item SHALL be derived from one `@testset` block
- **AND** leaf `@test` assertions inside a `@testset` SHALL NOT produce separate items
- **AND** node IDs SHALL be derived from source location (`file:line`) rather than the testset name
- **AND** suite kind SHALL be `"Test.jl"`

#### Scenario: No test directory
- **GIVEN** a Julia project with no `test/` directory
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return an empty items list

### Requirement: TIA-ADAPT-016 — Julia static dependency analysis (file-level)

A Julia adapter SHALL analyze file-level dependencies by parsing `include("...")` calls. `Base.include_dependency` SHALL NOT be parsed. Module-level `import`/`using` resolution is deferred to a future change — the adapter SHALL NOT resolve these in the initial implementation.

#### Scenario: Julia static deps via include
- **GIVEN** a set of changed Julia source files that use `include("...")`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return candidate tests
- **AND** file-level edges derived from `include` calls
- **AND** files whose dependencies cannot be resolved SHALL be reported as `unresolved`
- **AND** SHALL NOT return per-symbol edges (since `symbol_model_complete` is `false`)

#### Scenario: Unresolvable include path
- **GIVEN** a Julia source file with an `include` path that cannot be resolved
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL include the source file in the `unresolved` list
- **AND** the core SHALL apply the fallback mechanism (TIA-SAFE-004)