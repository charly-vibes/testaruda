# Changelog

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