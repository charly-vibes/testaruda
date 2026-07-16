## ADDED Requirements

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