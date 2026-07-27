# Python Stress-Test Candidate Selection

**Ticket:** testaruda-b1i  
**Date:** 2026-07-22  
**Status:** Final

## Selection Criteria

| Criterion | Requirement |
|-----------|-------------|
| Coverage | ≥70% test coverage (≥50% acceptable if structurally valuable) |
| Import diversity | Relative, absolute, namespace, conditional, dynamic imports |
| Size range | Small (<50 files), medium (50–500), large (500+) |
| Git history | Active maintenance, real commits (bugfixes, refactors, features) |
| Project structure | Standard layout (pyproject.toml or setup.py) |
| Test framework | pytest, unittest, or doctest |

## Selected Projects (8 total)

| # | Project | Size | Framework | Key characteristics |
|---|---------|------|-----------|---------------------|
| 1 | **click** | Small | pytest | CLI framework, decorator-heavy, high coverage |
| 2 | **attrs** | Small | pytest | Metaclass machinery, type stubs, stable codebase |
| 3 | **structlog** | Small | pytest | Modern Python, async logging, composition patterns |
| 4 | **httpx** | Medium | pytest | Async HTTP, type-annotated, rich fixture setup |
| 5 | **requests** | Medium | pytest | Classic HTTP, multi-package deps, utf-8 edge cases |
| 6 | **pydantic** | Medium | pytest | Type-heavy, Pydantic V2 core, generics, validators |
| 7 | **pandas** | Large | pytest | C extensions, huge import graph, mixed src layout |
| 8 | **scikit-learn** | Large | pytest | Cython/C extensions, namespace packages, sub-packages |

---

## Report Cards

### 1. click 8.x — CLI Framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/pallets/click](https://github.com/pallets/click) |
| **Size** | ~8,200 LOC, ~45 source files |
| **Test framework** | pytest |
| **Test count** | ~2,000+ tests |
| **Coverage** | ~90% |
| **Project layout** | `src/click/` + `tests/` (src layout) |
| **Test directory** | `tests/` |

**Import patterns:**
- Absolute imports within the package
- Standard library: `os`, `sys`, `re`, `typing`, `collections`, `inspect`
- Third-party: minimal (stdlib-first design)
- Heavy use of decorators and function attributes

**Test framework details:** pytest with conftest.py fixtures, parametrize, monkeypatch

**Edge cases covered:**
- **src/ layout** (Edge 1) — tests adapter's ability to strip `src/` prefix
- **`__init__.py` re-exports** — `click/__init__.py` re-exports from submodules
- **conftest.py at multiple levels** — fixture sharing across test files

**Why selected:** Small, clean codebase with high coverage and src layout. Tests adapter's most basic functionality. Good baseline.

---

### 2. attrs 25.x — Classes Without Boilerplate

| Metric | Value |
|--------|-------|
| **Source** | [github.com/python-attrs/attrs](https://github.com/python-attrs/attrs) |
| **Size** | ~25,000 LOC, ~60 source files |
| **Test framework** | pytest |
| **Test count** | ~3,000+ tests |
| **Coverage** | ~95% |
| **Project layout** | `src/attr/` + `tests/` |
| **Test directory** | `tests/` |

**Import patterns:**
- Absolute imports
- Complex `__init__.py` — conditional imports based on Python version
- `typing` module heavily used
- Dynamic imports for optional backends (`importlib`)
- Conditional `TYPE_CHECKING` imports to avoid circular deps

**Test framework details:** pytest with extensive parametrize, hypothesis property-based tests

**Edge cases covered:**
- **Conditional imports** (Edge 13) — version-dependent imports in `__init__.py`
- **Type-checking-only imports** (Edge 19) — `TYPE_CHECKING` guards
- **Circular imports** (Edge 16) — carefully managed in `_make.py`
- **Re-exports in `__init__.py`** (Edge 17)

**Why selected:** Excellent code quality with high coverage. Tests conditional and circular import handling. Property-based tests add hypothesis coverage to the stress test.

---

### 3. structlog — Structured Logging

| Metric | Value |
|--------|-------|
| **Source** | [github.com/hynek/structlog](https://github.com/hynek/structlog) |
| **Size** | ~6,500 LOC, ~35 source files |
| **Test framework** | pytest |
| **Test count** | ~1,500+ tests |
| **Coverage** | ~95% |
| **Project layout** | `src/structlog/` + `tests/` |
| **Test directory** | `tests/` |

**Import patterns:**
- Modern Python patterns (type hints, async)
- Composition over inheritance
- Third-party integration imports (stdlib logging, json, etc.)
- Context variable usage

**Test framework details:** pytest with hypothesis, property-based tests

**Edge cases covered:**
- **Async test fixtures** — tests logging in async contexts
- **Configuration-heavy imports** — processor chains, import-time registration
- **Mixed test/non-test directories**

**Why selected:** Modern Python with async patterns, composition-heavy design. Tests adapter's handling of async code and dynamic configuration.

---

### 4. httpx 0.28.x — Modern HTTP Client

| Metric | Value |
|--------|-------|
| **Source** | [github.com/encode/httpx](https://github.com/encode/httpx) |
| **Size** | ~20,000 LOC, ~80 source files |
| **Test framework** | pytest |
| **Test count** | ~3,500+ tests |
| **Coverage** | ~85% |
| **Project layout** | `httpx/` + `tests/` |
| **Test directory** | `tests/` |

**Import patterns:**
- Absolute imports
- Heavy third-party usage: `httpcore`, `h11`, `h2`, `certifi`, `anyio`
- Conditional imports for optional HTTP/2 support
- Type-annotated throughout
- Async/await patterns

**Test framework details:** pytest with asyncio fixtures, mock servers, parametrize

**Edge cases covered:**
- **Async test functions** — tests adapter's ability to discover async tests
- **Deeply nested packages** (Edge 7) — `httpx/_transports/...` hierarchy
- **Third-party dependencies** — tests import resolution across package boundaries
- **Non-standard test file naming** — some test files with unusual names

**Why selected:** Modern async HTTP client with real-world complexity. Covers async discovery, third-party import resolution, and medium-scale project structure.

---

### 5. requests 2.x — HTTP for Humans

| Metric | Value |
|--------|-------|
| **Source** | [github.com/psf/requests](https://github.com/psf/requests) |
| **Size** | ~30,000 LOC, ~60 source files |
| **Test framework** | pytest |
| **Test count** | ~2,000+ tests |
| **Coverage** | ~80% |
| **Project layout** | `requests/` + `tests/` |
| **Test directory** | `tests/` |

**Import patterns:**
- Absolute imports
- Conditional imports for optional backends (`urllib3`, `chardet`/`charset_normalizer`)
- Version-dependent imports
- Internal package structure with sub-packages

**Test framework details:** pytest with mock servers, HTTPBin, cassette-based testing

**Edge cases covered:**
- **Conditional imports** (Edge 13) — `chardet` vs `charset_normalizer` fallback
- **Non-UTF-8 filenames** (Edge 8) — historically observed in the issue tracker
- **Multiple conftest.py** (Edge 5) — fixture organization across test subdirs
- **Utf-8 edge cases** in response handling

**Why selected:** Battle-tested classic Python library. Covers conditional dependency resolution and medium-scale import structures. Complements httpx as the "sync counterpart."

---

### 6. pydantic 2.x — Data Validation

| Metric | Value |
|--------|-------|
| **Source** | [github.com/pydantic/pydantic](https://github.com/pydantic/pydantic) |
| **Size** | ~30,000 LOC (Python), ~40,000 LOC (Rust via pydantic-core) |
| **Source files** | ~120 Python files |
| **Test framework** | pytest |
| **Test count** | ~5,000+ tests |
| **Coverage** | ~85% (Python), N/A (Rust core) |
| **Project layout** | `pydantic/` + `tests/` |
| **Test directory** | `tests/` |

**Import patterns:**
- Heavy generics and type annotation usage
- Conditional imports for Python version compatibility
- Dynamic class creation patterns
- Rust extension (`pydantic-core`) interaction
- Complex `__init__.py` with selective re-exports

**Test framework details:** pytest with extensive parametrize, hypothesis

**Edge cases covered:**
- **C extension modules** (Edge 18) — `pydantic-core` is a Rust `.so` file
- **Type-checking-only imports** (Edge 19) — heavy `TYPE_CHECKING` usage
- **Generic type resolution** — complex import chains for type evaluators
- **Dynamic imports** (Edge 14) — import-time class generation, forward references
- **`__init__.py` re-exports** (Edge 17) — public API vs internal module separation

**Why selected:** Tests adapter's handling of mixed Python/C codebases. The Rust core creates a realistic scenario where some sources are unparseable `.so` files. Heavy generic usage stresses import resolution.

---

### 7. pandas 2.x — Data Analysis Library

| Metric | Value |
|--------|-------|
| **Source** | [github.com/pandas-dev/pandas](https://github.com/pandas-dev/pandas) |
| **Size** | ~400,000+ LOC, ~1,500+ source files |
| **Test framework** | pytest |
| **Test count** | ~40,000+ tests |
| **Coverage** | ~85% |
| **Project layout** | `pandas/` + `tests/`, some `src/` sub-packages |
| **Test directory** | `tests/` (top-level and per-module) |

**Import patterns:**
- Massive import graph — hundreds of interdependencies
- Cython (`*.pyx`) and C extension modules
- Conditional imports based on platform
- Sub-packages with private module hierarchy
- Deprecated import paths for backward compatibility

**Test framework details:** pytest with extensive parametrize, hypothesis

**Edge cases covered:**
- **C extension modules** (Edge 18) — `.pyx` files and compiled `.so` files
- **Namespace packages** (Edge 3) — some sub-packages
- **Deeply nested packages** (Edge 7) — `pandas/core/indexes/...` deep hierarchies
- **Circular imports** (Edge 16) — within the core module family
- **Gigantic import graph** — stress-tests static-deps performance
- **Symlinks** (Edge 9) — in some build artifacts

**Why selected:** Maximum stress test for the adapter. Hundreds of files, thousands of imports, C extensions, and a complex dependency graph. Tests adapter's performance at scale and ability to handle mixed Python/C codebases.

---

### 8. scikit-learn 1.x — Machine Learning

| Metric | Value |
|--------|-------|
| **Source** | [github.com/scikit-learn/scikit-learn](https://github.com/scikit-learn/scikit-learn) |
| **Size** | ~150,000+ LOC, ~500+ source files |
| **Test framework** | pytest |
| **Test count** | ~20,000+ tests |
| **Coverage** | ~90% |
| **Project layout** | `sklearn/` + `tests/` (co-located: each module has its own `tests/` subdirectory) |
| **Test directory** | Per-module: `sklearn/xxx/tests/` |

**Import patterns:**
- Cython (`*.pyx`) and C extensions in several modules
- Sub-packages with diverse import patterns
- Conditional imports for optional backends
- `__init__.py` re-exports throughout
- Namespace-like hierarchy within `sklearn/`

**Test framework details:** pytest, some numpy-based test utilities

**Edge cases covered:**
- **Tests co-located with source** (Edge 2) — `sklearn/cluster/tests/` etc.
- **C extension modules** (Edge 18) — Cython in `sklearn/`, `sklearn/neighbors/`, etc.
- **Name conflict with test files** — `sklearn/` starts with `sk`, not `test_`, but modules have `tests/` subdirs
- **Multiple test directories** — tests spread across the entire tree
- **Platform-specific code** — Windows vs Unix conditional imports

**Why selected:** Complements pandas with a different test layout (co-located tests). The per-module `tests/` subdirectories stress-test discover in ways that centralized test directories don't. Large but manageable codebase.

---

## Coverage Matrix

| Edge Case | click | attrs | structlog | httpx | requests | pydantic | pandas | sklearn |
|-----------|-------|-------|-----------|-------|----------|----------|--------|---------|
| Edge 1: src/ layout | ✓ | ✓ | ✓ | | | | | |
| Edge 2: Co-located tests | | | | | | | | ✓ |
| Edge 3: Namespace packages | | | | | | | ✓ | |
| Edge 5: Multi-conftest | ✓ | | | ✓ | ✓ | ✓ | ✓ | ✓ |
| Edge 6: Non-standard naming | | | | ✓ | | | | |
| Edge 7: Deeply nested | | | | ✓ | | | ✓ | ✓ |
| Edge 8: Non-UTF-8 files | | | | | ✓ | | | |
| Edge 12: Relative imports | | ✓ | | | | | ✓ | ✓ |
| Edge 13: Conditional imports | | ✓ | | | ✓ | | ✓ | ✓ |
| Edge 14: Dynamic imports | | ✓ | | | | ✓ | | |
| Edge 15: Re-exports | ✓ | ✓ | | | | ✓ | ✓ | ✓ |
| Edge 16: Circular imports | | ✓ | | | | ✓ | ✓ | |
| Edge 17: __init__.py re-exports | ✓ | ✓ | | | | ✓ | ✓ | ✓ |
| Edge 18: C extensions | | | | | | ✓ | ✓ | ✓ |
| Edge 19: TYPE_CHECKING | | ✓ | | | | ✓ | | |
| Edge 20: Lazy imports | | | ✓ | ✓ | | | | |

## Selection Rationale

**Structural diversity:**
- 3 small, 3 medium, 2 large projects
- Both flat (`requests/` + `tests/`) and src (`src/click/` + `tests/`) layouts
- Centralized (`tests/` at root) and co-located (`sklearn/*/tests/`) test directories
- Pure Python, C extension, and Rust extension codebases
- Async (httpx, structlog) and sync (click, requests, attrs) patterns

**Test framework coverage:**
- Pytest with: parametrize, fixtures, conftest, hypothesis, mock, asyncio
- Doctest (incidental in pandas, sklearn)

**Import pattern diversity:**
- Each project stresses different import resolution paths
- Together they cover: absolute, relative, conditional, dynamic, TYPE_CHECKING, re-exports, circular

**Risk mitigation:**
- pandas and sklearn have long clone times and large storage requirements
- Small projects (click, attrs, structlog) can be tested first as a fast feedback loop
- httpx and requests test the same domain (HTTP) with different structural choices — if one works, it's likely the other will too
- pydantic's Rust core is the only mixed-language codebase; if it causes issues, consider pure-Python alternatives
