# Tasks: Add .NET adapter detection (external `titi` binary)

## 1. Core change — shell-split adapter command strings (TIA-ADAPT-024, shared with Julia adapter)
- [x] 1.1 Add `shell-words` crate dependency; implement `parse_command_string()` + `spawn_adapter()` in `src/adapter.rs`; route all 5 spawn sites in `src/main.rs` through `spawn_adapter`
- [x] 1.2 Unit test: `"testaruda-adapter-rust"` → `("testaruda-adapter-rust", [])`
- [x] 1.3 Unit test: `"titi testaruda-adapter"` → `("titi", ["testaruda-adapter"])`
- [x] 1.4 Unit test: `"julia --project=. -e '...'"` → quoted body preserved as one arg
- [x] 1.5 Unit test: empty command string → config-error diagnostic
- [x] 1.6 Unit test: quoted Windows path → path with spaces handled
- [ ] 1.7 Verify the shell-split works on Windows (backslash paths, `shell-words` Windows behavior) — create a Windows CI job or document the tested behavior
- [ ] 1.8 Verify the Julia adapter change (`add-julia-adapter-via-testimonial`) is unblocked by this step (its config form assumes the split works); coordinate with the Julia change owner so this task isn't duplicated

## 2. .NET adapter mapping (opt-in, manual config)
- [x] 2.1 Document the `testaruda.toml` extension mappings for .NET in the adapter's user-facing docs (including `.cs`, `.fs`, `.vb` source extensions):
  ```toml
  [adapters.extensions]
  ".cs"    = "titi testaruda-adapter"
  ".fs"    = "titi testaruda-adapter"
  ".vb"    = "titi testaruda-adapter"
  ".csproj" = "titi testaruda-adapter"
  ".sln"    = "titi testaruda-adapter"
  ".slnx"   = "titi testaruda-adapter"
  ```

## 3. Validation & error paths
- [x] 3.1 When `titi` is not on `PATH` and the user has configured the titi mapping, `testaruda select` SHALL fall back to full-suite selection per TIA-ADAPT-012 and record the missing-binary failure (verify the existing `AdapterIO::spawn` NotFound path already handles this — no new code expected once shell-split is in place). The failure diagnostic SHALL name the resolved binary (`titi`), not the full command string.
- [x] 3.2 When the titi adapter exits non-zero (e.g. graph-build failure during handshake), the core SHALL fall back to full-suite per TIA-ADAPT-012 (verify existing `read_response`/`ProcessExit` path handles this)

## 4. Tests
- [ ] 4.1 Integration test (gated on titi being installed): `testaruda select` against a synthetic .NET monorepo with a changed `.cs` file returns a non-empty selection with `static` edges sourced from titi's `MonorepoGraph`
- [ ] 4.2 Integration test (gated on titi being installed): a changed `.fs` file is routed to titi and returns a non-empty selection (verifies F# source routing)
- [x] 4.3 Integration test (titi NOT installed): `testaruda select` with a titi mapping falls back to full-suite selection and records the missing-binary failure
- [ ] 4.4 Integration test (Julia smoke): with the shell-split in place, the Julia adapter's command-string config form spawns correctly (this validates the shared infrastructure; coordinate with the Julia change to avoid duplicate fixtures)
- [ ] 4.5 Integration test (polyglot): a `.cs` file outside titi's `MonorepoGraph` is routed to titi, returned as `unresolved`, and testaruda applies the over-approximation fallback (TIA-SAFE-004) — verifies the polyglot-routing edge case

## 5. Documentation
- [x] 5.1 Update `docs/getting-started.md` or the adapter list to include the .NET / titi adapter, noting it is an external binary (not a workspace crate), is opt-in (not auto-detected, following the Julia precedent), and linking to `github.com/sashakile/titi` for installation
- [x] 5.2 Note in `testaruda.toml` example comments that extension-mapping values are command strings that the core shell-splits (per TIA-ADAPT-024), so multi-token invocations (e.g. `titi testaruda-adapter`, `julia --project=. -e '...'`) are supported

## 6. Coordination
- [ ] 6.1 Confirm titi change `add-testaruda-adapter` (CLI-19) is implemented, including the `runtime_edges: false` handshake declaration, and the `titi` binary is installable before merging this change
- [ ] 6.2 Record the cross-repo dependency in both repos: this change depends on titi `titi-co9` / epic `titi-dik`; titi's adapter depends on testaruda for the shell-split core change (TIA-ADAPT-024) so the `titi testaruda-adapter` command-string config form works. titi does NOT depend on testaruda for detection, since .NET is opt-in.
- [ ] 6.3 Coordinate the shell-split task (1.1) with the Julia adapter change owner — both changes need it; agree on which change lands the core edit so it isn't duplicated. This change's proposal introduces the shell-split (TIA-ADAPT-024) because no existing change does, but if the Julia change lands first and includes it, task 1.1 is already done.

## 7. Design document
- [x] 7.1 Write `design.md` covering: the `shell-words` crate decision (over `shlex`), the five-spawn-site refactor via `spawn_adapter` helper, the `.cs`/`.fs`/`.vb` routing rationale, the opt-in-vs-detection decision (following Julia), and the polyglot-routing acknowledgment (misrouted files → titi `unresolved` → TIA-SAFE-004)
