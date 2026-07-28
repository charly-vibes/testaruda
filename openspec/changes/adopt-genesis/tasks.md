## 1. Dependency
- [x] 1.1 Add `genesis = { git = "https://github.com/charly-vibes/genesis", tag = "v0.1.0" }` to `Cargo.toml` (upgraded to crates.io `genesis-vibes = "0.2"` in upgrade-genesis change).
- [x] 1.2 Verify build with envelope/suggestions/managed_block/suite_linter modules stable (closed: testaruda-3bj.1).

## 2. Adopt suggestions
- [x] 2.1 Register testaruda's command list with `genesis::suggestions::SuggestionEngine` (closed: testaruda-3bj.1).
- [x] 2.2 Wire `main.rs` error sink to emit `genesis::suggestions` fix-footers (closed: testaruda-3bj.1).
- [x] 2.3 Regression: `testaruda slect` (typo) prints "Did you mean 'select'?" (closed: testaruda-3bj.1).

## 3. Adopt managed_block
- [x] 3.1 Source injector mechanics from `genesis::managed_block` (closed: testaruda-3bj.1).
- [x] 3.2 Regression: `testaruda init` injects/refreshes managed blocks (closed: testaruda-3bj.1).

## 4. Adopt shared envelope
- [x] 4.1 Route `select`/`discover`/`validate` `--json` through `genesis::envelope` (closed: testaruda-3bj.1).
- [x] 4.2 Test: top-level keys match the shared shape (closed: testaruda-3bj.1).

## 5. Add doctor (true-suite-minimum verb)
- [x] 5.1 Add `Doctor` variant to the `Commands` enum (closed: testaruda-3bj.2).
- [x] 5.2 Register testaruda's own checks (testaruda.toml schema, adapter config) with `genesis::suite_linter::LinterRegistry` (closed: testaruda-3bj.2).
- [x] 5.3 `doctor` calls `LinterRegistry::run_all()` to run all registered checks (closed: testaruda-3bj.2).
- [x] 5.4 `--fix` applies safe fixes; each check provides its own fix fn (closed: testaruda-3bj.2).

## 6. Ship llm.txt
- [x] 6.1 Hand-write a minimal `llm.txt` now (closed: testaruda-3bj.1).
- [x] 6.2 Switch to `genesis::aix` generation once that module is stable (deferred — genesis::aix not yet available in genesis-vibes 0.2).

## 7. Clean up
- [x] 7.1 `cargo clippy -- -D warnings` clean (closed: testaruda-3bj.1).
- [x] 7.2 Verify tool-craft (genesis `.wai` research) Appendix A.3 testaruda row (closed: testaruda-3bj.2).

## 8. Add `feedback` subcommand (wraps `genesis::feedback`)
- [x] 8.1 Add `Feedback` variant to the `Commands` enum with `KIND` + flags (closed: testaruda-3bj.2).
- [x] 8.2 Read testaruda's error scratch for `--from-last-error` (closed: testaruda-3bj.2).
- [x] 8.3 Default target repo = testaruda's `Cargo.toml` `repository` (closed: testaruda-3bj.2).
- [x] 8.4 Error-footer hook: non-zero exits with no `Fix` print feedback hint (closed: testaruda-3bj.2).
- [x] 8.5 Regression: `testaruda feedback bug --dry-run` prints body + `gh` line (closed: testaruda-3bj.2).