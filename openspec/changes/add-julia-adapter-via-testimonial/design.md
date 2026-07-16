## Context

testaruda's adapter protocol (`TIA-ADAPT-001`) is JSON-over-stdin/stdout;
the core spawns one subprocess per configured file extension and holds no
language-specific logic itself. This is how the Rust and Python adapters work —
both are separate binaries the core spawns. Julia is different: to get per-test
line coverage you need to *run* Julia code with `--code-coverage=user`, which
means the adapter must itself be a Julia program. That constraint is unavoidable.

Given that, the only real design question is *where that Julia program lives
and what it does* — not whether it needs to exist.

## Decision 1: the adapter is a Testimonial.jl subcommand, not a separate repo

**Chosen:** add one new entry point to Testimonial.jl's own package —
`Testimonial.run_adapter_protocol()`, wired to a thin executable script
(`bin/testaruda_adapter.jl`) that reads one JSON command per line from stdin
and writes one JSON response per line to stdout, per `TIA-ADAPT-001`.
testaruda's `testaruda.toml` then points `.jl` at the adapter entry point.
The recommended invocation is `-e` based, which works for both system-installed
and local-dev setups:
```toml
[adapters]".jl" = "julia --project=. -e 'using Testimonial; Testimonial.Protocol.run_adapter_protocol()'"
```
For local development, an alternative is `julia --project=/path/to/Testimonial.jl bin/testaruda_adapter.jl`.

**Rejected: a separate `testaruda-adapter-julia` repository.** No technical
win — the recording logic (subprocess spawning, `.jl.cov` parsing) has to exist
somewhere in Julia regardless, and splitting it into a second repo means two
`Project.toml`s, two CI configs, two release cadences.

**Rejected: Julia code inside testaruda's own Cargo workspace.** Breaks the
zero-language-specific-logic principle and would force a Julia toolchain into
testaruda's own build/CI for a feature most testaruda contributors never touch.

## Decision 2: protocol-command mapping

| testaruda protocol command | Backed by (Testimonial.jl internals) | Notes |
|---|---|---|
| `handshake` | static response | `languages: ["julia"]`, `granularity: "file"`, capabilities `{symbol_model_complete: false, fingerprinting: true, runtime_edges: true}` — unlike the superseded proposal's `runtime_edges: false`, because coverage-based edges *are* runtime edges |
| `discover` | Enumerate `@testitem`s under `test_directories`. Node ID = `test_file:line` (stable, location-based), not name-based. See ADAPT-015 rationale. |
| `static-deps` | **First invocation (no coverage recorded):** every changed file → `unresolved`. This triggers testaruda's existing fallback (`TIA-SAFE-004`). **Subsequent invocations:** look up changed files in the coverage map built by `ingest` and return those edges |
| `ingest` | Per-item subprocess with `--code-coverage=user`, parse `.jl.cov` via `Coverage.jl`, keep only `count > 0` lines. Return file→line→test edges as runtime edges. No separate `.testimonial/index.jls` persistence — testaruda's SQLite store is the system of record |
| `fingerprint` | SHA-256 of file contents — standard library, avoids non-stdlib dep |
| `run-args` | Emit `ReTestItems.runtests` invocation args for selected node IDs (`test_file:line`). Resolve each node ID to `(file, name)` pairs via AST parser. |

## Decision 3: granularity is file-level for v1

`@testitem`-level coverage recording gives *test-level* precision on the source
side (which lines each test covered) — but testaruda's adapter capability flags
distinguish `granularity: "symbol"` (source-code symbol-level resolution) from
`"file"`. Since this adapter has no symbol resolution on the *source* side (it
doesn't know which Julia function a line belongs to), it should declare
`granularity: "file"` and `symbol_model_complete: false`. The precision gain
over the superseded proposal is in the *dependency edges* (empirical vs.
statically-guessed), not in the granularity of the change side.

## Decision 4: subprocess isolation per @testitem (CAVEATED — SPIKE NEEDED)

A single full-suite coverage run gives aggregate hit counts with no way to tell
which test hit which line, so *some* form of per-test isolation is unavoidable
for per-test attribution.

**What is not yet settled:** whether that isolation must be a separate OS
subprocess per `@testitem`, or whether Julia ≥1.11's `Base.reset_coverage()`
permits sequential in-process resets instead. Testimonial.jl's own two design
documents disagree (checked 2026-07-15). Until resolved, treat
subprocess-per-item as the safe default.

**Firm (uncontested):** `.jl.cov` sidecar parsing via `Coverage.jl`, keeping
only `count > 0` lines.

## Decision 5: multi-package monorepo scoping — UNRESOLVED, BLOCKING

Testimonial.jl's stated primary target is "Julia monorepos with 10+ packages."
Neither this design nor either source repo currently answers how `discover` and
`static-deps` should behave across multiple `Project.toml`s in one repo.

Two candidate approaches:

1. **`discover` walks the whole repo tree for every `Project.toml`**, producing
   one merged `TestItemRef` set spanning all packages, keyed by absolute path.
   Keeps "one testaruda invocation per repo" intact; composes with testaruda's
   existing single-store architecture.

2. **testaruda is configured/invoked once per package**, each with its own
   adapter config scoped to that package's subtree. Simpler for the adapter but
   multiplies invocation/config overhead by package count and makes cross-package
   impact analysis harder.

**This must be decided before implementation** — shipping something that only
discovers the root package's tests would be a recall-safety failure for the
exact use case Testimonial.jl was designed for.

## Deferred (not needed to prove the core works)

The following Testimonial.jl requirement IDs (`REC-` prefix) reference its own
recording/caching spec — listed here with their mapping:
- Caching (`Testimonial.jl REC-003`), parallel recording (`Testimonial.jl
  REC-004`), cache cleanup (`Testimonial.jl REC-010`) — all performance, not
  correctness
- Public standalone recording APIs (`Testimonial.jl REC-006`/`REC-007`) — not
  needed if the only caller is the adapter itself
- Inference layer (`SnoopCompile.jl`) and static layer (`JET.jl`) — genuinely
  Julia-specific, no Rust-core equivalent; worth building later as additional
  edge sources
