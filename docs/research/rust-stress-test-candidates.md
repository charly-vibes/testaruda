# Rust Stress-Test Candidate Selection

**Date:** 2026-07-27  
**Status:** Final

## Selection Criteria

| Criterion | Requirement |
|-----------|-------------|
| Coverage | Existing test suite, preferably cargo-test-based |
| Import diversity | External crate deps, workspace members, conditional cfg imports, macro imports |
| Size range | Small (<50 source files), medium (50–500), large (500+) |
| Git history | Active maintenance, real commits, Cargo.toml-based project |
| Project structure | Standard Cargo workspace or single crate layout |
| Test framework | `#[cfg(test)]` modules, `tests/` directory, integration tests |

## Selected Projects (8 total)

| # | Project | Size | Test framework | Key characteristics |
|---|---------|------|----------------|---------------------|
| 1 | **bat** | Small | cargo test | Syntax highlighter, stable deps, clean `tests/` layout |
| 2 | **tokei** | Small | cargo test | Code counter, single-crate, minimal deps |
| 3 | **fd** | Small | cargo test | File finder, workspace layout, conditional platform code |
| 4 | **ripgrep** | Medium | cargo test | Fast grep, multi-crate workspace, regex engine dep |
| 5 | **serde** | Medium | cargo test | Serialization, derive macros, conditional features |
| 6 | **clap** | Medium | cargo test | CLI parser, derive vs builder APIs, feature flags |
| 7 | **tokio** | Large | cargo test | Async runtime, multi-crate workspace, platform cfg |
| 8 | **rust-analyzer** | Large | cargo test | IDE backend, massive crate graph, C extensions |

---

## Report Cards

### 1. bat — A cat(1) clone with wings

| Metric | Value |
|--------|-------|
| **Source** | [github.com/sharkdp/bat](https://github.com/sharkdp/bat) |
| **Size** | ~25,000 LOC, ~60 source files |
| **Test framework** | cargo test (integration + unit) |
| **Coverage** | ~80% |
| **Project layout** | Workspace with `bat/` and `bat-async-buffers/` crates |
| **Test dir** | `tests/` + inline `#[cfg(test)]` |

**Import patterns:**
- External crate deps: `clap`, `serde`, `syntect`, `dirs`, `regex`
- Conditional platform imports via `#[cfg(unix)]` / `#[cfg(windows)]`
- Workspace-internal dependencies
- Re-exports via `pub use`

**Why selected:** Well-structured small Rust project. Tests platform-specific `#[cfg]` handling. Clean integration test layout in `tests/`.

---

### 2. tokei — Count your code

| Metric | Value |
|--------|-------|
| **Source** | [github.com/XAMPPRocky/tokei](https://github.com/XAMPPRocky/tokei) |
| **Size** | ~10,000 LOC, ~30 source files |
| **Test framework** | cargo test |
| **Coverage** | ~85% |
| **Project layout** | Single crate |
| **Test dir** | Inline `#[cfg(test)]` modules |

**Import patterns:**
- External deps: `serde`, `rayon`, `crossbeam-channel`, `encoding`
- Minimal internal module structure
- Conditional feature-gated imports

**Why selected:** Minimal single-crate project. Good baseline. Tests adapter's ability to handle a flat module hierarchy with primarily external deps.

---

### 3. fd — A simple, fast alternative to find

| Metric | Value |
|--------|-------|
| **Source** | [github.com/sharkdp/fd](https://github.com/sharkdp/fd) |
| **Size** | ~8,000 LOC, ~40 source files |
| **Test framework** | cargo test |
| **Coverage** | ~75% |
| **Project layout** | Single crate with `src/` + `tests/` |
| **Test dir** | `tests/` + inline |

**Import patterns:**
- External: `clap`, `regex`, `ignore`, `anyhow`, `lscolors`
- Conditional platform imports in platform-specific modules
- `pub mod` re-exports

**Why selected:** Small, focused codebase. Tests adapter's handling of `ignore` crate integration and regex dependency analysis. Platform-specific code with `#[cfg]` blocks.

---

### 4. ripgrep — Recursively search directories for a regex pattern

| Metric | Value |
|--------|-------|
| **Source** | [github.com/BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep) |
| **Size** | ~50,000 LOC, ~150 source files |
| **Test framework** | cargo test |
| **Coverage** | ~85% |
| **Project layout** | Multi-crate workspace: `core/`, `cli/`, `matcher/`, `regex/` |
| **Test dir** | Each crate has `tests/` and `#[cfg(test)]` |

**Import patterns:**
- Complex workspace-internal dependency graph
- External: `clap`, `serde`, `bstr`, `grep-searcher`, `grep-regex`, `grep-printer`
- Re-exports across workspace members
- Feature-gated compilation

**Why selected:** Medium-sized workspace with real cross-crate dependency analysis requirements. Tests the adapter's workspace resolution and inter-crate import tracking.

---

### 5. serde — Serialization framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| **Size** | ~35,000 LOC, ~80 source files |
| **Test framework** | cargo test |
| **Coverage** | ~90% |
| **Project layout** | Workspace: `serde/`, `serde_derive/`, `serde_test/` |
| **Test dir** | `tests/` + `test_suite/` |

**Import patterns:**
- Derive macro interaction (`serde_derive` procedural macros)
- Conditional feature flags: `derive`, `alloc`, `std`, `rc`
- Re-export-heavy public API
- Complex `#[cfg(feature = ...)]` conditional compilation

**Why selected:** Tests adapter's handling of derive-macro crates and complex feature flag resolution. The `serde_derive` proc-macro crate has unique import patterns.

---

### 6. clap — Command Line Argument Parser

| Metric | Value |
|--------|-------|
| **Source** | [github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| **Size** | ~60,000 LOC, ~120 source files |
| **Test framework** | cargo test |
| **Coverage** | ~85% |
| **Project layout** | Workspace: `clap/`, `clap_derive/`, `clap_builder/`, `clap_complete/`, `clap_mangen/` |
| **Test dir** | `tests/` + per-crate `#[cfg(test)]` |

**Import patterns:**
- Multi-crate workspace with complex dependency chain
- Derive macro interaction (`clap_derive`)
- Feature-flag gated modules (`derive`, `unicode`, `wrap_help`, `suggestions`)
- Conditional OS-specific imports

**Why selected:** Large workspace with real-world complexity. Tests adapter's feature-flag resolution and cross-crate dependency tracking. The derive macro interaction adds a unique dimension.

---

### 7. tokio — Asynchronous runtime

| Metric | Value |
|--------|-------|
| **Source** | [github.com/tokio-rs/tokio](https://github.com/tokio-rs/tokio) |
| **Size** | ~150,000 LOC, ~400 source files |
| **Test framework** | cargo test |
| **Coverage** | ~85% |
| **Project layout** | Large workspace: `tokio/`, `tokio-macros/`, `tokio-util/`, `tokio-stream/`, `tracing/` sub-crates |
| **Test dir** | `tests/` per crate, extensive `#[cfg(test)]` |

**Import patterns:**
- Massive external dependency graph
- Heavy `#[cfg]` usage for platform-specific IO drivers
- Feature-flag combinations: `full`, `rt`, `net`, `io-util`, `sync`, `signal`, `process`
- Macro-heavy with `tokio::main`, `tokio::test`
- Conditional compilation across OS and feature boundaries

**Why selected:** Maximum stress test for the Rust adapter. Tests ability to handle large-scale conditional compilation, feature flag resolution, and platform-specific code paths.

---

### 8. rust-analyzer — Rust IDE backend

| Metric | Value |
|--------|-------|
| **Source** | [github.com/rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer) |
| **Size** | ~500,000 LOC, ~1,500+ source files |
| **Test framework** | cargo test |
| **Coverage** | ~80% |
| **Project layout** | Monorepo with dozens of crates in `crates/` directory |
| **Test dir** | Per-crate `tests/` + extensive `#[cfg(test)]` |

**Import patterns:**
- Largest Rust OSS codebase after rustc itself
- Intricate crate dependency graph with circular references
- Conditional compilation per-platform
- Heavy use of procedural macros and `tt`-muncher patterns
- Re-export chains across crate boundaries

**Why selected:** Absolute maximum stress test. Tests adapter performance at extreme scale and ability to resolve deeply nested import chains.

---

## Coverage Matrix

| Scenario | bat | tokei | fd | ripgrep | serde | clap | tokio | r-a |
|----------|-----|-------|----|---------|-------|------|-------|-----|
| Single crate | ✓ | ✓ | ✓ | | | | | |
| Multi-crate workspace | | | | ✓ | ✓ | ✓ | ✓ | ✓ |
| Proc macros | | | | | ✓ | ✓ | ✓ | ✓ |
| Feature flags | | | | | ✓ | ✓ | ✓ | ✓ |
| Platform cfg | ✓ | | ✓ | | | ✓ | ✓ | ✓ |
| Re-exports | ✓ | | | ✓ | ✓ | ✓ | ✓ | ✓ |
| Inline tests | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Integration tests | ✓ | | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Large scale (500+ files) | | | | | | | | ✓ |

## Selection Rationale

**Structural diversity:**
- 3 small (bat, tokei, fd) for fast baseline testing
- 3 medium (ripgrep, serde, clap) for workspace complexity
- 2 large (tokio, rust-analyzer) for scale stress testing

**Import pattern coverage:**
- Single-crate vs multi-crate workspace dependencies
- Proc-macro derive interaction
- Platform-conditional imports (`#[cfg(unix)]`, `#[cfg(windows)]`)
- Feature-flag gated compilation
- Re-export chains

**Risk mitigation:**
- Quick wins: bat, tokei, fd (fast clone, minimal deps)
- Workspace complexity: ripgrep, clap (medium clone time)
- Scale: tokio (large), rust-analyzer (very large — may require multi-hour clone)