## Context

testaruda's adapter protocol (`TIA-ADAPT-001`) is JSON-over-stdin/stdout;
the core spawns one subprocess per configured file extension and holds no
language-specific logic itself. The existing Rust and Python adapters are
both Rust binaries in the same crate — `src/bin/adapter-rust.rs` and
`src/bin/adapter-python.rs`. The Clojure adapter follows the same pattern.

The sibling project **pretender** (at `../pretender/`) already depends on
`tree-sitter-clojure` and has a working Clojure parser using tree-sitter
queries. The Clojure adapter can reuse the same approach without coupling
to pretender's API.

The key difference from the Rust/Python adapters is that Clojure's homoiconic
syntax makes regex-based extraction fragile. tree-sitter provides a full CST
with named nodes (`list_lit`, `sym_lit`, `vec_lit`, `kw_lit`) that can be
queried declaratively via Scheme `.scm` files.

## Decision 1: tree-sitter queries (not custom s-expression parser, not Babashka)

**Chosen:** The adapter uses `tree-sitter-clojure` to parse each `.clj` file
into a CST, then runs tree-sitter queries (`.scm` files) to extract:
- `deftest` / `deftest-` forms for test discovery
- `:require` / `:use` / `:import` entries from `ns` forms for dependency edges
- The namespace name from `(ns ...)` declarations

Tree-sitter handles all edge cases (comments, `#_` discard forms, reader
conditionals, strings with embedded parens, metadata) natively — no custom
logic needed.

**Rejected: hand-rolled s-expression parser.** A custom balanced-paren
tokenizer must handle `#_` comment forms, `#?` reader conditionals, metadata
blocks, string literals with escaped parens, and tagged literals. Each is a
potential bug source. tree-sitter gives all of this for free.

**Rejected: shelling out to Babashka.** Adding `bb` as a runtime dependency
breaks the single-binary distribution model. Babashka must be installed
separately, which adds friction for CI setup and differs from how the
existing Rust/Python adapters work.

## Decision 2: tree-sitter query scope — just enough for deps and tests

The adapter uses tree-sitter queries (Scheme `.scm` files) to extract the
relevant forms. The `.scm` files are embedded into the binary at compile time
using `include_str!()`, following the same pattern as pretender's
`languages/clojure/metrics.scm`.

### Test discovery query
```scheme
(list_lit
  value: (sym_lit) @_deftest
  (#match? @_deftest "^(deftest|deftest-)$")
  .
  value: (sym_lit) @test_name) @test_item
```
This finds every `(deftest name ...)` and `(deftest- name ...)` form
regardless of surrounding metadata, comments, or reader conditionals —
tree-sitter handles those internally.

### Namespace extraction query
`ns` is a `sym_lit` node (not a `kw_lit` — Clojure uses bare symbols for
special forms like `ns`, `defn`, `def`).
```scheme
(list_lit
  value: (sym_lit) @_ns
  (#eq? @_ns "ns")
  .
  value: (sym_lit) @namespace_name) @ns_form
```

### Dependency extraction query
```scheme
(list_lit
  value: (kw_lit) @_keyword
  (#match? @_keyword "^:(require|use|import)$")
  .
  [(vec_lit) (sym_lit)] @dep_entry) @dep_form
```
The adapter then walks the captured `@dep_entry` nodes to extract each
namespace string. For each `@dep_entry`:
- If it is a `vec_lit` (e.g. `[my-project.core :as core]`), the **first child**
  `sym_lit` is the namespace name. The adapter MUST NOT pick up subsequent
  children (`:as`, `:refer`, `:rename` entries).
- If it is a bare `sym_lit` (e.g. `my-project.core`), the symbol itself is
  the namespace name.

The adapter does NOT need to handle:
- Full Clojure evaluation semantics
- Macro expansion
- Java interop forms beyond `(:import ...)`
- Symbol-level resolution (`:refer`, `:rename`, aliases)

## Decision 3: dependency resolution

### Direction
For each changed source file, the adapter SHALL parse its `:require`/`:use` forms
from the `ns` declaration to determine the namespace it exports. It SHALL then
find all test files whose `:require` forms include that namespace, and emit
edges FROM those test files TO the changed source file. This mirrors the
direction used by the Python and Rust adapters.

### Simple case (80% of projects)
`(:require [my-project.core :as core])` → direct 1:1 edge from importing test
to the file that exports `my-project.core`. The namespace is resolved to a
file path by:
1. Mapping periods to slashes (`my-project.core` → `my-project/core`)
2. Replacing hyphens with underscores (`my-project/core` → `my_project/core`)
3. Appending `.clj` (`src/my_project/core.clj`)

Clojure namespace names use hyphens, but the corresponding file paths use
underscores (e.g., `my-project.core` → `src/my_project/core.clj`). The
adapter SHALL replace hyphens with underscores when mapping a namespace to
a file path.

### Require forms
Clojure supports three `:require` notations — all three resolve to the same
namespace string:
- **Vector notation:** `[my-project.core :as core]` — namespace is `my-project.core`
- **Bare symbol notation:** `my-project.core` (no brackets) — namespace is `my-project.core`
- **Compound vector notation:** `[my-project.core :refer [foo] :as core]` —
  namespace is `my-project.core`; `:refer` and `:as` are ignored for dependency
  edge construction

### Prefix lists
`(:require [clojure.java.io :as io])` — the adapter creates an edge from the
importing test to `clojure.java.io`. The full namespace is used as the
dependency key, not the alias.

### `:refer :all`
`(:require [clojure.test :refer :all])` — handled as a dependency on
`clojure.test`. Core treats this as a file-level edge; no symbol-level
resolution is attempted.

### Stdlib namespace filtering
The adapter MAY filter out well-known Clojure stdlib namespaces
(`clojure.test`, `clojure.string`, `clojure.java.io`, etc.) from dependency
edges to reduce noise, since they can never be in the changed file set.
This is a performance optimization, not a correctness requirement.

### Unresolved requires
`(:require [some.dependency])` where `some.dependency` is not in the project's
source tree — the adapter still emits the edge. testaruda's core handles
out-of-project dependencies via the fallback mechanism (TIA-SAFE-004).

### Java interop
`(:import java.util.Date)` — the adapter SHALL NOT create a separate
dependency edge for `java.util.Date`. The existing edge from the test to
the source file (via `:require`) already covers the change. If a Java
import changes, the source file's fingerprint changes, and the test is
selected via the existing edge.

## Decision 4: runner detection and configuration

The adapter reads `deps.edn` (preferred) or `project.clj` (fallback) at
startup to determine the test runner and test paths. The adapter SHALL check
that `clojure` or `lein` is on PATH before defaulting to the respective runner;
if neither is found, the adapter SHALL return an error on `run-args`.

| Runner | Detection | Args format |
|--------|-----------|-------------|
| Cognitect test runner | `:deps {io.github.cognitect-labs/test-runner ...}` in deps.edn, or `:aliases :test` with `:extra-deps` | `clojure -M:test -n namespace` |
| Leiningen | `project.clj` exists | `lein test :only namespace/test-name` |
| Kaocha | deps.edn `:test` alias contains `lambdaisland/kaocha` in `:extra-deps` | `clojure -M:test --focus namespace` |

Default: `clojure -M:test` (Cognitect style). If deps.edn is present, the
adapter checks for a `:test` alias. If project.clj is present, it uses
Leiningen.

## Decision 5: output parsing

The Cognitect test runner outputs per-test results in a structured format
when run with `--junit` or in verbose mode. The adapter supports:

1. **JUnit XML** (`--junit <file>`) — primary format, same as Python adapter
2. **Cognitect EDN output** — `{:test 5, :pass 4, :fail 1, :error 0}` with
   per-test details in `*test-out*`
3. **Leiningen output** — parsed from stdout patterns

The adapter does NOT depend on a Clojure EDN parser. JUnit XML is the
preferred output format; the adapter configures the runner to produce it.

## Decision 6: no namespace-level granularity for v1

The adapter declares `granularity: "file"` and `symbol_model_complete: false`.
While `deftest` gives test-level granularity on the test side, the source
side has no symbol boundaries (Clojure is a Lisp-1 with no module-level
encapsulation). A change to any top-level form in a source file could affect
any test that requires that namespace. This is a conservative starting point;
symbol-level could be added later via `clojure.tools.analyzer` integration.

## Risks and Trade-offs

- **tree-sitter dependency:** The adapter binary pulls in `tree-sitter` and
  `tree-sitter-clojure` as Rust crate dependencies. This adds ~30s to the
  first build and ~400KB to the binary. Mitigation: the adapter is a separate
  Cargo workspace member, so the core binary is unaffected.
- **tree-sitter version compatibility:** `tree-sitter-clojure` must be
  compatible with the `tree-sitter` crate version. The pretender project
  already maintains this compatibility (uses tree-sitter 0.25).
- **deps.edn parsing:** Clojure's EDN format is not JSON. The adapter needs
  a lightweight EDN parser or must use a heuristic. Mitigation: the adapter
  only reads `:deps` keys and `:aliases`, which appear at predictable
  positions. The deps.edn reader SHALL use a balanced-brace scanner to
  correctly handle nested maps (e.g., `:deps {my-lib {:mvn/version "1.0"}}`).
- **tree-sitter build dependency:** `tree-sitter-clojure` requires a C
  compiler (cc/gcc) at build time to compile the embedded grammar. This is
  already a dependency of the sibling pretender project and is standard for
  Rust CI, but should be noted in setup documentation.
- **project.clj parsing:** As Clojure data structures, these are harder to
  parse. Mitigation: Leiningen support is secondary; deps.edn is the primary
  target. A regex-based heuristic for `defproject` dependencies is acceptable.
- **Test discovery with `deftest` in source files:** Clojure allows `deftest`
  in any file, not just test directories. The adapter discovers `deftest` in
  all `.clj`/`.cljs` files under the project root, but filters by test path
  conventions (based on deps.edn `:test-paths` or default `test/`).

## Deferred (not needed for v1)

- **Symbol-level granularity** — requires `clojure.tools.analyzer` integration
  or static analysis of `defn`/`defmacro` boundaries.
- **Leiningen `:test-selectors`** — complex test filtering is a project
  configuration concern, not an adapter concern.
- **ClojureScript (.cljs) support** — the `.cljs` extension is registered
  but the adapter only tests against `.clj` files initially.
- **Babashka (`bb.edn`) support** — could be added later as a runner option.