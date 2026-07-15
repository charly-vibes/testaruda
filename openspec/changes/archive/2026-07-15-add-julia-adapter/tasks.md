> All tasks in this file are **SUPERSEDED** by
> `openspec/changes/add-julia-adapter-via-testimonial/`. See that proposal
> for the current task list.

## 1. Core config changes
- [ ] 1.1 Add `.jl` → `"testaruda-adapter-julia"` to `AdapterConfig::default()` extension map
- [ ] 1.2 Add Julia detection in `detect_project_language()`: check for `Project.toml` existence, return `"testaruda-adapter-julia"`
- [ ] 1.3 Update `write_default()` template to include `".jl" = "testaruda-adapter-julia"` in the generated config
- [ ] 1.4 Add Julia artifact names to default discover exclude list: `Manifest.toml`, `JuliaArtifacts`

## 2. Adapter implementation (external — `testaruda-adapter-julia`)
- [ ] 2.1 Create Julia project scaffold with `Project.toml` and `src/TestarudaAdapter.jl`
- [ ] 2.2 Implement `handshake` command: declare name `testaruda-adapter-julia`, version, protocol=1, languages=`["julia"]`, granularity=`"file"`, capabilities `{symbol_model_complete: false, fingerprinting: true, runtime_edges: false}`
- [ ] 2.3 Implement `discover` command: walk `test/` directory via `readdir`/`walkdir`, parse top-level `@testset` blocks; each top-level `@testset` = one test item. Leaf `@test` is NOT an item. Derive node IDs from source location (`file:line`) not name.
- [ ] 2.4 Implement `static-deps` command: parse `include("...")` calls in Julia source files to build file-level dependency edges. Ignore `include_dependency`. Do not resolve `import`/`using` (deferred to follow-up — file under `unresolved` when path not traceable via `include`).
- [ ] 2.5 Implement `fingerprint` command: compute BLAKE3 content hashes for requested files
- [ ] 2.6 Implement `run-args` command: read `Project.toml` for `[tests]` target first; fall back to `test/runtests.jl`. Emit `["julia", "--project=.", "-e", "include(\"<entry_point>\")"]` with collection path `"test/"`
- [ ] 2.7 Implement `ingest` command: parse `Test.jl` stdout/stderr output to extract per-test results (`PASS`/`FAIL`/`ERROR`), runtime edges, and observed external inputs
- [ ] 2.8 Add error handling: graceful timeout, malformed response, fallback to all-tests on failure

## 3. Testing
- [ ] 3.1 Add a Julia fixture project in `tests/fixtures/` (a minimal `Project.toml` + `test/runtests.jl` with `@testset` blocks)
- [ ] 3.2 Write integration tests exercising the adapter pipeline against the Julia fixture
- [ ] 3.3 Write espectacular contracts covering Julia adapter protocol requirements (ADAPT-014, ADAPT-015, ADAPT-016)
- [ ] 3.4 Run `ah check` to validate spec-contract correspondence
- [ ] 3.5 Run `cargo test` to confirm core changes don't break existing tests