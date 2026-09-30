# Julia Stress-Test Candidate Selection

**Date:** 2026-07-27  
**Status:** Final

## Selection Criteria

| Criterion | Requirement |
|-----------|-------------|
| Coverage | Existing test suite using ReTestItems, TestSet, or @testitem |
| Import diversity | `using`, `import`, relative module deps, conditional deps |
| Size range | Small (<30 source files), medium (30–200), large (200+) |
| Git history | Active maintenance, real commits, Project.toml |
| Project structure | Standard Julia package layout (src/ + test/) |
| Test framework | ReTestItems.jl, Test.jl |

## Selected Projects (6 total)

| # | Project | Size | Framework | Key characteristics |
|---|---------|------|-----------|---------------------|
| 1 | **JSON.jl** | Small | Test.jl | JSON parser, stable, minimal deps |
| 2 | **CSV.jl** | Small | Test.jl | CSV handling, pure Julia, structured tests |
| 3 | **DataFrames.jl** | Medium | Test.jl | Data manipulation, large test suite |
| 4 | **HTTP.jl** | Medium | Test.jl | HTTP client/server, async, conditional deps |
| 5 | **Plots.jl** | Large | Test.jl | Visualization, multi-backend, huge dep graph |
| 6 | **DifferentialEquations.jl** | Large | Test.jl | Scientific computing, massive ecosystem |

---

## Report Cards

### 1. JSON.jl — JSON Parser

| Metric | Value |
|--------|-------|
| **Source** | [github.com/JuliaIO/JSON.jl](https://github.com/JuliaIO/JSON.jl) |
| **Size** | ~2,000 LOC, ~10 source files |
| **Test framework** | Test.jl |
| **Project layout** | `src/` + `test/` |
| **Test dir** | `test/` |

**Import patterns:**
- Minimal: `Base`, `Core` only
- No external dependencies beyond stdlib
- Simple `module` structure

**Why selected:** Minimal baseline. Tests adapter on the simplest possible Julia package.

---

### 2. CSV.jl — CSV File Reader

| Metric | Value |
|--------|-------|
| **Source** | [github.com/JuliaData/CSV.jl](https://github.com/JuliaData/CSV.jl) |
| **Size** | ~8,000 LOC, ~25 source files |
| **Test framework** | Test.jl |
| **Project layout** | `src/` + `test/` |
| **Test dir** | `test/` |

**Import patterns:**
- External: `DataFrames`, `PooledArrays`, `SentinelArrays`, `InlineStrings`
- `using` and `import` statements
- Type parameters and multiple dispatch

**Why selected:** Small, clean codebase with real external deps. Tests basic `using`/`import` resolution.

---

### 3. DataFrames.jl — Data Manipulation

| Metric | Value |
|--------|-------|
| **Source** | [github.com/JuliaData/DataFrames.jl](https://github.com/JuliaData/DataFrames.jl) |
| **Size** | ~30,000 LOC, ~80 source files |
| **Test framework** | Test.jl |
| **Coverage** | ~85% |
| **Project layout** | `src/` + `test/` |
| **Test dir** | `test/` |

**Import patterns:**
- Multiple external deps: `Tables`, `DataAPI`, `SortingAlgorithms`, `CategoricalArrays`
- `include("submodule.jl")` pattern for code organization
- Re-exports via `import ...: ...`
- Conditional dependency loading

**Why selected:** Medium-sized real-world Julia package. Tests adapter's ability to follow `include()` chains and resolve multi-package import graphs.

---

### 4. HTTP.jl — HTTP Client and Server

| Metric | Value |
|--------|-------|
| **Source** | [github.com/JuliaWeb/HTTP.jl](https://github.com/JuliaWeb/HTTP.jl) |
| **Size** | ~25,000 LOC, ~60 source files |
| **Test framework** | Test.jl |
| **Coverage** | ~75% |
| **Project layout** | `src/` + `test/` |
| **Test dir** | `test/` |

**Import patterns:**
- Network deps: `Sockets`, `MbedTLS`, `URIs`
- Async patterns with `@async` and `@sync`
- Conditional imports for SSL vs non-SSL code paths
- Version-dependent imports

**Why selected:** Tests adapter's handling of conditional deps and networking-specific code. Async patterns add complexity.

---

### 5. Plots.jl — Visualization Library

| Metric | Value |
|--------|-------|
| **Source** | [github.com/JuliaPlots/Plots.jl](https://github.com/JuliaPlots/Plots.jl) |
| **Size** | ~60,000 LOC, ~150 source files |
| **Test framework** | Test.jl |
| **Coverage** | ~70% |
| **Project layout** | `src/` + `test/` |
| **Test dir** | `test/` |

**Import patterns:**
- Backend-specific code: `GR`, `PyPlot`, `PlotlyJS`, `PGFPlotsX`
- Conditional imports per backend
- Heavy `include()` tree
- Re-export chains
- Multiple dispatch method definitions in separate files

**Why selected:** Large, backend-heavy codebase. Tests adapter's ability to handle conditional imports and multi-backend dependency resolution.

---

### 6. DifferentialEquations.jl — Scientific Computing

| Metric | Value |
|--------|-------|
| **Source** | [github.com/SciML/DifferentialEquations.jl](https://github.com/SciML/DifferentialEquations.jl) |
| **Size** | ~100,000+ LOC across the SciML ecosystem |
| **Project layout** | Meta-package re-exporting from many sub-packages |
| **Test dir** | Per-sub-package `test/` |

**Import patterns:**
- Complex meta-package re-export structure
- Extension packages for different solver backends
- Conditional compilation per-problem type
- Deep `include()` chains
- Scientific computing with C/Rust interop

**Why selected:** Maximum complexity. Tests adapter's ability to handle meta-packages and deeply nested re-export chains.

---

## Selection Rationale

**Structural diversity:**
- 2 small (JSON.jl, CSV.jl) for baseline
- 2 medium (DataFrames.jl, HTTP.jl) for multi-dep packages
- 2 large (Plots.jl, DiffEq.jl) for scale and complexity

**Import pattern coverage:**
- Single-module vs multi-file `include()` packages
- `using` vs `import` patterns
- Conditional backend imports
- Meta-package re-exports

**Risk mitigation:**
- JSON.jl and CSV.jl are fast clones — good quick feedback
- Plots.jl has heavy backend dependencies; skip if CI time constrained
- DiffEq.jl is an ecosystem-level meta-package; test individual sub-packages first if needed