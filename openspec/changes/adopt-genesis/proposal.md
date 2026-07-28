# Change: Adopt genesis

## Why

testaruda is the largest-gap tool in the suite (tool-craft.md Appendix A.3):
it lacks the `Suggestion` self-healing enum, the managed-block injector, the
`doctor` command, and `llm.txt`. It also emits `--json` without the shared
envelope. This change makes testaruda a *consumer* of genesis to close those
gaps at once.

## What Changes

- Add `genesis` git dependency (pinned by tag `v0.1.0`) to `Cargo.toml`.
- Adopt `genesis::suggestions` for typo detection and fix-footers across
  `init`/`select`/`import`/`explain`/`validate`/`discover`.
- Source the managed-block injector from `genesis::managed_block` (closes the
  `wai-bdqw.7` gap for testaruda).
- Route `--json` output through `genesis::envelope` (`select`, `discover`,
  `validate`).
- Ship `llm.txt` (generated via `genesis::aix` once stable; hand-written
  minimally until then — closes the Appendix A.3 testaruda `llm.txt` gap).
- Add a `doctor` command (the true-suite-minimum verb testaruda is missing —
  tool-craft §2.1) backed by `genesis::suite_linter` for its checks.
- Add a `testaruda feedback [KIND]` subcommand wrapping `genesis::feedback`.
  testaruda owns the command surface; genesis owns the machinery. Blocked
  on `genesis::feedback` (wai donates first).
- Keep all domain logic (Ascent Datalog engine, provenance semiring, SQLite
  store, language adapters). The genesis boundary rule protects this.

## Impact

- Affected specs: `cli` surface (new `doctor`), `observability`
  (MODIFIED — envelope + suggestions), `non-functional` (llm.txt), plus a
  new `doctor` capability delta.
- Affected code: `Cargo.toml`, `src/main.rs` (new `Doctor` variant + error
  footer), `src/cli.rs`, new `llm.txt`, new `src/doctor.rs`.
- Blocked by: genesis tagging `v0.1.0` (envelope/suggestions/managed_block/
  suite_linter stable).
- Coordinates with `testaruda-86m` (testaruda.toml schema) for the
  `suite_linter` check.
