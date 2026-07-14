# Change: Auto-detect project language during `init`

## Why

On a Python-only project, `testaruda discover` runs the default Rust adapter and finds 0 tests. The user must manually edit `testaruda.toml` to change `default = "testaruda-adapter-python"`. This creates a poor first-run experience — the tool appears broken out of the box.

## What Changes

- **ADD** TIA-CHG-009 — `init` SHALL auto-detect the project's primary language by probing for well-known project files (`pyproject.toml`, `Cargo.toml`, `package.json`, etc.)
- **MODIFY** `init` command to set the default adapter based on the detected language
- Fall back to `testaruda-adapter-rust` when no project files match known patterns

## Impact

- Affected specs: `change-detection` (new CHG-009)
- Affected code: `main.rs` — `init` command handler
- Non-breaking: detection is best-effort; manual config override always takes precedence