# Change: Add Clojure adapter (tree-sitter queries)

## Why

testaruda currently supports Rust and Python (in-tree adapters) and Julia (via
Testimonial.jl integration). Clojure is used in production at several shops
that rely on test-selection tooling, and the Clojure toolchain (deps.edn,
Leiningen) has well-defined source/test paths that make static dependency
analysis tractable.

A Clojure adapter is structurally similar to the existing Rust/Python adapters
— a Rust binary that speaks the JSON adapter protocol — but uses
`tree-sitter-clojure` queries to extract `:require`/`:use` forms and
`deftest` discovery instead of regex or a hand-rolled parser. This is the same
approach used by the sibling project **pretender** for Clojure metrics.

## What Changes

1. **New crate:** `adapter-clojure/` — a separate workspace member producing
   the `testaruda-adapter-clojure` binary, implementing the 6 adapter protocol
   commands (`handshake`, `discover`, `static-deps`, `fingerprint`, `run-args`,
   `ingest`).

2. **tree-sitter queries:** Declarative Scheme queries (`.scm` files) using
   `tree-sitter-clojure` to extract:
   - `deftest` / `deftest-` forms for test discovery
   - `(:require ...)` / `(:use ...)` / `(:import ...)` forms for dependency edges
   - `(ns ...)` declarations for namespace resolution

   Tree-sitter handles all edge cases (comments, `#_` discard forms, reader
   conditionals, strings with embedded parens, metadata) natively — no custom
   s-expression parser needed.

3. **Runner detection:** The adapter reads `deps.edn` (preferred) or
   `project.clj` (fallback) to determine the test runner. Supported runners:
   - Cognitect test runner (`clojure -M:test`)
   - Leiningen (`lein test`)
   - Kaocha (`clojure -M:test` with kaocha as the test runner, via deps.edn `:test` alias)

4. **Config registration:** `testaruda.toml` gets `.clj` and `.cljs` extension
   mappings pointing to `testaruda-adapter-clojure`.

5. **Language detection:** `testaruda init` probes for `deps.edn` and
   `project.clj` and sets the default adapter to `testaruda-adapter-clojure`
   when detected.

## Impact

- **Affected specs:** `adapter-protocol` (new requirements ADAPT-017..019),
  `change-detection` (CHG-009 update for Clojure project detection).
- **Affected code:** `adapter-clojure/Cargo.toml` (new crate with
  `tree-sitter` and `tree-sitter-clojure` deps), `adapter-clojure/src/`
  (new adapter binary), `adapter-clojure/queries/` (`.scm` query files),
  `src/config.rs` (language detection + default extensions), `src/main.rs`
  (adapter name in CLI messages).
- **No change to core engine:** The adapter protocol is unchanged; the core
  already handles the required commands generically.
- **New external dependency:** `tree-sitter` and `tree-sitter-clojure` — but
  only in the separate `adapter-clojure` crate, not in the core.

## Success criteria

This change is complete when:

1. `testaruda select --files <changed.clj>` against a Clojure project with
   `deps.edn` returns a non-empty, correct selection with edges of origin
   `static`.
2. The adapter passes a seeded-fault recall check: modify a source namespace,
   confirm the test that requires it is still selected.
3. `testaruda init` in a directory with `deps.edn` generates a
   `testaruda.toml` with `default = "testaruda-adapter-clojure"`.
4. `ah check`-equivalent coverage exists for the 3 new requirements
   (ADAPT-017..019).
5. The adapter binary builds without error in a fresh checkout of the
   workspace (`tree-sitter-clojure` compiles correctly).