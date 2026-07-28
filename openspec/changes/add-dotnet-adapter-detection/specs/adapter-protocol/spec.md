## ADDED Requirements

### Requirement: TIA-ADAPT-023 — .NET adapter via external `titi` binary (command-string invocation)

A .NET (C#) adapter SHALL be provided as an **external binary** invoked as
`titi testaruda-adapter` — i.e. the `testaruda.toml` extension-mapping value is
the command string `"titi testaruda-adapter"`, which the core SHALL shell-split
into `binary="titi"`, `args=["testaruda-adapter"]` before calling
`AdapterIO::spawn` (see TIA-ADAPT-024 for the shell-split contract). This mirrors
the Julia adapter's command-string form (`"julia --project=. -e '...'"`,
TIA-ADAPT-014); the .NET adapter is the second external subcommand-style adapter,
not the first.

The adapter SHALL NOT be a workspace crate; it MUST be installed separately
(titi's own installation instructions apply — see
`github.com/sashakile/titi`), exactly as the Julia adapter requires
Testimonial.jl to be installed separately. The adapter is **opt-in via manual
`testaruda.toml` configuration** — it SHALL NOT be auto-detected by `testaruda
init` (see `change-detection` spec, TIA-CHG-009), following the Julia adapter's
precedent that external/hosted adapters are not auto-detected because they are
separate installs (see `add-julia-adapter-via-testimonial` proposal, Impact).

The adapter's handshake SHALL declare (per the titi adapter spec, titi change
`add-testaruda-adapter` CLI-19):
- `name = "titi"`
- `languages = ["csharp"]`
- `granularity = "project"` (Phase 1 — each test item is a whole test assembly)
- `symbol_model_complete = false`
- `runtime_edges = false` (Phase 1 — titi uses its MSBuild `MonorepoGraph` for
  static edges; runtime-edge construction is deferred to Phase 2, per titi
  `add-test-item-detection` DM-08. The value is pinned in the titi adapter spec
  CLI-19 and referenced here.)
- `protocol` = the current `PROTOCOL_VERSION`

The `testaruda.toml` extension mappings for .NET SHALL map the .NET **source**
extensions (the units of change that titi's `AffectedSet` consumes — see titi
`dependency-graph` spec DG-04) plus the project/solution files:

```toml
[adapters.extensions]
".cs"    = "titi testaruda-adapter"
".fs"    = "titi testaruda-adapter"
".vb"    = "titi testaruda-adapter"
".csproj" = "titi testaruda-adapter"
".sln"    = "titi testaruda-adapter"
".slnx"   = "titi testaruda-adapter"
```

Routing `.cs`/`.fs`/`.vb` is mandatory: testaruda routes changed files to
adapters strictly by extension (`registry.resolve` in `src/config.rs`), and titi
receives the changed-file set from testaruda (titi CLI-19 "Static-deps uses
titi's AffectedSet"). Without the source-extension mappings, typical .NET source
changes would never reach titi and the adapter would compute an empty affected
set.

> **Polyglot routing:** In a polyglot repo, files routed to the titi adapter that
> are not part of titi's `MonorepoGraph` (e.g. a stray `.cs` file outside any
> `.csproj`) SHALL be returned by titi as `unresolved` in its `static-deps`
> response, and testaruda SHALL apply its existing over-approximation fallback
> (TIA-SAFE-004). Polyglot repos SHOULD map only .NET source extensions
> (`.cs`/`.fs`/`.vb`) plus `.csproj`/`.sln`/`.slnx` to titi, leaving other
> languages' extensions mapped to their own adapters.

#### Scenario: .NET adapter handshake
- **GIVEN** the `titi` binary is on `PATH` and the titi adapter subcommand is implemented
- **WHEN** the core spawns `titi testaruda-adapter` and sends the `handshake` command
- **THEN** the adapter SHALL return `name = "titi"`, `languages = ["csharp"]`, `granularity = "project"`, `symbol_model_complete = false`, and `runtime_edges = false` (per titi CLI-19)
- **AND** the adapter SHALL be a long-lived process that reuses the same `MonorepoGraph` for all subsequent commands (discover/static-deps/fingerprint/run-args/ingest)

#### Scenario: .NET source file routed to titi
- **GIVEN** a `testaruda.toml` mapping `.cs`, `.fs`, `.vb`, `.csproj`, `.sln`, `.slnx` to `titi testaruda-adapter`
- **WHEN** a `.cs` source file is in the change set and `testaruda select` runs
- **THEN** the core SHALL route the `.cs` file to the titi adapter (not to `unresolved`), shell-split the command string per TIA-ADAPT-024, and spawn `titi` with arg `testaruda-adapter`
- **AND** titi's `static-deps` SHALL receive the `.cs` file in its changed-file set and return affected tests via its `AffectedSet` (titi DG-04)

#### Scenario: titi binary not found at selection time
- **GIVEN** `testaruda.toml` maps `.cs` to `titi testaruda-adapter` but the `titi` binary is not on `PATH`
- **WHEN** the core attempts to spawn the adapter during `select`
- **THEN** the core SHALL fall back to selecting all tests in the affected component per TIA-ADAPT-012
- **AND** the failure diagnostic SHALL name the resolved binary (`titi`) rather than the full command string, so the user can locate the missing executable
- **AND** SHALL record the adapter failure (binary not found) for observability

#### Scenario: Polyglot file misrouted to titi is handled by over-approximation
- **GIVEN** a polyglot repo mapping `.cs`→titi and `.ts`→typescript, and a `.cs` file changed in a subdirectory that is not part of any `.csproj` in titi's `MonorepoGraph`
- **WHEN** `testaruda select` runs
- **THEN** the file SHALL be routed to the titi adapter (per extension mapping)
- **AND** titi SHALL return it as `unresolved` in its `static-deps` response
- **AND** testaruda SHALL apply its over-approximation fallback (TIA-SAFE-004) — selecting all tests in the affected component — rather than silently dropping the file

### Requirement: TIA-ADAPT-024 — Command-string adapter invocation (shell-split)

Adapter extension-mapping values in `testaruda.toml` SHALL be command strings
that the core shell-splits into `(binary, args)` before calling
`AdapterIO::spawn`. This requirement generalizes the invocation mechanism beyond
bare-binary adapters (`testaruda-adapter-rust`, `-python`, `-typescript`,
`-clojure`) to support external/subcommand-style adapters such as the .NET
adapter (`titi testaruda-adapter`, TIA-ADAPT-023) and the Julia adapter
(`julia --project=. -e '...'`, TIA-ADAPT-014). It unblocks both external
adapters; no prior change specified the shell-split.

The core SHALL apply the shell-split at **every** `AdapterIO::spawn` call site
that consumes a configured adapter string (currently `src/main.rs` lines 448,
579, 803, 858, 904). To avoid partial coverage, the core SHALL route all such
spawn sites through a single helper (e.g. `spawn_adapter(command_string,
timeout)`) so the split is applied by construction.

The core SHALL use the `shell-words` crate (POSIX-ish quoting; sufficient for
both known command-string adapters; no surprising shell expansions in a config
file). The split SHALL handle: single-token strings (bare binaries),
multi-token strings (subcommand + args), and quoted paths/args containing
spaces.

#### Scenario: Bare-binary adapter (no regression)
- **GIVEN** a `testaruda.toml` with `".rs" = "testaruda-adapter-rust"` (a single-token command string)
- **WHEN** the core spawns the adapter
- **THEN** the shell-split SHALL produce `binary = "testaruda-adapter-rust"` and `args = []`
- **AND** the core SHALL invoke `AdapterIO::spawn("testaruda-adapter-rust", &[], timeout)` (unchanged behavior for in-workspace adapters)

#### Scenario: Subcommand adapter (.NET)
- **GIVEN** a `testaruda.toml` with `".cs" = "titi testaruda-adapter"`
- **WHEN** the core spawns the adapter for a .NET project
- **THEN** the shell-split SHALL produce `binary = "titi"` and `args = ["testaruda-adapter"]`
- **AND** SHALL invoke `AdapterIO::spawn("titi", &["testaruda-adapter"], timeout)`

#### Scenario: Quoted-arg adapter (Julia)
- **GIVEN** a `testaruda.toml` with `".jl" = "julia --project=. -e 'using Testimonial; Testimonial.Protocol.run_adapter_protocol()'"`
- **WHEN** the core spawns the adapter for a `.jl` file
- **THEN** the shell-split SHALL produce `binary = "julia"` and `args = ["--project=.", "-e", "using Testimonial; Testimonial.Protocol.run_adapter_protocol()"]` (the `-e` body preserved as a single arg)
- **AND** the Julia adapter SHALL function correctly (this is the shared core infrastructure that unblocks TIA-ADAPT-014)

#### Scenario: Binary path with spaces (Windows install path)
- **GIVEN** a `testaruda.toml` with `".cs" = "\"C:\\Program Files\\titi\\titi.exe\" testaruda-adapter"`
- **WHEN** the core spawns the adapter
- **THEN** the shell-split SHALL produce `binary = "C:\Program Files\titi\titi.exe"` and `args = ["testaruda-adapter"]`
- **AND** SHALL invoke `AdapterIO::spawn` with that binary path (quoted paths with spaces are supported, including on Windows)

#### Scenario: Empty command string is a config error
- **GIVEN** a `testaruda.toml` with `".foo" = ""` (empty command string)
- **WHEN** the core attempts to resolve the adapter for a `.foo` file
- **THEN** the core SHALL emit a config-error diagnostic naming the extension (`.foo`) and the empty mapping
- **AND** SHALL NOT attempt to spawn `AdapterIO::spawn` with an empty binary
- **AND** SHALL fall back per TIA-ADAPT-012 (treat as adapter failure → full-suite selection for the affected component)
