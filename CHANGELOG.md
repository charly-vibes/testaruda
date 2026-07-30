# Changelog

## 0.3.0 — genesis v0.4.0 adoption, .NET adapter support (2026-07-30)

### Added

- genesis v0.4.0 modules: CliVerbosity (global `-v`/`-vv`/`-vvv` + `-q`), CliFormat
  (global `--json`/`--human` with TTY auto-detect), discovery module (cross-tool
  registration in `.genesis/tools.toml`), scaffold, status, feedback.
- `.NET adapter (titi)`: opt-in extension mappings for `.cs`/`.fs`/`.vb`/`.csproj`/`.sln`/`.slnx`
  via `titi testaruda-adapter` external binary (TIA-ADAPT-024).
- `testaruda fingerprint` subcommand — refresh all content unit fingerprints from disk.
- `testaruda status` subcommand — cross-tool health summary via genesis.
- Stress-test: `--mode synthetic` for meaningful adapter quality measurement.
- Rust adapter static dependency edge analysis from test file imports.

### Fixed

- Rust adapter: use `.get()` instead of `[]` indexing for Cargo.toml parsing.
- TypeScript adapter: path canonicalization for static-deps edge discovery.
- Clojure adapter: flat static-deps response format matching core protocol.
- Test alignment: Julia, TypeScript, and .NET adapter integration tests updated
  to match actual protocol wire formats (edges as `Vec<DepEdge>`, discover as
  direct array, fingerprint params format).

### Changed

- `testaruda select --json` replaced by global `--json`/`--human` flags (available
  on all commands, with TTY auto-detect: JSON for agents/pipes, Human for terminals).
- genesis-vibes dependency bumped from 0.3 to 0.4.

---

## 0.2.6 — Julia Base.Test support, titi adapter compliance (2026-07-28)

### Added

- Julia adapter now discovers `@testset` blocks (Base.Test) alongside existing
  `@testitem` (ReTestItems.jl) support. Includes file-level fallback for files
  with no test blocks (closes testaruda-thz).
- titi (.NET) adapter protocol compliance: handshake now includes `version`,
  `protocol`, and nested `capabilities`; all responses include `ok: true/false`;
  error format follows the standard `{ok: false, error: {message: ...}}` envelope.
- Custom `@testset` type discovery (e.g. `@testset MyCustomType ...`).
- Helper-file exclusion in file-level fallback (skips `helpers.jl`, `utils.jl`).

### Fixed

- O(n²) dedup in `discover_testsets` replaced with O(1) `Set{Tuple{String,Int}}`.

---

## 0.2.5 — Spec-contract coverage sweep (2026-07-28)

### Added

- Clojure adapter: all 6 commands (handshake, discover, static-deps,
  fingerprint, run-args, ingest) with fixture project, integration tests,
  and documentation (closes 19 tickets).
- .NET adapter detection shell-split infrastructure: `parse_command_string()`
  + `spawn_adapter()` helper in `src/adapter.rs` (TIA-ADAPT-024).
- 26 new espectacular contracts covering change-detection, adapter-protocol,
  agent-mode, local-mode, observability, selection-engine, and non-functional
  domains — `ah check` now reports 0 issues.
- JSON Schema for agent and pre-edit output formats
  (`docs/schemas/agent-output.schema.json`,
  `docs/schemas/pre-edit-output-v1.json`).
- TypeScript and Clojure adapter documentation in `docs/configuration.md`.

### Fixed

- `run_adapter_pipeline` error diagnostics name resolved binary (e.g. `titi`)
  not full command string.
- Adapter-clojure protocol standardized to use params-style invocation.
- Stress-test harness: adapter resolved to absolute path, `--test-dir` flag,
  empty node_ids handled.
- Multi-language benchmark script.

---

## 0.2.2 — Pre-push integration (2026-07-17)

### Added

- `testaruda select --safe`: pre-flight checks (testaruda.toml, store,
  git refs) with graceful fallback to `cargo test`. Implies --ci.
  Intended for pre-push hooks in Rust projects.
- `.flatpak-builder/` exclusion in Rust adapter discover to avoid
  bloated discovery in Flatpak build environments.

### Fixed

- `schema_version()` query table name mismatch (`_schema_version` vs
  `schema_version`).
- CI mode now propagates test runner exit code instead of always exiting 0.
- `--safe` mode captures exit code before ingesting results, preserving
  the feedback loop for failed runs.

---

## 0.2.1 — UX Round 5 implementation (2026-07-15)

### Added
- `testaruda completions bash|zsh|fish|powershell` subcommand (clap_complete) (UX8)
- `gen-cli-docs` hidden subcommand + `just doc-cli`/`just doc-cli-check` for CLI doc freshness (TIA-PORT-004)
- JSON Schema at `schemas/agent-output-v1.json` + `docs/agent-mode.md` for LLM agent consumers (TIA-AGENT-008)
- `Store::check_initialized()` — graceful error on store ops before `init` (TIA-LOCAL-006)
- Pre-edit output now emits structured JSON (`testaruda-pre-edit-v1`) instead of emoji prose (TIA-AGENT-005)
- `TestOrdering::Display` impl for help text and `ValueEnum` derive for validation (TIA-SEL-008)
- NO_COLOR, CLICOLOR, and non-TTY ANSI suppression (UX9)

### Fixed
- Tracing output routed to stderr to prevent breaking `--json`/`--agent` parsing (TIA-OBS-005)
- `--ordering` now validates against enumerated values — no silent default fallback for typos (TIA-SEL-008)
- `explain <unknown-id>` returns clear error instead of `{dependencies:[]}` (UX10)
- Parallel test interference in adapter-python CWD-manipulating tests (spurious `No such file or directory`)
- Duplicate quick-start docs collapsed into canonical Tutorial (UX7)

### Changed
- `--ordering` field type from `String` to `TestOrdering` (clap ValueEnum)
- `docs/cli.md` regenerated from clap definitions, covers all 12 subcommands
- `README.md` Quick Start trimmed; `docs/getting-started.md` links to reference docs