## 1. Specification work
- [x] 1.1 Add TIA-CHG-009 requirement in `change-detection/spec.md` with scenarios for Python, Rust, JS/TS, Go detection and unknown fallback

## 2. Implementation
- [x] 2.1 Add project-file probing logic (check for `pyproject.toml`, `Cargo.toml`, `package.json`, `go.mod`) — `detect_project_language()` in `src/config.rs`
- [x] 2.2 Modify `init` command to set default adapter based on detected language — in `main.rs` init handler, calls `detect_project_language`
- [x] 2.3 Ensure user-provided config always overrides auto-detection — config loading checks user-supplied adapter first, then falls back to detected default
- [x] 2.4 Update `testaruda.toml` template to reflect detection result — `Config::write_default` uses detected language to set default adapter

## 3. Validation
- [x] 3.1 Test init on Python-only project — default adapter is python — `detect_project_language` returns `Some("testaruda-adapter-python")` for pyproject.toml
- [x] 3.2 Test init on Rust-only project — default adapter is rust — `detect_project_language` returns `Some("testaruda-adapter-rust")` for Cargo.toml
- [x] 3.3 Test init on unknown project — fallback to rust — `detect_project_language` returns `None` → `Config::write_default` defaults to rust
- [x] 3.4 Test init with explicit user config — override detection — config loading respects user-supplied adapter before detection