# Change: Add TypeScript adapter (tree-sitter queries)

## Why

testaruda currently supports Rust, Python (in-tree adapters), Clojure (tree-sitter adapter in progress), and Julia (via Testimonial.jl). TypeScript/JavaScript is the most widely used language ecosystem in web development, and several production shops that depend on test-selection tooling maintain large TypeScript monorepos.

A TypeScript adapter follows the same pattern as the Clojure adapter — a Rust binary that speaks the JSON adapter protocol — using `tree-sitter-typescript` queries to extract `import`/`require()` forms for dependency analysis and `describe`/`it`/`test` blocks for test discovery. Tree-sitter handles all edge cases (decorators, JSDoc comments, string interpolation, optional chaining) natively — no regex-heuristic parser needed.

## What Changes

1. **New crate:** `adapter-typescript/` — a separate workspace member producing the `testaruda-adapter-typescript` binary, implementing the 6 adapter protocol commands (`handshake`, `discover`, `static-deps`, `fingerprint`, `run-args`, `ingest`).

2. **tree-sitter queries:** Declarative Scheme queries (`.scm` files) using `tree-sitter-typescript` to extract:
   - `describe`/`it`/`test` blocks for test discovery
   - `import ... from` declarations and `require(...)` calls for dependency edges
   - Support for both `.ts` and `.tsx` (TypeScript) via the grammar switch, with `.js`/`.jsx` (JavaScript) support planned using `tree-sitter-javascript`

   Tree-sitter handles all structural edge cases (decorators on test functions, JSDoc, template literals with imports, optional chaining, type-only imports) natively — no regex needed.

3. **Runner detection:** The adapter probes for test framework configuration at startup:
   - **Vitest** (preferred): `vitest.config.ts`, `vitest.config.js`, or `vite.config.ts` with Vitest plugin
   - **Jest** (fallback): `jest.config.ts`, `jest.config.js`
   - **Fallback:** `package.json` scripts, defaulting to `npx vitest run` if ambiguous

4. **Config registration:** `testaruda.toml` gets `.ts`, `.tsx`, `.mts`, `.cts` extension mappings pointing to `testaruda-adapter-typescript`. `.js`/`.jsx` support is deferred to a follow-up change (the JavaScript grammar `tree-sitter-javascript` is architecturally identical but has different test-framework conventions).

5. **Language detection:** `testaruda init` probes for `vitest.config.ts`, `jest.config.ts`, `package.json` with `vitest`/`jest` in devDependencies, and sets the default adapter to `testaruda-adapter-typescript` when detected. Projects using only `.js`/`.jsx` (no `.ts`) require manual `testaruda.toml` configuration until JavaScript support is added.

## Impact

- **Affected specs:** `adapter-protocol` (new requirements ADAPT-020..022), `change-detection` (CHG-009 update for TypeScript project markers).
- **Affected code:** `adapter-typescript/Cargo.toml` (new crate with `tree-sitter` and `tree-sitter-typescript` deps), `adapter-typescript/src/` (new adapter binary), `adapter-typescript/queries/` (`.scm` query files), `src/config.rs` (language detection + default extensions).
- **No change to core engine:** The adapter protocol is unchanged; the core already handles all required commands generically.
- **New external dependency:** `tree-sitter` and `tree-sitter-typescript` — but only in the separate `adapter-typescript` crate, not in the core.

## Success criteria

This change is complete when:

1. `testaruda select --files <changed.ts>` against a TypeScript project with Vitest returns a non-empty, correct selection with edges of origin `static`.
2. The adapter passes a seeded-fault recall check: modify a source module, confirm the test that imports it is still selected.
3. `testaruda init` in a directory with `vitest.config.ts` generates a `testaruda.toml` with `default = "testaruda-adapter-typescript"`.
4. `ah check`-equivalent coverage exists for the 3 new requirements (ADAPT-020..022).
5. The adapter binary builds without error in a fresh checkout of the workspace (`tree-sitter-typescript` compiles correctly).