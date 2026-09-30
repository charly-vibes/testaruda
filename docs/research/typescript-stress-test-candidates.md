# TypeScript Stress-Test Candidate Selection

**Date:** 2026-07-27  
**Status:** Draft (adapter not yet built — see testaruda-xq1)

## Selection Criteria

| Criterion | Requirement |
|-----------|-------------|
| Coverage | Existing test suite (jest, mocha, vitest, etc.) |
| Import diversity | ES modules, CommonJS, dynamic imports, type imports |
| Size range | Small (<50 source files), medium (50–500), large (500+) |
| Git history | Active maintenance, real commits |
| Project structure | package.json with standard test layout |
| Test framework | vitest, jest, mocha, ava, or node:test |

## Selected Projects (6 total)

| # | Project | Size | Test framework | Key characteristics |
|---|---------|------|----------------|---------------------|
| 1 | **chalk** | Small | ava | Terminal styling, zero-dependency |
| 2 | **zod** | Small | vitest | Schema validation, type-heavy |
| 3 | **express** | Medium | mocha | Web framework, middleware pattern |
| 4 | **prettier** | Medium | jest | Code formatter, large test suite |
| 5 | **next.js** | Large | jest + playwright | React framework, monorepo |
| 6 | **typescript** | Large | jest | Compiler, massive codebase |

---

## Report Cards

### 1. chalk — Terminal string styling

| Metric | Value |
|--------|-------|
| **Source** | [github.com/chalk/chalk](https://github.com/chalk/chalk) |
| **Size** | ~1,000 LOC, ~10 source files |
| **Test framework** | ava |
| **Project layout** | ESM package |
| **Import patterns** | ES module imports, zero external deps |

**Why selected:** Minimal baseline. Fast clone, no external dependencies.

---

### 2. zod — TypeScript-first schema validation

| Metric | Value |
|--------|-------|
| **Source** | [github.com/colinhacks/zod](https://github.com/colinhacks/zod) |
| **Size** | ~10,000 LOC, ~30 source files |
| **Test framework** | vitest |
| **Import patterns** | ES modules, type imports (`import type`), generic-heavy |

**Why selected:** Modern TypeScript with heavy generic usage and type-only imports. Tests adapter's TypeScript-specific import detection.

---

### 3. express — Web framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/expressjs/express](https://github.com/expressjs/express) |
| **Size** | ~20,000 LOC, ~60 source files |
| **Test framework** | mocha |
| **Import patterns** | CommonJS (`require`), middleware pattern, conditional requires |

**Why selected:** Classic Node.js project with CommonJS imports. Tests adapter's CommonJS `require()` detection.

---

### 4. prettier — Opinionated code formatter

| Metric | Value |
|--------|-------|
| **Source** | [github.com/prettier/prettier](https://github.com/prettier/prettier) |
| **Size** | ~80,000 LOC, ~200 source files |
| **Test framework** | jest |
| **Import patterns** | ES modules, dynamic imports, internal plugin architecture |

**Why selected:** Medium-large codebase with real-world import complexity. Plugin architecture creates interesting dependency patterns.

---

### 5. next.js — React framework

| Metric | Value |
|--------|-------|
| **Source** | [github.com/vercel/next.js](https://github.com/vercel/next.js) |
| **Size** | ~500,000 LOC, ~1,000+ source files |
| **Test framework** | jest + playwright |
| **Import patterns** | Monorepo with dozens of packages, complex internal deps, SWC bindings |

**Why selected:** Maximum scale for TypeScript adapter stress testing. Monorepo structure with cross-package dependencies.

---

### 6. TypeScript compiler

| Metric | Value |
|--------|-------|
| **Source** | [github.com/microsoft/TypeScript](https://github.com/microsoft/TypeScript) |
| **Size** | ~500,000+ LOC, ~1,000+ source files |
| **Test framework** | jest + mocha |
| **Import patterns** | Deep import chains, internal compiler APIs, performance-sensitive |

**Why selected:** Large, performance-sensitive codebase. Tests adapter at extreme scale.