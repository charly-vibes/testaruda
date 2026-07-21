## Precondition: resolve blocking decisions
- [x] Resolve `Base.reset_coverage()` spike (design.md Decision 4 caveat) — confirmed: `Base.reset_coverage()` does not exist in Julia 1.12.5. Subprocess isolation is unconditionally necessary.
- [x] Resolve multi-package monorepo scoping (design.md Decision 5) — adopted per-package invocation (Option 2). Adapter handles one package; CI invokes testaruda per package.

## 1. Testimonial.jl adapter implementation
- [x] 1.1 Add `bin/testaruda_adapter.jl` entry point (thin JSON protocol dispatcher)
- [x] 1.2 Implement `handshake` command: respond with languages=`["julia"]`, granularity=`"file"`, capabilities `{symbol_model_complete: false, fingerprinting: true, runtime_edges: true}`
- [x] 1.3 Implement `discover` command: walk test directories, parse `@testitem` blocks via `ASTParser.jl`. Node IDs = `test_file:line` (stable, location-based)
- [x] 1.4 Implement `static-deps` command: return changed files as `unresolved` on first invocation; return coverage-map edges on subsequent invocations
- [x] 1.5 Implement `fingerprint` command: compute SHA-256 content hashes for requested files
- [x] 1.6 Implement `ingest` command: spawn per-item subprocess with `--code-coverage=user`, parse `.jl.cov` via `Coverage.jl`, return runtime edges. Also supports TIA-ADAPT-008 `run_output` format.
- [x] 1.7 Implement `run-args` command: emit `ReTestItems.runtests` invocation args filtered by selected items
- [x] 1.8 Add error handling: malformed JSON, unknown command, missing params, invalid node IDs, file-not-found. Timeout and fallback-to-all-tests are handled by testaruda core (TIA-ADAPT-012, TIA-ADAPT-013).

## 2. Testing
- [x] 2.1 Add a `ReTestItems.jl` fixture project for integration testing — `tests/fixtures/julia/` with 3 @testitems
- [x] 2.2 Write integration tests exercising the full adapter pipeline — 7 tests in `tests/adapter_julia.rs` covering handshake, discover, fingerprint, static-deps, ingest, run-args, and a full pipeline test
- [ ] 2.3 Write seeded-fault recall test — deferred. Requires real coverage recording via Julia subprocess. Testimonial.jl has its own seeded-fault tests. testaruda-side test would need Julia installed with fixture deps resolved.
- [ ] 2.4 Write multi-package monorepo test — Decision 5 chose per-package invocation, so the adapter handles one package at a time. No adapter-level change needed.
- [x] 2.5 Write espectacular contracts — 4 contracts in `.espectacular/adapter-protocol/` (julia-handshake, julia-discovery, julia-dependency-analysis, julia-full-pipeline)
- [ ] 2.6 Run `ah check` to validate spec-contract correspondence — `ah check` timed out running all contract shell commands. Contracts point to valid `cargo test` commands that pass individually.

## 3. Documentation
- [x] 3.1 Update `testaruda.toml` example config to show Julia adapter invocation — updated `docs/configuration.md` and default template in `config.rs:write_default()`
- [x] 3.2 Add section to getting-started docs explaining Julia support preconditions — added to `docs/getting-started.md` Prerequisites section

## 4. Core config (only if monorepo scoping decision requires it)
- [ ] 4.1 If Decision 5 chooses walk-all-`Project.toml`s: add Julia artifact names to default discover exclude list — Decision 5 chose per-package invocation. Not needed.