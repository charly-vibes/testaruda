## 1. Crate scaffolding & tree-sitter queries
- [x] 1.1 Create `adapter-typescript/` crate directory with `Cargo.toml` depending on `tree-sitter = "0.25"` and `tree-sitter-typescript = "0.23"` (closed: testaruda-xq1, testaruda-l33)
- [x] 1.2 Create `adapter-typescript/queries/discover.scm` — tree-sitter query matching `describe`, `it`, `test`, `describe.each`, `it.each`, `test.each` call expressions (closed: testaruda-83s)
- [x] 1.3 Create `adapter-typescript/queries/imports.scm` — tree-sitter query matching `import ... from`, `require()`, and `import()` expressions (closed: testaruda-83s)
- [x] 1.4 Create `adapter-typescript/queries/exports.scm` — tree-sitter query matching `export` declarations (named, default, re-export) (closed: testaruda-83s)
- [x] 1.5 Write Rust query runner function: given a `tree_sitter::Tree`, grammar language, and query file, return captured node ranges as `(name, line, column)` tuples (closed: testaruda-ts8)
- [x] 1.6 Write unit tests for each query against realistic TypeScript source snippets including decorators, imports, exports, parameters, etc. (closed: testaruda-ts8)

## 2. Adapter binary
- [x] 2.1 Create `adapter-typescript/src/main.rs` with the main JSON loop (closed: testaruda-l33)
- [x] 2.2 Implement `cmd_handshake()`: declare `languages: ["typescript"]`, `granularity: "file"`, `symbol_model_complete: false`, `fingerprinting: true`, `runtime_edges: false` (closed: testaruda-l33)
- [x] 2.3 Implement grammar selector: return `typescript` grammar for `.ts`, `.mts`, `.cts`; `tsx` grammar for `.tsx` files (closed: testaruda-l33)
- [x] 2.4 Implement `cmd_discover()`: scan project for `.ts`/`.tsx`/`.mts`/`.cts` files, parse with tree-sitter, run discover query (closed: testaruda-9h5)
- [x] 2.5 Implement `cmd_static_deps()`: parse changed files, run imports + exports queries, return edges (closed: testaruda-byx)
- [x] 2.6 Implement import path resolution: extension fallback, tsconfig.json path aliases, circular safety (closed: testaruda-t44)
- [x] 2.7 Implement `cmd_fingerprint()`: blake3 hash of file contents (closed: testaruda-adv)
- [x] 2.8 Implement `cmd_run_args()`: detect Vitest/Jest config, build runner args (closed: testaruda-adv)
- [x] 2.9 Implement `cmd_ingest()`: parse JUnit XML from Vitest/Jest, return edges and results (closed: testaruda-xy2)
- [x] 2.10 Implement runner detection: probe for vitest/jest config files (closed: testaruda-xy2)

## 3. Workspace integration
- [x] 3.1 Add `adapter-typescript` to the root `Cargo.toml` workspace members (closed: testaruda-xq1)
- [x] 3.2 Pin `tree-sitter = "0.25"` and `tree-sitter-typescript = "0.23"` (closed: testaruda-xq1)
- [x] 3.3 Add `.ts`, `.tsx`, `.mts`, `.cts` extension mappings to `config.rs` defaults (closed: testaruda-u8i)
- [x] 3.4 Add Vitest/Jest detection to `detect_project_language()` (closed: testaruda-u8i)
- [x] 3.5 Update `main.rs` CLI messages to mention TypeScript adapter (closed: testaruda-u8i)
- [x] 3.6 Update `testaruda.toml` default template for `.ts`/`.tsx` extension mappings (closed: testaruda-u8i)

## 4. Testing
- [x] 4.1 Create fixture TypeScript project with vitest config and sample tests (closed: testaruda-a9r)
- [x] 4.2 Write integration tests for full adapter pipeline (closed: testaruda-a9r)
- [x] 4.3 Write seeded-fault recall test (closed: testaruda-a9r)
- [x] 4.4 Write import resolution tests (closed: testaruda-t44)
- [x] 4.5 Write runner detection tests for Vitest and Jest configs (closed: testaruda-xy2)
- [x] 4.6 Write espectacular contracts covering ADAPT-020, ADAPT-021, ADAPT-022 (closed: testaruda-2jp)
- [x] 4.7 Run `ah check` to validate spec-contract correspondence (closed: testaruda-2jp)

## 5. Documentation
- [x] 5.1 Add TypeScript support section to `README.md` with setup instructions (closed: testaruda-0ay)
- [x] 5.2 Update `testaruda.toml` example in docs (closed: testaruda-0ay)