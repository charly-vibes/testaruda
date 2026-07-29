## Context

testaruda's adapter protocol (TIA-ADAPT-001) is JSON-over-stdin/stdout; the core spawns one subprocess per configured file extension and holds no language-specific logic itself. The existing Rust, Python, and in-progress Clojure adapters are all Rust binaries — a pattern that the TypeScript adapter follows.

The sibling **pretender** project (at `../pretender/`) depends on `tree-sitter-typescript` for TypeScript metrics, confirming the crate compiles and works with tree-sitter 0.25.

The key difference from the Rust/Python adapters is that TypeScript imports use ES module syntax (`import ... from ...`), which has complex grammar features (default imports, named imports, namespace imports, type-only imports, side-effect imports, re-exports). Tree-sitter provides named CST nodes (`import_statement`, `import_clause`, `import_specifier`, `call_expression` for `require()`) that handle all import forms natively.

## Decision 1: Rust binary with tree-sitter (not Node.js, not regex)

**Chosen:** The adapter is a separate Rust crate (`adapter-typescript/`) in the testaruda workspace, producing `testaruda-adapter-typescript` — a Rust binary that uses `tree-sitter-typescript` to parse `.ts`/`.tsx` files and runs embedded `.scm` queries for test discovery and import extraction.

**Rejected: Node.js/Deno/Bun script.** Adding a JavaScript runtime as a hard dependency breaks the single-binary distribution model. CI must install the runtime separately, startup latency is higher per invocation, and it differs from how all existing adapters work.

**Rejected: regex-based import parser.** ES module imports can span multiple lines, have decorators, interleaved JSDoc, nested type annotations (`import { type Foo as Bar }`), inline comments, and optional chaining in `require()` calls. Regex is fragile for all of these. Tree-sitter gives a clean CST for free.

**Rejected: separate `testaruda-adapter-typescript` repository.** No technical win — the adapter is a single Rust crate with tree-sitter queries, no different from the Clojure adapter. A separate repo means two CI configs, two release cadences.

## Decision 2: tree-sitter grammar selection — TypeScript for .ts/.tsx, JavaScript for .js/.jsx

The TypeScript grammar (`tree-sitter-typescript`) supports both `.ts` and `.tsx` via two named grammars: `typescript` and `tsx`. The JavaScript grammar (`tree-sitter-javascript`) supports `.js` and `.jsx`.

For v1, the adapter targets `.ts` and `.tsx` files. JavaScript support (`.js`, `.jsx`) is deferred to a follow-up but architecturally identical — it's a grammar switch in the query factory.

The adapter SHALL use the `typescript` grammar for `.ts` files and the `tsx` grammar for `.tsx` files.

## Decision 3: tree-sitter query scope — imports, exports, and test declarations

### Import extraction query (static deps)

```scheme
; ES module imports: import ... from "module"
(import_statement
  source: (string (string_fragment) @import_source)) @import_stmt

; require() calls: const x = require("module")
(call_expression
  function: (identifier) @_require_fn
  (#eq? @_require_fn "require")
  arguments: (arguments (string (string_fragment) @require_source))) @require_call

; Dynamic import(): import("module")
(import_expression
  (string (string_fragment) @dynamic_import_source))
```

For each captured `@import_source`, `@require_source`, or `@dynamic_import_source`:
- If it starts with `./`, `../`, or `/` — it's a local relative/absolute path. The adapter SHALL resolve it to the corresponding source file path.
- If it does not start with `.` or `/` — it's a package import (e.g., `lodash`, `@angular/core`). The adapter SHALL emit it as-is; the core handles external dependencies via the fallback mechanism (TIA-SAFE-004).

### Import extraction query (static deps)

```scheme
; ES module imports: import ... from "module"
(import_statement
  source: (string (string_fragment) @import_source)) @import_stmt

; require() calls: const x = require("module")
(call_expression
  function: (identifier) @_require_fn
  (#eq? @_require_fn "require")
  arguments: (arguments (string (string_fragment) @require_source))) @require_call

; Dynamic import(): import("module")
(import_expression
  (string (string_fragment) @dynamic_import_source))
```

For each captured `@import_source`, `@require_source`, or `@dynamic_import_source`:
- If it starts with `./`, `../`, or `/` — it's a local relative/absolute path. The adapter SHALL resolve it to the corresponding source file path.
- If it does not start with `.` or `/` — it's a package import (e.g., `lodash`, `@angular/core`). The adapter SHALL emit it as-is; the core handles external dependencies via the fallback mechanism (TIA-SAFE-004).

### Test discovery query

```scheme
; describe/it/test call: describe("name", ...), it("name", ...), test("name", ...)
(call_expression
  function: (identifier) @_test_fn
  (#match? @_test_fn "^(describe|it|test)$")
  arguments: (arguments
    (string (string_fragment) @test_name) .)) @test_declaration

; describe/it/test via member expression: describe.skip("name", ...), it.only("name", ...)
; NOTE: .skip and .only are intentionally excluded — skipped/focused tests are not test items

; Parameterized test methods: describe.each(...)("name", ...), it.each(...)("name", ...), test.each(...)("name", ...)
; These return a function that is then called with the description string — so the
; outer call_expression has the string as its first (or second, after the data table) argument.
; .each produces: (call_expression function: (call_expression
;   function: (member_expression property: (property_identifier) "each")))
(call_expression
  function: (call_expression
    function: (member_expression
      object: (identifier) @_each_obj
      property: (property_identifier) @_each_method
      (#match? @_each_method "^each$"))
    arguments: (_) @_data_table)
  arguments: (arguments . (string (string_fragment) @test_name) _*) ) @test_declaration
```

The adapter SHALL also discover test files by file-name convention:
- Files matching `*.test.ts`, `*.test.tsx`, `*.spec.ts`, `*.spec.tsx`, `*.test.mts`, `*.spec.mts`, `*.test.cts`, `*.spec.cts`
- Files matching `*.test.ts`, `*.test.tsx` in `__tests__/` or `__test__/` directories
- These SHALL be returned as file-level test items when no test-declaration parsing is needed (conservative fallback).

The adapter SHALL also discover test files by file-name convention:
- Files matching `*.test.ts`, `*.test.tsx`, `*.spec.ts`, `*.spec.tsx`
- Files in `__tests__/` directories
- These SHALL be returned as file-level test items when no test-declaration parsing is needed (conservative fallback).

### Export extraction query (mapping exports to modules)

```scheme
; Exported const: export const foo = ...
(export_statement
  (lexical_declaration
    (variable_declarator name: (identifier) @export_name))) @export_decl

; Exported function: export function foo() {}
(export_statement
  (function_declaration name: (identifier) @export_name)) @export_decl

; Exported class: export class Foo {}
(export_statement
  (class_declaration name: (identifier) @export_name)) @export_decl

; Export list: export { foo, bar }
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @export_name))) @export_clause

; Default export: export default function() {} or export default class {}
(export_statement
  (export_default) @export_default)

; Wildcard re-export: export * from "./module"
(export_statement
  (wildcard_export) @wildcard_export
  source: (string (string_fragment) @re_export_source)) @re_export_stmt

; Wildcard-as re-export: export * as Utils from "./module"
(export_statement
  (named_export
    (export_specifier name: (identifier) @re_export_name)
    source: (string (string_fragment) @re_export_source)) @re_export_spec) @re_export_stmt_ns
```

Exports are used to match `import { Foo } from "./module"` to the correct source file — the adapter maps exported names back to their source file for edge construction.

Named default exports (e.g., `export default function foo() {}`) are captured by the `function_declaration` query above. Anonymous default exports (`export default class {}`) produce no named export entry and are tracked as file-level dependencies only.

## Decision 4: dependency resolution

### Direction
For each changed source file, the adapter SHALL parse its export declarations to determine which symbols it exports. It SHALL then find all test files whose `import ... from` or `require()` forms import from the changed file, and emit edges FROM those test files TO the changed source file.

### Import path resolution
The adapter SHALL resolve import paths as follows:

| Import form | Resolution | Example |
|-------------|-----------|---------|
| `"./module"` | Relative to importing file → `src/module.ts` | `import('./module'`) → same dir |
| `"../utils"` | Up one dir → `src/utils.ts` | Try extensions: `.ts`, `.tsx`, `/index.ts`, `/index.tsx` |
| `"lodash"` | Package import — emit as-is | Untracked, core handles fallback |
| `"@scope/package"` | Scoped package — emit as-is | Untracked, core handles fallback |
| `"@/components/Button"` | tsconfig path alias — **resolved if tsconfig.json is present** | See `tsconfig.json path resolution` below |

### tsconfig.json path resolution
The adapter SHALL attempt to read `tsconfig.json` (and `jsconfig.json` for JS projects) from the project root. If found, the adapter SHALL parse `compilerOptions.paths` and `compilerOptions.baseUrl` and apply them during import resolution.

Resolution order:
1. If an import matches a `paths` key prefix (e.g., `@/` → `./src/`), replace the prefix with the corresponding path entry relative to `baseUrl`.
2. Resolve the resulting path using the standard extension fallback (`.ts` → `.tsx` → `/index.ts` → `/index.tsx`).
3. If `tsconfig.json` is absent or unreadable, the adapter SHALL treat all non-relative imports (including `@`-prefixed ones) as external package imports, and SHALL emit a warning to stderr.

The adapter SHALL handle the most common `paths` patterns:
- `"@/*": ["src/*"]` (Vite/Next.js convention)
- `"~/*": ["src/*"]` (Angular convention)
- `"@components/*": ["src/components/*"]` (explicit alias)

Wildcard path mappings (`*` → `src/*`) are supported. Non-wildcard path mappings (single-file aliases like `"@utils": ["src/utils.ts"]`) are also supported.

The adapter SHALL strip comments from `tsconfig.json` before parsing (TypeScript config files allow `//` and `/* */` comments, which are invalid JSON). A simple comment-stripping pass is sufficient for v1.

### Circular import safety
When resolving re-export chains, the adapter SHALL maintain a visited-set of file paths and MUST NOT re-visit a file already in the chain. This prevents infinite loops from circular imports (`a.ts` → `b.ts` → `a.ts`).

### Non-code imports
`import "./styles.css"` or `import logo from "./logo.png"` — these are syntactically valid TypeScript imports. The adapter SHALL resolve them to the target file path (if it exists on disk) and include them as dependency edges. testaruda's core already handles non-test-file changes via the standard fallback.

### Barrel files (index.ts)
`./barrel` → `./barrel/index.ts`. The adapter SHALL handle this by trying the path as a directory before trying as a file. If the resolved path is a directory, it looks for `index.ts` or `index.tsx`.

### Type-only imports
TypeScript `import type { Foo } from "./module"` — these still produce a dependency edge. Even though the import is erased at runtime, a change to the type could affect the test at compile time.

### Re-exports (one level)
`export { Foo } from "./module"` — the adapter SHALL follow re-exports from the **immediate target** of the importing file's import path. Depth limit: one hop. A test importing from `./barrel` that re-exports from `./module` creates an edge from the test to `./module` if `./module` changed. The adapter MUST NOT follow chains longer than one hop — it SHALL stop after resolving the re-export source of the direct import target.

## Decision 5: runner detection

The adapter probes for configuration files in the project root and selects the test runner:

| Runner | Detection | Args format | Output format |
|--------|-----------|-------------|---------------|
| **Vitest** (preferred) | `vitest.config.ts`, `vitest.config.js`, or `vitest` in package.json devDependencies | `npx vitest run --reporter=junit --outputFile=<path> -- <selected>` | JUnit XML |
| **Jest** (fallback) | `jest.config.ts`, `jest.config.js`, or `jest` in package.json devDependencies | `npx jest <selected>` | Jest JSON output or JUnit |
| **Generic** (fallback) | No config detected → `npx vitest run` | `npx vitest run --reporter=junit --outputFile=<path>` | JUnit XML |

Default: **Vitest**. Vitest is the modern standard for TypeScript testing (2026), supports both ESM and CommonJS natively, and produces structured JUnit XML output. Jest support is secondary.

## Decision 6: output parsing

The adapter supports two ingestion formats:

1. **JUnit XML** (`--reporter=junit`) — primary format. Vitest produces a `testsuites`/`testcase` hierarchy with `file`, `name`, `classname`, `time` attributes. The adapter reuses the same JUnit parsing logic from the Python adapter (`parse_junit_testcase`, `has_failure_element`, `parse_junit_xml`).

2. **Vitest/Jest verbose JSON** — fallback when JUnit is unavailable. Parsed from stdout: per-test lines matching `✓`/`×` patterns or structured JSON lines.

## Decision 7: granularity is file-level for v1

The adapter declares `granularity: "file"` and `symbol_model_complete: false`. While `describe`/`it` blocks give fine-grained test declaration, the *source side* has no explicit symbol boundary that maps imports to specific exports in a type-safe way without full TypeScript type resolution (which would require `ts.Program` — the full TypeScript compiler).

The dependency edge is from a test file to a source file (file-level). Future versions could add symbol-level granularity by integrating with `ts-morph` or the TypeScript compiler's `ts.SourceFile` reference resolution.

## Risks and Trade-offs

- **tree-sitter dependency:** The adapter binary pulls in `tree-sitter` and `tree-sitter-typescript` as Rust crate dependencies. This adds ~30s to the first build and ~400KB to the binary. Mitigation: the adapter is a separate Cargo workspace member, so the core binary is unaffected.
- **tree-sitter version compatibility:** `tree-sitter-typescript = "0.23.2"` must be compatible with `tree-sitter = "0.25"`. Verified — `tree-sitter-typescript` supports `tree-sitter >= 0.20`.
- **tree-sitter build dependency:** `tree-sitter-typescript` requires a C compiler (cc/gcc) at build time to compile the embedded C grammar. This is already a dependency of testaruda's existing adapters and is standard for Rust CI.
- **Import resolution complexity:** TypeScript path resolution includes `tsconfig.json` `paths`/`baseUrl`, which the adapter does not read for v1. The adapter resolves relative imports (`.`/`..`-prefixed) by trying standard extensions. Absolute imports (non-`.`/`..`) are treated as external. This is conservative: external imports trigger the fallback (all-tests) path. A follow-up could add `tsconfig.json` path resolution.
- **Re-export chains:** The adapter resolves one level of re-exports for v1. Deeper chains may cause missed edges. This is conservative (false positives, not false negatives) because the direct import edge still exists.
- **JavaScript support deferred:** `.js`/`.jsx` files are registered but not tested in v1. The tree-sitter grammar switch is straightforward, but the test framework landscape for plain JS differs (more Mocha/Cypress usage). JS support is deferred to a follow-up.
- **Dynamic imports (`import()`)**: The adapter captures dynamic imports as static dependencies for v1. This is conservative — the dependency edge exists even if the dynamic import is conditional.
- **CSS/image imports:** `import "./styles.css"` or `import logo from "./logo.png"` — these are non-code dependencies. The adapter SHALL include them as edges (fingerprint changes still matter for rebuilds), but testaruda's core already handles non-test-file changes via the standard fallback.

## Deferred (not needed for v1)

- **JavaScript (.js/.jsx) support** — architecturally identical (switch grammar to `tree-sitter-javascript`), but test framework diversity for JS (Mocha, Cypress, Ava) warrants separate validation.
- **Symbol-level granularity** — requires TypeScript compiler integration to map imports to specific exported bindings.
- **Monorepo workspace support** (pnpm workspaces, turborepo, Nx) — complex path resolution across packages. Conservative v1: each workspace package configured as a separate testaruda adapter.
- **SWC/esbuild transforms** — not relevant for static analysis; the adapter analyzes source files directly, not compiled output.
- **Next.js, Remix, Nuxt framework detection** — deferred. Generic Vitest detection is sufficient for v1.
- **`describe.only`/`it.only`/`describe.skip`/`it.skip` test variants** — intentionally excluded. Focused/skipped tests are not test items. Only unqualified `describe`/`it`/`test` calls are discovered.