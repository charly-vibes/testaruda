## Precondition: resolve blocking decisions
- [x] Resolve `Base.reset_coverage()` spike (design.md Decision 4 caveat) — confirmed: `Base.reset_coverage()` does not exist in Julia 1.12.5. Subprocess isolation is unconditionally necessary.
- [x] Resolve multi-package monorepo scoping (design.md Decision 5) — adopted per-package invocation (Option 2). Adapter handles one package; CI invokes testaruda per package.

## 1. Testimonial.jl adapter implementation
- [x] 1.1 Add `bin/testaruda_adapter.jl` entry point (thin JSON protocol dispatcher)
- [x] 1.2 Implement `handshake` command: respond with languages=`["julia"]`, granularity=`"file"`, capabilities `{symbol_model_complete: false, fingerprinting: true, runtime_edges: true}`
- [ ] 1.3 Implement `discover` command: walk test directories, parse `@testitem` blocks via `ASTParser.jl`. Node IDs = `test_file:line` (stable, location-based)
- [ ] 1.4 Implement `static-deps` command: return changed files as `unresolved` on first invocation; return coverage-map edges on subsequent invocations
- [x] 1.5 Implement `fingerprint` command: compute SHA-256 content hashes for requested files
- [ ] 1.6 Implement `ingest` command: spawn per-item subprocess with `--code-coverage=user`, parse `.jl.cov` via `Coverage.jl`, return runtime edges
- [x] 1.7 Implement `run-args` command: emit `ReTestItems.runtests` invocation args filtered by selected items
- [ ] 1.8 Add error handling: graceful timeout, malformed input, fallback to all-tests on failure

## 2. Testing
- [ ] 2.1 Add a `ReTestItems.jl` fixture project for integration testing
- [ ] 2.2 Write integration tests exercising the full adapter pipeline (handshake → discover → ingest → static-deps → select → run-args)
- [ ] 2.3 Write seeded-fault recall test: mutate a covered line, confirm the test that should catch it is still selected
- [ ] 2.4 Write multi-package monorepo test (once Decision 5 is resolved)
- [ ] 2.5 Write espectacular contracts covering adapter protocol requirements (ADAPT-014, ADAPT-015, ADAPT-016)
- [ ] 2.6 Run `ah check` to validate spec-contract correspondence

## 3. Documentation
- [ ] 3.1 Update `testaruda.toml` example config to show Julia adapter invocation
- [ ] 3.2 Add section to getting-started docs explaining Julia support preconditions (install Julia, `Pkg.add Testimonial.jl`)

## 4. Core config (only if monorepo scoping decision requires it)
- [ ] 4.1 If Decision 5 chooses walk-all-`Project.toml`s: add Julia artifact names to default discover exclude list