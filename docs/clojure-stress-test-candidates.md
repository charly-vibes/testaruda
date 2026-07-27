# Clojure Stress-Test Candidate Selection

**Date:** 2026-07-27  
**Status:** Draft (adapter not yet built — see testaruda-4tm)

## Selection Criteria

| Criterion | Requirement |
|-----------|-------------|
| Coverage | Existing test suite (clojure.test, cognitect test-runner) |
| Import diversity | `:require`, `:import`, `use`, refer, aliases |
| Size range | Small (<30 source files), medium (30–200), large (200+) |
| Git history | Active maintenance, real commits |
| Project structure | deps.edn or project.clj with standard test layout |
| Test framework | clojure.test, kaocha, cognitect-labs test-runner |

## Selected Projects (6 total)

| # | Project | Size | Build tool | Key characteristics |
|---|---------|------|------------|---------------------|
| 1 | **cheshire** | Small | deps.edn | JSON parsing, stable, minimal deps |
| 2 | **clj-http** | Small | deps.edn | HTTP client, real external deps |
| 3 | **ring** | Medium | deps.edn | Web server, middleware chain, multi-module |
| 4 | **re-frame** | Medium | deps.edn | ClojureScript SPA framework, reagent interop |
| 5 | **metabase** | Large | deps.edn | BI platform, massive dep tree |
| 6 | **leiningen** | Large | project.clj | Build tool, project generator, plugin system |

---

## Report Cards

### 1. cheshire — Fast JSON encoding/decoding

| Metric | Value |
|--------|-------|
| **Source** | [github.com/dakrone/cheshire](https://github.com/dakrone/cheshire) |
| **Size** | ~3,000 LOC, ~15 source files |
| **Test framework** | clojure.test |
| **Build tool** | deps.edn + Leiningen |
| **Import patterns** | Simple `:require` with aliases |

**Why selected:** Minimal baseline. Fast clone, straightforward import structure.

---

### 2. clj-http — HTTP Client

| Metric | Value |
|--------|-------|
| **Source** | [github.com/dakrone/clj-http](https://github.com/dakrone/clj-http) |
| **Size** | ~5,000 LOC, ~20 source files |
| **Test framework** | clojure.test |
| **Build tool** | deps.edn |
| **Import patterns** | External deps, conditional imports, async |

**Why selected:** Small codebase with real external dependencies. Tests basic `:require` resolution.

---

### 3. ring — Clojure Web Framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/ring-clojure/ring](https://github.com/ring-clojure/ring) |
| **Size** | ~10,000 LOC, ~40 source files |
| **Test framework** | clojure.test |
| **Build tool** | deps.edn |
| **Import patterns** | Multi-module `ring-core`, `ring-jetty-adapter`, middleware chains |

**Why selected:** Multi-module project with middleware pattern. Tests adapter's cross-module `:require` resolution.

---

### 4. re-frame — ClojureScript SPA Framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/day8/re-frame](https://github.com/day8/re-frame) |
| **Size** | ~15,000 LOC, ~50 source files |
| **Test framework** | clojure.test + cljs.test |
| **Build tool** | deps.edn |
| **Import patterns** | ClojureScript-specific macros, reagent interop, complex re-exports |

**Why selected:** Modern ClojureScript project with complex macro/var interop. Tests adapter's handling of `.cljs` files and cross-compilation imports.

---

### 5. metabase — Business Intelligence

| Metric | Value |
|--------|-------|
| **Source** | [github.com/metabase/metabase](https://github.com/metabase/metabase) |
| **Size** | ~200,000+ LOC, ~1,000+ source files |
| **Test framework** | clojure.test |
| **Build tool** | deps.edn |
| **Import patterns** | Massive dependency tree, Java interop, driver plugins, Ring middleware |

**Why selected:** Largest open-source Clojure project. Tests adapter at extreme scale with Java interop and multi-driver architecture.

---

### 6. leiningen — Build automation

| Metric | Value |
|--------|-------|
| **Source** | [github.com/technomancy/leiningen](https://github.com/technomancy/leiningen) |
| **Size** | ~40,000 LOC, ~100 source files |
| **Test framework** | clojure.test |
| **Build tool** | Self-hosted (project.clj) |
| **Import patterns** | Plugin system, AOT compilation, complex `:require` with inheritance |

**Why selected:** Classic Clojure project with complex build and plugin architecture. Tests adapter's handling of `project.clj`-based projects (alternative to deps.edn).