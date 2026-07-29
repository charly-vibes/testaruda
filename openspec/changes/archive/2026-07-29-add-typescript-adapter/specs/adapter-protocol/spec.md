## ADDED Requirements

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