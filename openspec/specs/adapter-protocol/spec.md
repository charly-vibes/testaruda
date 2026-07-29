# Adapter Protocol

## Purpose

Language/framework adapter interface — JSON request/response protocol over stdin/stdout with handshake, discovery, dependency analysis, and ingestion commands.

## Requirements

### Requirement: TIA-ADAPT-001 — JSON protocol

The core SHALL communicate with adapters using JSON request/response over standard input and output, with diagnostics on standard error and status via exit code.

#### Scenario: Adapter communication
- **GIVEN** the core and an adapter process
- **WHEN** the core sends a command
- **THEN** the adapter SHALL receive JSON on stdin
- **AND** SHALL respond with JSON on stdout
- **AND** SHALL emit diagnostics on stderr
- **AND** SHALL indicate status via exit code

### Requirement: TIA-ADAPT-002 — Adapter handshake

When the core starts an adapter, the adapter SHALL return a handshake declaring its name, version, supported protocol version, languages, granularity, and capability flags. Capability flags SHALL include at minimum: `symbol_model_complete` (boolean — used by TIA-CHG-004 to permit sub-file granularity narrowing).

#### Scenario: Handshake response
- **GIVEN** an adapter is started
- **WHEN** the core requests its capabilities
- **THEN** the adapter SHALL return name, version, protocol version, languages, granularity, and capability flags
- **AND** `symbol_model_complete` SHALL be included among the capability flags

### Requirement: TIA-ADAPT-003 — Required commands

An adapter SHALL implement the commands `discover`, `static-deps`, `fingerprint`, `run-args`, and `ingest`.

#### Scenario: Required command set
- **GIVEN** an adapter implementation
- **WHEN** the core sends any of the required commands
- **THEN** the adapter SHALL respond appropriately for `discover`, `static-deps`, `fingerprint`, `run-args`, and `ingest`

### Requirement: TIA-ADAPT-004 — Discover command

When invoked with `discover`, an adapter SHALL enumerate test items in scope with their node id, suite kind, and file.

#### Scenario: Test discovery
- **GIVEN** an adapter with access to test files
- **WHEN** the `discover` command is invoked
- **THEN** it SHALL return test items with node id, suite kind, and file path

### Requirement: TIA-ADAPT-005 — Static-deps command

When invoked with `static-deps` and a changed-file set, an adapter SHALL return candidate test items, K-valued edges, and a list of files it could not resolve. When `symbol_model_complete` is `true`, the adapter SHALL also return per-symbol edges; otherwise the core treats all edges as file-level (TIA-CHG-004).

#### Scenario: Static dependency analysis
- **GIVEN** a changed-file set
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return candidate tests, K-valued edges, and unresolved files
- **AND** if `symbol_model_complete` is true, SHALL also return per-symbol edges

### Requirement: TIA-ADAPT-006 — Fingerprint command

When invoked with `fingerprint`, an adapter SHALL return content fingerprints at its declared granularity.

#### Scenario: Content fingerprinting
- **GIVEN** files or symbols to fingerprint
- **WHEN** the `fingerprint` command is invoked
- **THEN** the adapter SHALL return content fingerprints at its declared granularity

### Requirement: TIA-ADAPT-007 — Run-args command

When invoked with `run-args` and a selected set, an adapter SHALL return the native runner arguments and a collection path, and SHALL NOT execute the tests.

#### Scenario: Run arguments generation
- **GIVEN** a selected set of tests
- **WHEN** the `run-args` command is invoked
- **THEN** the adapter SHALL return native runner arguments and a collection path
- **AND** SHALL NOT execute the tests

### Requirement: TIA-ADAPT-008 — Ingest command

When invoked with `ingest` and a run's output, an adapter SHALL return runtime edges, per-test results, and observed external inputs.

#### Scenario: Run output ingestion
- **GIVEN** a test run's output
- **WHEN** the `ingest` command is invoked
- **THEN** the adapter SHALL return runtime edges, per-test results, and observed external inputs

### Requirement: TIA-ADAPT-009 — Semiring edge values

An adapter SHALL emit dependency edges as semiring values, defaulting to the multiplicative identity where it has no finer weight.

#### Scenario: Default semiring weight
- **GIVEN** a dependency edge with no specific weight
- **WHEN** the adapter emits it
- **THEN** it SHALL use the multiplicative identity as the default weight

### Requirement: TIA-ADAPT-010 — Graceful degradation

If an adapter does not declare a capability, then the core SHALL degrade gracefully for that capability rather than fail, applying conservative defaults.

#### Scenario: Missing capability
- **GIVEN** an adapter that does not declare a capability
- **WHEN** the core needs that capability
- **THEN** the core SHALL NOT fail
- **AND** SHALL apply conservative defaults instead

### Requirement: TIA-ADAPT-011 — Protocol incompatibility

If an adapter's protocol version is incompatible with the core, then the core SHALL refuse to use it and report the mismatch.

#### Scenario: Version mismatch
- **GIVEN** an adapter with an incompatible protocol version
- **WHEN** the core attempts to use it
- **THEN** the core SHALL refuse to use it
- **AND** SHALL report the version mismatch

### Requirement: TIA-ADAPT-012 — Adapter failure fallback

If an adapter fails, times out, or returns malformed output, then the core SHALL fall back to selecting all tests in the affected component and record the failure. The core SHALL handle pre-spawn failures (adapter binary not found, command not found) and startup failures (adapter crashes before responding) identically.

#### Scenario: Adapter timeout
- **GIVEN** an adapter that times out
- **WHEN** the core is waiting for a response
- **THEN** the core SHALL fall back to selecting all tests in the affected component
- **AND** SHALL record the adapter failure

#### Scenario: Adapter binary not found
- **GIVEN** the adapter binary is not installed or not found at the configured path
- **WHEN** the core attempts to spawn the adapter
- **THEN** the core SHALL report the missing binary
- **AND** SHALL fall back to selecting all tests
- **AND** SHALL record the adapter failure

### Requirement: TIA-ADAPT-013 — Least privilege and timeout

The core SHALL invoke adapters with least privilege and a configurable timeout.

#### Scenario: Adapter sandboxing
- **GIVEN** an adapter process
- **WHEN** it is invoked
- **THEN** it SHALL be run with least privilege
- **AND** a configurable timeout SHALL be enforced

### Requirement: TIA-ADAPT-017 — Clojure adapter handshake (static deps)

A Clojure adapter SHALL declare `"clojure"` as one of its supported languages
in the handshake. The adapter SHALL declare `symbol_model_complete: false` and
`runtime_edges: false` in its handshake capabilities. The adapter SHALL declare
`granularity: "file"`.

The adapter SHALL be a Rust binary in the testaruda workspace (not a separate
repository), as a separate crate named `adapter-clojure` producing the binary
`testaruda-adapter-clojure`. The `testaruda.toml` adapter config SHALL map
`.clj` and `.cljs` extensions to this binary.

#### Scenario: Clojure handshake with static deps
- **GIVEN** a Clojure adapter binary is spawned
- **WHEN** the handshake command is invoked
- **THEN** the adapter SHALL include `"clojure"` in its `languages` array
- **AND** SHALL declare `symbol_model_complete: false`
- **AND** SHALL declare `runtime_edges: false`
- **AND** SHALL declare `granularity: "file"`

#### Scenario: Missing Clojure CLI or Leiningen
- **GIVEN** a system with the adapter binary installed but without Clojure
  CLI (`clojure`) or Leiningen (`lein`), and the adapter cannot detect a
  supported runner
- **WHEN** the `run-args` command is invoked
- **THEN** the adapter SHALL return an error

### Requirement: TIA-ADAPT-018 — Clojure discover scope (deftest-based)

A Clojure adapter SHALL discover test items by walking the project's test
directories (as declared in `deps.edn` `:test-paths`, `project.clj`
`:test-paths`, or defaulting to `test/`) and scanning `.clj` files for
`deftest` and `deftest-` top-level forms. Each `deftest` SHALL be one test
item.

Node IDs SHALL follow the format `<namespace>`, derived from the file's `ns`
declaration (or, where absent, the file path relative to the source root).
The namespace-level ID is used because the Cognitect test runner cannot run
individual `deftest` forms — it runs all tests in a namespace.

The adapter SHALL discover `deftest` forms in all `.clj` files under the
test paths, including nested subdirectories. The adapter SHALL NOT discover
tests in directories outside the declared test paths.

#### Scenario: Discover Clojure tests from deftest forms
- **GIVEN** a Clojure project with test files containing `deftest` forms
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return test items
- **AND** each item SHALL be derived from one `deftest` or `deftest-` form
- **AND** node IDs SHALL follow the format `<namespace>`
- **AND** suite kind SHALL be `"clojure.test"`

#### Scenario: No test directory
- **GIVEN** a Clojure project with no test directory
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return an empty items list

#### Scenario: deftest in non-test directory
- **GIVEN** a Clojure project with `deftest` forms in both `src/` and `test/`
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL only discover tests under the declared test paths
- **AND** SHALL NOT discover `deftest` forms under `src/`

#### Scenario: Nested test directories
- **GIVEN** a Clojure project with tests in `test/unit/` and `test/integration/`
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL discover tests in both subdirectories
- **AND** each test item SHALL include the correct file path

### Requirement: TIA-ADAPT-019 — Clojure dependency analysis (require-based static deps)

A Clojure adapter SHALL build dependency edges by parsing `(:require ...)`
and `(:use ...)` forms from the `ns` declaration of each changed source file.
Each uniquely required namespace SHALL produce a dependency edge from the
importing test to the source file.

The adapter SHALL NOT resolve aliases, `:refer` lists, or `:rename` mappings
for dependency edge construction — all required namespaces are treated as
file-level dependencies. The adapter SHALL NOT create dependency edges for
`(:import ...)` entries — changes to Java imports are captured via the
existing edge from the test to the source file.

The adapter SHALL emit edges at file-level granularity with `origin: "static"`.

#### Scenario: Static dependency from require
- **GIVEN** a changed source file `src/my_project/core.clj` with namespace
  `my-project.core`
- **AND** a test file `test/my_project/core_test.clj` with `(:require [my-project.core :as sut])`
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/my_project/core.clj"]`
- **THEN** the adapter SHALL return an edge from the test item to `src/my_project/core.clj`
- **AND** the edge SHALL have `origin: "static"`

#### Scenario: Multiple requires produce multiple edges
- **GIVEN** a test file that requires two different namespaces
- **WHEN** the `static-deps` command is invoked with changes to both
- **THEN** the adapter SHALL return edges from the test to each changed file

#### Scenario: Deduplicate edges for same namespace
- **GIVEN** a test file with two separate require entries for the same namespace
  (e.g., `(:require [my-project.core :as core] [my-project.core :as c])`)
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL emit a single edge for that namespace

#### Scenario: Changed source file with no test coverage
- **GIVEN** a changed source file whose namespace is not required by any test
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL include the source file in the `unresolved` list
- **AND** the core SHALL apply the fallback mechanism (TIA-SAFE-004)

#### Scenario: Non-Clojure file silently ignored
- **GIVEN** a changed file that is not a `.clj`, `.cljs`, or `.cljc` file
  (e.g. `.edn`, `.md`)
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL NOT include the file in its response
- **AND** the core SHALL handle the unmodeled file via existing fallback mechanisms

#### Scenario: Dependency on Java interop
- **GIVEN** a source file with `(:import java.util.Date)` in its `ns` form
- **AND** a test depending on that source file via `:require`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL NOT create a separate edge for `java.util.Date`
- **AND** the existing edge from the test to the source file SHALL remain

### Requirement: TIA-ADAPT-020 — TypeScript adapter handshake (tree-sitter deps)

A TypeScript adapter SHALL declare `"typescript"` as one of its supported languages in the handshake. The adapter SHALL declare `symbol_model_complete: false` and `runtime_edges: false` in its handshake capabilities. The adapter SHALL declare `granularity: "file"`.

The adapter SHALL be a Rust binary in the testaruda workspace (not a separate repository), as a separate crate named `adapter-typescript` producing the binary `testaruda-adapter-typescript`. The `testaruda.toml` adapter config SHALL map `.ts`, `.tsx`, `.mts`, and `.cts` extensions to this binary.

The adapter SHALL support both TypeScript and TSX syntax. For `.ts` files, the adapter SHALL use the `tree-sitter-typescript` `typescript` grammar. For `.tsx` files, the adapter SHALL use the `tree-sitter-typescript` `tsx` grammar. For `.mts` and `.cts` files, the adapter SHALL use the `typescript` grammar (same as `.ts`).

#### Scenario: TypeScript handshake with static deps
- **GIVEN** a TypeScript adapter binary is spawned
- **WHEN** the handshake command is invoked
- **THEN** the adapter SHALL include `"typescript"` in its `languages` array
- **AND** SHALL declare `symbol_model_complete: false`
- **AND** SHALL declare `runtime_edges: false`
- **AND** SHALL declare `granularity: "file"`

#### Scenario: Missing test runner
- **GIVEN** a system with the adapter binary installed but without Vitest or Jest available (no `npx`, no local `node_modules/.bin/vitest` or `jest`)
- **WHEN** the `run-args` command is invoked
- **THEN** the adapter SHALL return an error indicating no supported runner was found

#### Scenario: TSX file uses tsx grammar
- **GIVEN** a `.tsx` file with JSX syntax
- **WHEN** the adapter parses it via tree-sitter
- **THEN** the adapter SHALL use the `tsx` grammar from `tree-sitter-typescript`
- **AND** SHALL correctly parse JSX elements, fragments, and expressions

#### Scenario: MTS/CTS files use typescript grammar
- **GIVEN** a `.mts` or `.cts` file
- **WHEN** the adapter parses it via tree-sitter
- **THEN** the adapter SHALL use the `typescript` grammar (same as `.ts`)
- **AND** SHALL correctly parse the file

### Requirement: TIA-ADAPT-021 — TypeScript discover scope (describe/it/test + file conventions)

A TypeScript adapter SHALL discover test items by scanning the project for `.ts`, `.tsx`, `.mts`, and `.cts` files and applying two strategies:

1. **Declaration-based discovery:** Parse each file with tree-sitter and match `describe`, `it`, `test`, `describe.each`, `it.each`, and `test.each` call expressions using tree-sitter queries. Each test-function call with a string literal first argument SHALL produce one test item. For `.each` calls, the string argument is at position 2 (after the data table).

2. **File-convention discovery:** Any file matching `*.test.ts`, `*.test.tsx`, `*.spec.ts`, `*.spec.tsx`, `*.test.mts`, `*.spec.mts`, `*.test.cts`, `*.spec.cts`, or located in a `__tests__/` or `__test__/` directory SHALL produce a file-level test item, even if no test declarations are found in it.

Node IDs SHALL follow the format `<file-path>::<test-name>`, where `<file-path>` is relative to the project root. For nested `describe` blocks, `<test-name>` SHALL be the `::`-separated chain of description strings from outermost to innermost (e.g., `file::Calculator::adds two numbers` for `describe("Calculator", () => { it("adds two numbers", ...) })`). For file-convention test items, the node ID SHALL be the file path alone.

Suite kind: the adapter SHALL always return `suite_kind: "unit"` for all discovered test items.

The adapter SHALL discover test files under the `src/` and `test/` directories (or their equivalents), including nested subdirectories. The adapter SHALL NOT discover files in `node_modules/`, `dist/`, `build/`, `.next/`, or `.git/`.

#### Scenario: Discover Vitest tests from describe/it blocks
- **GIVEN** a TypeScript project with a file `tests/calculator.test.ts` containing:
  ```typescript
  describe("Calculator", () => {
    it("adds two numbers", () => { expect(1+1).toBe(2); });
  });
  ```
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return two test items:
  - `tests/calculator.test.ts::Calculator` (from `describe`)
  - `tests/calculator.test.ts::Calculator::adds two numbers` (from `it` nested in `describe`)
- **AND** each item SHALL have `suite_kind: "unit"`
- **AND** each item SHALL have the correct `file` path

#### Scenario: Discover test from top-level test() call
- **GIVEN** a file `src/utils.test.ts` with `test("isEven returns true for even numbers", () => { ... })`
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return a test item with node ID `src/utils.test.ts::isEven returns true for even numbers`

#### Scenario: Discover parameterized tests with describe.each
- **GIVEN** a file `tests/calculator.test.ts` with:
  ```typescript
  describe.each([1, 2, 3])("value %i", (n) => {
    it(`works with ${n}`, () => {});
  });
  ```
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return a test item with node ID containing `value %i` (from `describe.each`)
- **AND** SHALL return test items for each `it` block nested inside

#### Scenario: Discover parameterized tests with test.each
- **GIVEN** a file `tests/utils.test.ts` with `test.each([{a:1,b:2,expected:3}])("$a + $b = $expected", ({a,b,expected}) => {})`
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return a test item with node ID containing `$a + $b = $expected`

#### Scenario: File-convention discovery with no test declarations
- **GIVEN** a file `__tests__/integration.ts` that imports and calls test functions dynamically but has no `describe`/`it`/`test` declarations
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL return a file-level test item with the file path as the node ID

#### Scenario: Exclude node_modules
- **GIVEN** a TypeScript project with a `node_modules` directory containing `.test.ts` files
- **WHEN** the `discover` command is invoked
- **THEN** the adapter SHALL NOT return any test items from `node_modules`
- **AND** SHALL NOT discover tests from `dist/`, `build/`, or `.git/`

### Requirement: TIA-ADAPT-022 — TypeScript dependency analysis (import-based static deps)

A TypeScript adapter SHALL build dependency edges by parsing export declarations from changed source files and import declarations from test files using tree-sitter queries. For each changed source file, the adapter SHALL:
1. Parse its export declarations to determine which symbols it exports
2. Scan all discovered test files for import declarations (ES `import ... from`, CommonJS `require()`, dynamic `import()`) that reference the changed file's path
3. For each matching test-to-source pair, emit a dependency edge from the test item to the source file

Each uniquely resolved source file that is imported by a test SHALL produce a dependency edge from the test item to the source file.

**Import path resolution:** The adapter SHALL resolve relative import paths (starting with `./`, `../`, or `/`) to file paths by:
1. Removing the leading `./` or `../` prefix and computing the canonical path relative to the importing file
2. Appending the file extension if absent — trying in order: `.ts`, `.tsx`, `/index.ts`, `/index.tsx`
3. Replacing hyphens with underscores is NOT needed for TypeScript (unlike Clojure) — TypeScript file names match import paths directly

Non-relative imports (e.g., `lodash`, `@angular/core`) SHALL be treated as external package imports, emitted as-is without file-resolution attempts.

**tsconfig.json path resolution:** The adapter SHALL read `tsconfig.json` (if present) and apply `compilerOptions.paths` and `compilerOptions.baseUrl` to resolve path-alias imports (e.g., `@/components/Button` → `src/components/Button.ts`). The adapter SHALL strip JSON comments before parsing. If `tsconfig.json` is absent, path-aliased imports SHALL be treated as external and SHALL NOT produce dependency edges; the adapter SHALL emit a warning to stderr.

**Non-code imports:** `import "./styles.css"` or `import logo from "./logo.png"` — the adapter SHALL resolve these to their target file paths (if the files exist on disk) and include them as dependency edges.

**Circular import safety:** When resolving re-export chains, the adapter MUST maintain a visited-set of file paths and MUST NOT re-visit a file already in the chain, preventing infinite loops from circular imports.

Re-exports (`export { X } from "./module"`) SHALL be followed for **one hop only**: the adapter resolves the immediate re-export source of the direct import target. Deeper chains are not followed. Wildcard re-exports (`export * from "./module"`) are treated the same way — one hop of resolution.

The adapter SHALL emit edges at file-level granularity with `origin: "static"`.

#### Scenario: Static dependency from ES import
- **GIVEN** a changed source file `src/calculator.ts` exporting a `Calculator` class
- **AND** a test file `tests/calculator.test.ts` with `import { Calculator } from "../src/calculator"`
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/calculator.ts"]`
- **THEN** the adapter SHALL parse `src/calculator.ts` for export declarations
- **AND** SHALL scan test files for import declarations referencing `../src/calculator`
- **AND** SHALL return an edge from the test item to `src/calculator.ts`
- **AND** the edge SHALL have `origin: "static"`
- **AND** the edge SHALL have `weight: 1000000`

#### Scenario: Static dependency from require() call
- **GIVEN** a changed source file `src/utils.ts`
- **AND** a test file `tests/utils.test.ts` with `const { helper } = require("../src/utils")`
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/utils.ts"]`
- **THEN** the adapter SHALL parse `src/utils.ts` for exports
- **AND** scan test imports for `require("../src/utils")`
- **AND** SHALL return an edge from the test item to `src/utils.ts`

#### Scenario: Multiple imports produce multiple edges
- **GIVEN** a test file that imports from two different source modules
- **WHEN** the `static-deps` command is invoked with changes to both
- **THEN** the adapter SHALL return edges from the test to each changed file

#### Scenario: Extension-less import resolution
- **GIVEN** a changed file `src/models/user.ts`
- **AND** a test that imports `import { User } from "../src/models/user"` (no `.ts` extension)
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL resolve `../src/models/user` to `src/models/user.ts`
- **AND** SHALL return an edge from the test to `src/models/user.ts`

#### Scenario: Barrel file import resolution (index.ts)
- **GIVEN** a barrel file `src/models/index.ts` that re-exports from `src/models/user.ts`
- **AND** a test that imports `import { User } from "../src/models"` (resolves to `src/models/index.ts`)
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/models/user.ts"]`
- **THEN** the adapter SHALL follow the re-export and return an edge from the test to `src/models/user.ts`

#### Scenario: Re-export chain (one level)
- **GIVEN** a file `src/core.ts` exporting a function
- **AND** a barrel file `src/index.ts` with `export { coreFn } from "./core"`
- **AND** a test that imports `import { coreFn } from "../src"` (resolves to `src/index.ts`)
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/core.ts"]`
- **THEN** the adapter SHALL follow the re-export
- **AND** SHALL return an edge from the test to `src/core.ts`

#### Scenario: Non-relative (package) import emitted as-is
- **GIVEN** a test file with `import { Component } from "@angular/core"`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL NOT resolve `@angular/core` to a file path
- **AND** the import SHALL NOT produce a local dependency edge

#### Scenario: Changed source file with no test coverage
- **GIVEN** a changed source file that no test imports
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL include the source file in the `unresolved` list
- **AND** the core SHALL apply the fallback mechanism (TIA-SAFE-004)

#### Scenario: Type-only imports produce edges
- **GIVEN** a source file `src/types.ts` with type definitions
- **AND** a test file with `import type { MyType } from "../src/types"`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return an edge from the test to `src/types.ts`
- **AND** the edge SHALL have `origin: "static"`
- **AND** `weight: 1000000`

#### Scenario: Dynamic import() produces edge
- **GIVEN** a test file with `const mod = await import("../src/dynamic_module")`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL include a static edge from the test to `src/dynamic_module.ts`

#### Scenario: tsconfig.json path alias resolution
- **GIVEN** a project with `tsconfig.json` containing `"paths": {"@/*": ["src/*"]}` and `"baseUrl": "."`
- **AND** a changed source file `src/components/Button.ts`
- **AND** a test file with `import { Button } from "@/components/Button"`
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/components/Button.ts"]`
- **THEN** the adapter SHALL resolve `@/components/Button` to `src/components/Button.ts` using `tsconfig.json` paths
- **AND** SHALL return an edge from the test to `src/components/Button.ts`

#### Scenario: tsconfig.json absent — path-alias imports produce warning
- **GIVEN** a project with NO `tsconfig.json`
- **AND** a test file with `import { X } from "@/components/X"`
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL NOT resolve `@/components/X` to a file path
- **AND** the adapter SHALL emit a warning to stderr indicating that `tsconfig.json` was not found and path-alias resolution is disabled
- **AND** the import SHALL NOT produce a local dependency edge

#### Scenario: Non-code import (CSS) produces edge
- **GIVEN** a source file `src/styles.css`
- **AND** a test file with `import "./styles.css"`
- **WHEN** the `static-deps` command is invoked with `changed_files: ["src/styles.css"]`
- **THEN** the adapter SHALL resolve `./styles.css` to `src/styles.css`
- **AND** SHALL return an edge from the test to `src/styles.css`
- **AND** the edge SHALL have `origin: "static"`

#### Scenario: Non-TypeScript file silently ignored
- **GIVEN** a changed file that is not a `.ts`, `.tsx`, `.mts`, `.cts` file (e.g. `.json`, `.md`)
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL NOT include the file in its response
- **AND** the core SHALL handle the unmodeled file via existing fallback mechanisms

#### Scenario: TypeScript adapter does not deduplicate identical import paths
- **GIVEN** a changed source file
- **AND** two different test files that both import from it
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL return two edges (one per importing test)
- **AND** each edge SHALL reference the correct test node ID

#### Scenario: Circular re-export chain handled safely
- **GIVEN** file `a.ts` re-exports from `b.ts`, and `b.ts` re-exports from `a.ts` (circular)
- **WHEN** the adapter resolves re-exports during `static-deps`
- **THEN** the adapter SHALL NOT loop infinitely
- **AND** SHALL produce at most one edge per unique test-to-source pair

#### Scenario: Wildcard re-export from barrel produces edge
- **GIVEN** a barrel file `src/index.ts` with `export * from "./core"`
- **AND** a test importing `import { coreFn } from "../src"`
- **AND** `src/core.ts` is changed
- **WHEN** the `static-deps` command is invoked
- **THEN** the adapter SHALL follow the wildcard re-export
- **AND** SHALL return an edge from the test to `src/core.ts`

