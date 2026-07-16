## ADDED Requirements (replacing ADAPT-014..016 from the superseded proposal)

### Requirement: TIA-ADAPT-014 — Julia adapter handshake (coverage-based)

A Julia adapter SHALL declare `"julia"` as one of its supported languages in
the handshake. The adapter SHALL declare `symbol_model_complete: false` and
`runtime_edges: true` in its handshake capabilities. The adapter SHALL declare
`granularity: "file"`.

The adapter SHALL be hosted inside the Testimonial.jl package (not as a
separate repository), accessed via a Julia `-e` expression. The `testaruda.toml`
adapter config SHALL use the recommended invocation:
```toml
[adapters]".jl" = "julia --project=. -e 'using Testimonial; Testimonial.Protocol.run_adapter_protocol()'"
```
This works for both system-installed (via `Pkg.add`) and local-dev setups.
The `--project=.` resolves to the current project's environment, which
must have `Testimonial` as a dependency.

For local development from a cloned Testimonial.jl repo, an alternative is:
```toml
[adapters]".jl" = "julia --project=/path/to/Testimonial.jl bin/testaruda_adapter.jl"
```

#### Scenario: Julia handshake with runtime_edges
- **GIVEN** a Julia adapter entry point is spawned
- **WHEN** the handshake command is invoked
- **THEN** the adapter SHALL include `"julia"` in its `languages` array
- **AND** SHALL declare `symbol_model_complete: false`
- **AND** SHALL declare `runtime_edges: true`
- **AND** SHALL declare `granularity: "file"`

#### Scenario: Missing Julia runtime
- **GIVEN** a system without Julia installed
- **WHEN** the core attempts to spawn the adapter entry point
- **THEN** the core SHALL report "adapter entry point not found"
- **AND** SHALL fall back to selecting all tests (per TIA-ADAPT-012)

#### Scenario: Missing Testimonial.jl package
- **GIVEN** a system with Julia installed but without the `Testimonial` package
- **WHEN** the core spawns the adapter shim (`julia --project=. bin/testaruda_adapter.jl`)
- **THEN** the adapter SHALL fail with a `LoadError`
- **AND** the error message SHALL suggest installing Testimonial.jl
  (e.g. `Pkg.add("Testimonial")`)
- **AND** the core SHALL fall back to selecting all tests (per TIA-ADAPT-012)

### Requirement: TIA-ADAPT-015 — Julia discover scope (@testitem-based)

A Julia adapter SHALL discover test items by walking the project's test
directories and parsing `.jl` files for `@testitem` blocks (from
`ReTestItems.jl`). Each `@testitem` SHALL be one test item. Node IDs SHALL be
derived from source location (`test_file:line`) rather than the test-item
name, because dynamically-generated names (e.g.
`@testitem name="test_$i"`) would produce non-deterministic IDs across
invocations.

Projects that use `Test.jl` without `ReTestItems.jl` MAY use a separate
fallback code path at the adapter's discretion, but the primary discovery
target is `@testitem`. The adapter SHALL only discover `@testitem` blocks;
`@testset`-only files in a mixed project SHALL NOT be discovered, and this
limitation SHALL be documented.

#### Scenario: Discover Julia tests from @testitem blocks
- **GIVEN** a Julia project with `ReTestItems.jl` tests containing `@testitem`
  blocks
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return test items
- **AND** each item SHALL be derived from one `@testitem` block
- **AND** node IDs SHALL be derived from source location (`test_file:line`)
- **AND** suite kind SHALL be `"ReTestItems.jl"`

#### Scenario: No test directory
- **GIVEN** a Julia project with no test directories
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return an empty items list

#### Scenario: Mixed @testitem and @testset project
- **GIVEN** a Julia project containing both `@testitem` blocks and `@testset`
  blocks in different files
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL discover only the `@testitem`-based test items
- **AND** SHALL NOT discover `@testset`-based tests
- **AND** the adapter's documentation SHALL note this limitation

#### Scenario: Dynamically generated @testitem names
- **GIVEN** a Julia project with `@testitem` blocks using dynamically
  generated names (e.g. `@testitem name="test_$i"`)
- **WHEN** the `discover` command is invoked
- **THEN** each `@testitem` SHALL be assigned a stable node ID derived from
  its source location (`test_file:line`) rather than its name
- **AND** subsequent invocations with the same source SHALL produce the same
  node IDs

### Requirement: TIA-ADAPT-016 — Julia dependency analysis (coverage-based runtime edges)

A Julia adapter SHALL build dependency edges empirically, not statically:
- On first invocation (no coverage recorded): report every changed file as
  `unresolved`, which triggers testaruda's existing fallback (`TIA-SAFE-004`).
- After at least one recording pass: return edges derived from per-item
  subprocess coverage recording (`--code-coverage=user`, `.jl.cov` parsed via
  `Coverage.jl`).

The adapter SHALL NOT perform static `include()`-parsing or `import`/`using`
resolution for dependency edges. When the adapter returns dependency edges,
they SHALL be runtime edges sourced from coverage data. The adapter MAY report
`unresolved` for files with no coverage data, which triggers testaruda's
existing fallback (`TIA-SAFE-004`). The adapter SHALL emit edges at file-level
granularity.

#### Scenario: First invocation — unresolved fallback
- **GIVEN** a set of changed Julia source files with no prior coverage recording
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL report all changed files as `unresolved`
- **AND** SHALL NOT return any specific dependency edges

#### Scenario: Subsequent invocation — coverage-based edges
- **GIVEN** a set of changed Julia source files with a prior coverage recording
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return candidate tests
- **AND** file-level edges derived from coverage data (which lines each test
  covered during recording)
- **AND** the edge origin SHALL be runtime

#### Scenario: Missing test coverage for a changed file
- **GIVEN** a changed Julia source file with no coverage recorded in any test
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL include the source file in the `unresolved` list
- **AND** the core SHALL apply the fallback mechanism (`TIA-SAFE-004`)
