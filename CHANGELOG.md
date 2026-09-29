# Changelog

## 0.5.0 — exec subcommand, babashka support in the Clojure adapter (2026-09-29)

### Added

- `testaruda exec` subcommand — select→run→ingest→calibrate loop with an
  uncalibrated-store advisory that fires before select (gh-26 / testaruda-n5b4).

### Fixed

- Clojure adapter: babashka `.bb` files are now treated as Clojure-family
  sources — discover and static-deps honor repos that map `.bb` to the adapter
  (gh-34 / testaruda-rfxa).
- Clojure adapter: top-level `(require '[ns :as alias])` forms (babashka
  script style) now produce dependency edges — previously only `:require`
  keywords inside `(ns ...)` matched (testaruda-xcb6).
- Clojure adapter: ns-less script files map to a namespace via the Clojure
  path convention (`src/finanzas/my_ns.bb` → `finanzas.my-ns`) instead of
  landing in `unresolved` (testaruda-t79n).
- Engine: saturating distance arithmetic in Ascent rules — u32 overflow on
  long dependency chains could corrupt distance comparison (testaruda-vax).
- ISO timestamp formatting now uses the `time` crate instead of ~55 lines of
  hand-rolled calendar math (testaruda-jxd0).
- Python adapter: strip trailing comments from import lines
  (`import ctypes  # noqa` no longer yields a bogus module name)
  (testaruda-wpil).
- Feedback: multi-line stdin descriptions no longer truncated to the first
  line — pinned by regression test; fixed upstream in genesis-vibes 0.8.1
  (gh-27 / testaruda-p5zl).
- `validate-imports.py`: relative-import base resolution iterated dotted
  strings char-by-char, producing phantom mismatches (testaruda-rpqs).
- Pre-edit mode now exits with the outcome-derived CI code (testaruda-ljeg).
- Suggestion path only fires on invalid subcommands (testaruda-ixp0).
- Genesis suggestion suppressed when clap already prints its tip
  (testaruda-p1uv).

### Changed

- genesis-vibes 0.7 → 0.8.2 (envelope API, feedback `--title` flag).
- dont epistemic gate wired into pre-commit and CI (`just check-claims`)
  (testaruda-zvbw).
- Docs: getting-started example prefers the global `--json` flag (testaruda-qdw9).

---

## 0.4.0 — genesis v0.6.0 adoption, content unit dedup, CI exit codes (2026-08-05)

### Added

- genesis v0.6.0 envelope API adoption (testaruda-2wwn).

### Fixed

- Preserve CI exit status in JSON output mode (testaruda-fnyi).
- Prevent duplicate content units when symbol is NULL — partial unique
  indexes (testaruda-p37i).
- Accept human-readable node_id in `testaruda explain` (testaruda-8dm).
- Improve ingest error message for invalid outcome (testaruda-7dq).
- Handle git porcelain rename paths with `->` arrow (testaruda-jzp).
- Skip genesis suggestion logic on help/version display (testaruda-ll8).

### Changed

- Default tracing level info → warn (testaruda-f05).
- Command registry updated — validate→oracle, added status/fingerprint
  (testaruda-iwf).
- `--help` shows proper description via Cli doc comment (testaruda-kxr).

---
## 0.3.1 — Full genesis v0.4.0 adoption (2026-07-31)

### Added

- `genesis::cli::maybe_print_version_json` — structured `--version --json` output
  via genesis envelope (pre-parsed before clap).
- `genesis::fixture::Fixture` — adopted in test files (`ordering`, `seeded_fault`)
  replacing `tempfile::TempDir` + manual file writes.
- Documentation: genesis adoption table in `llms.txt`/`llm.txt` covering all 14 modules.

### Fixed

- Markdown table formatting in `llms.txt`/`llm.txt` genesis adoption tables.

### Changed

- genesis-vibes dependency confirmed at 0.4 (unchanged).

---

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