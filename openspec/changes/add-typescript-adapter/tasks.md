## 1. Crate scaffolding & tree-sitter queries
- [ ] 1.1 Create `adapter-typescript/` crate directory with `Cargo.toml` depending on `tree-sitter = "0.25"` and `tree-sitter-typescript = "0.23"`
- [ ] 1.2 Create `adapter-typescript/queries/discover.scm` — tree-sitter query matching `describe`, `it`, `test`, `describe.each`, `it.each`, `test.each` call expressions
- [ ] 1.3 Create `adapter-typescript/queries/imports.scm` — tree-sitter query matching `import ... from`, `require()`, and `import()` expressions
- [ ] 1.4 Create `adapter-typescript/queries/exports.scm` — tree-sitter query matching `export` declarations (named, default, re-export)
- [ ] 1.5 Write Rust query runner function: given a `tree_sitter::Tree`, grammar language, and query file, return captured node ranges as `(name, line, column)` tuples
- [ ] 1.6 Write unit tests for each query against realistic TypeScript source snippets including:
      - Decorators on test classes, JSDoc comments, template literals
      - Named, default, namespace, type-only, side-effect imports
      - `require()` calls with destructuring
      - Re-exports via `export { ... } from "./module"`
      - Barrel file (`index.ts`) imports
      - Dynamic `import()` expressions
      - Parameterized tests with `describe.each`, `it.each`, `test.each`
      - Wildcard re-exports (`export * from` and `export * as ns from`)
      - Circular imports between test and source files

## 2. Adapter binary
- [ ] 2.1 Create `adapter-typescript/src/main.rs` with the main JSON loop (identical structure to the Clojure adapter)
- [ ] 2.2 Implement `cmd_handshake()`: declare `languages: ["typescript"]`, `granularity: "file"`, `symbol_model_complete: false`, `fingerprinting: true`, `runtime_edges: false`
- [ ] 2.3 Implement grammar selector: return `typescript` grammar for `.ts`, `.mts`, `.cts`; `tsx` grammar for `.tsx` files
- [ ] 2.4 Implement `cmd_discover()`: scan project for `.ts`/`.tsx`/`.mts`/`.cts` files under `src/` and `test` directories, parse each with tree-sitter, run the discover query, return test items. Also discover by file-name convention (`*.test.ts`, `*.spec.ts`, `*.test.mts`, `*.test.cts`, `__tests__/`, `__test__/`). Include `.each` discovery for parameterized tests.
- [ ] 2.5 Implement `cmd_static_deps()`: for each changed file, parse with tree-sitter, run the imports + exports queries, match imports to exports across files, return edges with `weight: 1_000_000`, `origin: "static"`
- [ ] 2.6 Implement import path resolution: relative paths → file resolution with extension fallback (`.ts` → `.tsx` → `/index.ts` → `/index.tsx`), including tsconfig.json path alias resolution (`compilerOptions.paths`/`baseUrl` with comment stripping), non-code import resolution (CSS/images), and circular import safety (visited-set)
- [ ] 2.7 Implement `cmd_fingerprint()`: blake3 hash of file contents (identical to existing adapters)
- [ ] 2.8 Implement `cmd_run_args()`: detect Vitest/Jest config, build runner args for selected tests
- [ ] 2.9 Implement `cmd_ingest()`: parse JUnit XML output from Vitest or Jest, return runtime edges and per-test results (reuses JUnit parsing pattern from Python adapter)
- [ ] 2.10 Implement runner detection: probe for `vitest.config.ts`, `jest.config.ts`, and `package.json` devDependencies

## 3. Workspace integration
- [ ] 3.1 Add `adapter-typescript` to the root `Cargo.toml` workspace members
- [ ] 3.2 Pin `tree-sitter = "0.25"` and `tree-sitter-typescript = "0.23"` in `adapter-typescript/Cargo.toml`
- [ ] 3.3 Add `.ts`, `.tsx`, `.mts`, `.cts` extension mappings to `config.rs` defaults
- [ ] 3.4 Add `vitest.config.ts`, `jest.config.ts`, `package.json` (with `vitest`/`jest` in devDependencies) to `detect_project_language()` in `config.rs` → returns `"testaruda-adapter-typescript"`
- [ ] 3.5 Update `main.rs` CLI messages to mention TypeScript adapter
- [ ] 3.6 Update `testaruda.toml` default template to include `.ts`/`.tsx`/`.mts`/`.cts` adapter config

## 4. Testing
- [ ] 4.1 Create a fixture TypeScript project under `adapter-typescript/tests/fixtures/typescript/` with `vitest.config.ts`, `src/`, `tests/`, and sample `describe`/`it`/`test` tests
- [ ] 4.2 Write integration tests for the full adapter pipeline: handshake → discover → static-deps → fingerprint → run-args → ingest
- [ ] 4.3 Write seeded-fault recall test: change a source file and confirm the dependent test is selected
- [ ] 4.4 Write import resolution tests covering: relative paths, extension-less imports, barrel files, package imports, dynamic imports
- [ ] 4.5 Write runner detection tests for Vitest and Jest configs
- [ ] 4.6 Write espectacular contracts covering ADAPT-020, ADAPT-021, ADAPT-022
- [ ] 4.7 Run `ah check` to validate spec-contract correspondence

## 5. Documentation
- [ ] 5.1 Add TypeScript support section to `README.md` with setup instructions (prerequisites: `npx vitest` or `npx jest`)
- [ ] 5.2 Update `testaruda.toml` example in docs to show `.ts`/`.tsx` adapter config