## 1. Specification work
- [ ] 1.1 Add TIA-CHG-009 requirement in `change-detection/spec.md` with scenarios for Python, Rust, JS/TS, Go detection and unknown fallback

## 2. Implementation
- [ ] 2.1 Add project-file probing logic (check for `pyproject.toml`, `Cargo.toml`, `package.json`, `go.mod`)
- [ ] 2.2 Modify `init` command to set default adapter based on detected language
- [ ] 2.3 Ensure user-provided config always overrides auto-detection
- [ ] 2.4 Update `testaruda.toml` template to reflect detection result

## 3. Validation
- [ ] 3.1 Test init on Python-only project — default adapter is python
- [ ] 3.2 Test init on Rust-only project — default adapter is rust
- [ ] 3.3 Test init on unknown project — fallback to rust
- [ ] 3.4 Test init with explicit user config — override detection