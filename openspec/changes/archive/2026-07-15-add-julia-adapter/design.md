## Context

Adding Julia support involves two changes: (1) core config defaults so `testaruda init` and auto-detection work for Julia projects, and (2) an external adapter binary that speaks the JSON protocol.

The adapter must be a separate executable (per TIA-ADAPT-001 and TIA-PORT-002) — the core never embeds language-specific logic.

## Goals / Non-Goals

**Goals:**
- `testaruda init` auto-detects Julia projects and generates a working config
- A Julia project can be discovered and selected without manual adapter configuration
- The adapter handles Julia's standard `Test.jl` framework (stdlib, no external deps)

**Non-Goals:**
- Full coverage of third-party test frameworks (ReTest.jl, SafeTestsets.jl, etc.) — these can be added later
- Symbol-level dependency resolution — file-level granularity with `symbol_model_complete: false` is the initial target
- Module-level dependency resolution (`import`/`using` to source files) — deferred to a follow-up change; initial adapter uses `include`-only edges
- Julia-specific runtime edge collection — `runtime_edges: false` initially

## Decisions

### Decision: Adapter language
The adapter SHALL be implemented in Julia itself. This is the natural choice: it can parse Julia source with the language's own parser (`JuliaSyntax.jl` or `Meta.parse`), invoke `Pkg.test` programmatically, and parse `Test.jl` output natively.

Alternatives considered:
- **Rust adapter** — Would require embedding a Julia parser, adding complexity and maintenance burden. No clear benefit.
- **Shell/polyglot** — Fragile regex-based parsing; not robust.

### Decision: Granularity — file-level, `symbol_model_complete: false`
Julia's `Test.jl` does not expose per-symbol test dependencies natively. File-level granularity (`include`-based dependency tracking) is the pragmatic initial target. Symbol-level refinement can be added in a future change.

### Decision: Discover strategy — walk `test/` directory; `@testset` = test item
The adapter SHALL walk the `test/` directory using Julia's `walkdir` (or `readdir` recursively) and parse `.jl` files for top-level `@testset` blocks. Each top-level `@testset` becomes one test item. Leaf `@test` assertions are NOT individual items — they are part of their enclosing `@testset`.

**Stable node IDs:** Test item node IDs SHALL be derived from source location (`file:line`) rather than the `@testset` name, because dynamic names (e.g. `@testset "iteration $i" for i in 1:10`) produce non-deterministic IDs across invocations.

### Decision: Static deps — `include` parsing (file-level only)
The adapter SHALL scan Julia source files for `include("...")` calls to build file-level dependency edges. `Base.include_dependency` is ignored — it is a low-level precompilation utility rarely used in user code.

Module-level `import X`/`using X` resolution is **deferred** to a follow-up change. The initial adapter produces file-level edges from `include` calls only. Dependencies introduced via `import`/`using` are not tracked, meaning the dependency graph is a conservative under-approximation for those edges — the core's soundness invariant (TIA-SAFE-001) still holds because the adapter reports unresolved files for non-`include` dependencies, triggering the core's fallback mechanism.

### Decision: Fingerprinting — BLAKE3
The adapter SHALL compute BLAKE3 content hashes (matching the core's choice) for files at the declared granularity.

### Decision: Run args — `Project.toml`-aware entry point
The adapter SHALL first read `Project.toml` for a `[tests]` target entry point. If found, it uses that path. Otherwise, it falls back to `test/runtests.jl`. The default collection path is `"test/"`. Future versions can support a configurable entry point.

Default args: `["julia", "--project=.", "-e", "include(\"<entry_point>\")"]`

### Decision: Ingest — `Test.jl` output parsing
The adapter SHALL parse `Test.jl`'s standard output format line-by-line. Each line matching the pattern:
```
^\s*(Test|Expression|Error)\s+([^:]+):\s+(Passed|Failed|Error)(\s+.*)?(\s+[\d.]+\s*s)?$
```
extracts the test name, outcome (`Passed`→`PASS`, `Failed`→`FAIL`, `Error`→`ERROR`), and optional duration. Lines are grouped by indentation level: the outermost `@testset` is the test item; nested `@testset` blocks are attributed to the parent.

**Risks:**
- Custom `AbstractTestSet` types may emit non-standard output — the adapter SHALL fall back to using the `@testset` name as the test item and the file-level outcome in that case.
- Threaded test execution may interleave output lines — the adapter SHALL parse line-by-line with a stack-based indentation tracker.

Examples:
- `Test Passed    My test set  0.3s` → outcome `PASS`, item `My test set`, duration `0.3s`
- `Test Failed    Arithmetic      ` → outcome `FAIL`, item `Arithmetic`

## Risks / Trade-offs

| Risk | Impact | Mitigation |
|------|--------|------------|
| Julia's `Test.jl` output format is version-dependent | Adapter may fail on older/newer Julia versions | Pin minimum Julia version (≥ 1.6 LTS) in `Project.toml`; test against latest stable release in CI |
| `include` resolution is complex (relative paths, `LOAD_PATH`, `Project.toml` source paths) | Missing edges → false negatives (missed selections) | Start conservative: resolve `include` relative to file location; flag unresolvable paths as `unresolved` |
| Julia runtime not installed | Adapter binary not found by core | Core's `AdapterIO::spawn` handles missing binary gracefully; falls back to all-tests |
| `import X` / `using X` module resolution is expensive | Slow `static-deps` | Deferred to follow-up change; initial adapter uses `include`-only edges (under-approximation triggers fallback) |
| No `Project.toml` or non-standard project structure | Detection fails | Fall back to default adapter; user can configure manually. Secondary detection via `test/runtests.jl` mitigates this |

## Migration Plan

1. Merge core config changes (tasks 1.1–1.4) — no breaking changes, just new defaults
2. Create `testaruda-adapter-julia` repo and implement the adapter (tasks 2.1–2.8)
3. Add fixture project and integration tests (tasks 3.1–3.4)
4. Release both in tandem: core v0.1.x + adapter v0.1.0
5. Rollback: revert core config defaults; adapter just doesn't need to be installed

## Julia Version Support

The adapter SHALL target Julia ≥ 1.6 LTS (long-term support release). This guarantees availability of:
- `Base.include` for file dependency parsing
- `Meta.parse` for Julia source code analysis
- `readdir` / `walkdir` for recursive directory traversal
- Standard `Test.jl` output format (stable since Julia 1.0, with minor format adjustments in 1.6+)

Key features that Julia ≥ 1.6 enables:
- `@testset` with `show` output stable across versions
- `Pkg.test` integration for `Project.toml` test target detection (available since Julia 1.0)
- `Base.find_package` for module resolution (reserved for future import/using support)

## Open Questions

- Should `Manifest.toml` changes trigger a full run (like lockfiles per TIA-SAFE-005)? Possibly — but this is a separate change.
- Should the adapter handle `Pkg.test` directly (invoking the test target in `Project.toml`) rather than hardcoding `test/runtests.jl`? Yes, but that's a future enhancement.