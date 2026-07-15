## Precondition: resolve blocking decisions
- [ ] Resolve `Base.reset_coverage()` spike (`design.md` Decision 4 caveat) — does Julia ≥1.11 permit sequential in-process resets, or is subprocess-isolation unconditionally necessary?
- [ ] Resolve multi-package monorepo scoping (`design.md` Decision 5) — walk-all-`Project.toml`s vs. per-package invocation

## 1. Testimonial.jl adapter implementation
- [ ] 1.1 Add `bin/testaruda_adapter.jl` entry point (thin JSON protocol dispatcher)
- [ ] 1.2 Implement `handshake` command: respond with languages=`["julia"]`, granularity=`"file"`, capabilities `{symbol_model_complete: false, fingerprinting: true, runtime_edges: true}`
- [ ] 1.3 Implement `discover` command: walk test directories, parse `@testitem` blocks via `ASTParser.jl`. Node IDs = `test_file:testitem_name`
- [ ] 1.4 Implement `static-deps` command: return changed files as `unresolved` on first invocation; return coverage-map edges on subsequent invocations
- [ ] 1.5 Implement `fingerprint` command: compute BLAKE3 content hashes for requested files
- [ ] 1.6 Implement `ingest` command: spawn per-item subprocess with `--code-coverage=user`, parse `.jl.cov` via `Coverage.jl`, return runtime edges
- [ ] 1.7 Implement `run-args` command: emit `ReTestItems.runtests` invocation args filtered by selected items
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
