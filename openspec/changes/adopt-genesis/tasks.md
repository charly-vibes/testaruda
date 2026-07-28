## 1. Dependency
- [ ] 1.1 Add `genesis = { git = "https://github.com/charly-vibes/genesis", tag = "v0.1.0" }` to `Cargo.toml`.
- [ ] 1.2 Verify build with envelope/suggestions/managed_block/suite_linter modules stable.

## 2. Adopt suggestions
- [ ] 2.1 Register testaruda's command list with `genesis::suggestions::SuggestionEngine`.
- [ ] 2.2 Wire `main.rs` error sink to emit `genesis::suggestions` fix-footers.
- [ ] 2.3 Regression: `testaruda slect` (typo) prints "Did you mean 'select'?".

## 3. Adopt managed_block
- [ ] 3.1 Source injector mechanics from `genesis::managed_block` (closes wai-bdqw.7 for testaruda).
- [ ] 3.2 Regression: `testaruda init` injects/refreshes managed blocks.

## 4. Adopt shared envelope
- [ ] 4.1 Route `select`/`discover`/`validate` `--json` through `genesis::envelope`.
- [ ] 4.2 Test: top-level keys match the shared shape.

## 5. Add doctor (true-suite-minimum verb)
- [x] 5.1 Add `Doctor` variant to the `Commands` enum.
- [x] 5.2 Register testaruda's own checks (testaruda.toml schema, adapter config) with `genesis::suite_linter::LinterRegistry`.
- [x] 5.3 `doctor` calls `LinterRegistry::run_all()` to run all registered checks; genesis owns the orchestration, testaruda owns the check logic.
- [x] 5.4 `--fix` applies safe fixes; each check provides its own fix fn.

## 6. Ship llm.txt
- [x] 6.1 Hand-write a minimal `llm.txt` now (closes Appendix A.3 gap).
- [ ] 6.2 Switch to `genesis::aix` generation once that module is stable.

## 7. Clean up
- [x] 7.1 `cargo clippy -- -D warnings` clean.
- [ ] 7.2 Verify tool-craft (genesis `.wai` research) Appendix A.3 testaruda row; file a charly-monorepo ticket if inaccurate.

## 8. Add `feedback` subcommand (wraps `genesis::feedback`)
- [x] 8.1 Add `Feedback` variant to the `Commands` enum with `KIND` + flags (playbook §2).
- [x] 8.2 Read testaruda's error scratch for `--from-last-error`; never shadow the real error.
- [x] 8.3 Default target repo = testaruda's `Cargo.toml` `repository`; labels from playbook §8.
- [ ] 8.4 Error-footer hook: non-zero exits with no `genesis::suggestions::Fix` print `Feedback: testaruda feedback bug --from-last-error`.
- [x] 8.5 Regression: `testaruda feedback bug --dry-run` prints body + exact `gh` line; redactor strips a `https://<pat>@…` remote.
