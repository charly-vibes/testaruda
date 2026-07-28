## 1. Crate scaffolding & tree-sitter queries
- [x] 1.1 Create `adapter-clojure/` crate directory with `Cargo.toml`
      depending on `tree-sitter = "0.25"` and `tree-sitter-clojure = "0.1"`
- [x] 1.2 Create `adapter-clojure/queries/discover.scm` — tree-sitter query
      matching `(deftest|deftest-)` forms and their test names
- [x] 1.3 Create `adapter-clojure/queries/ns.scm` — tree-sitter query matching
      `(ns ...)` declarations and extracting the namespace name
- [x] 1.4 Create `adapter-clojure/queries/deps.scm` — tree-sitter query matching
      `(:require ...)`, `(:use ...)`, `(:import ...)` entries inside `ns` forms
- [x] 1.5 Write Rust query runner function: given a `tree_sitter::Tree` and a
      query file, return captured node ranges as `(name, line, column)` tuples
- [x] 1.6 Write unit tests for each query against realistic Clojure source
      snippets including:
      - Comments, metadata, `#_`, `#?`, strings with parens
      - Complex `:require` forms with `:refer`, `:as`, and `:rename`
        (e.g., `(:require [clojure.test :refer [deftest is] :as test])`)
      - Nested `:import` forms

## 2. Adapter binary
- [x] 2.1 Create `adapter-clojure/src/main.rs` with the main JSON loop
- [x] 2.2 Implement `cmd_handshake()`: declare `languages: ["clojure"]`,
      `granularity: "file"`, `symbol_model_complete: false`,
      `fingerprinting: true`, `runtime_edges: false`
- [ ] 2.3 Implement `cmd_discover()`: scan project for `.clj` files
      (respecting deps.edn `:test-paths` or default `test/`), parse each
      with tree-sitter, run the discover query, return test items
- [ ] 2.4 Implement `cmd_static_deps()`: for each changed file, parse with
      tree-sitter, run the deps query, find tests that require the changed
      file's namespace, return edges with `weight: 1_000_000`, `origin: "static"`
- [ ] 2.5 Implement `cmd_fingerprint()`: blake3 hash of file contents
      (identical to existing adapters)
- [ ] 2.6 Implement `cmd_run_args()`: detect deps.edn vs project.clj, build
      runner args for selected test namespaces
- [ ] 2.7 Implement `cmd_ingest()`: parse JUnit XML output from Cognitect
      runner or Leiningen stdout, return runtime edges and per-test results
- [x] 2.8 Implement deps.edn reader: extract `:test-paths` and detect
      Cognitect test runner alias
- [x] 2.9 Implement project.clj reader: extract `:test-paths` and detect
      Leiningen project

## 3. Workspace integration
- [x] 3.1 Add `adapter-clojure` to the root `Cargo.toml` workspace members
- [x] 3.2 Pin `tree-sitter = "0.25"` and `tree-sitter-clojure = "0.1"`
      in `adapter-clojure/Cargo.toml` (matching pretender's versions)
- [x] 3.3 Add `.clj` and `.cljs` extension mappings to `config.rs` defaults
- [x] 3.3 Add `deps.edn` and `project.clj` to `detect_project_language()` in
      `config.rs` → returns `"testaruda-adapter-clojure"`
- [x] 3.4 Update `main.rs` CLI messages to mention Clojure adapter
- [x] 3.5 Update `testaruda.toml` default template to include `.clj`/`.cljs`

## 4. Testing
- [ ] 4.1 Create a fixture Clojure project under
      `adapter-clojure/tests/fixtures/clojure/` with `deps.edn`, `src/`,
      `test/`, and sample `deftest` tests
- [ ] 4.2 Write integration tests for the full adapter pipeline:
      handshake → discover → static-deps → fingerprint → run-args → ingest
- [ ] 4.3 Write seeded-fault recall test: change a source file and confirm
      the dependent test is selected
- [ ] 4.4 Write project detection tests for `deps.edn` and `project.clj`
- [ ] 4.5 Write espectacular contracts covering ADAPT-017, ADAPT-018, ADAPT-019
- [ ] 4.6 Run `ah check` to validate spec-contract correspondence

## 5. Documentation
- [ ] 5.1 Add Clojure support section to `README.md` with setup instructions
      (prerequisites: Clojure CLI or Leiningen)
- [ ] 5.2 Update `testaruda.toml` example in docs to show `.clj` adapter
      config