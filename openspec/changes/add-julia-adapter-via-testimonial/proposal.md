# Change: Add JuliaLang adapter (via Testimonial.jl integration)

> **Supersedes** `openspec/changes/archive/2026-07-15-add-julia-adapter/`.
> The earlier proposal targeted `Test.jl`/`@testset` + `include()`-parsing for a
> separate `testaruda-adapter-julia` repository. This replacement uses
> `ReTestItems.jl`/`@testitem` + subprocess coverage recording, living inside
> Testimonial.jl as a subcommand — a strictly stronger mechanism (empirical
> edges instead of static guesses).

## Why

testaruda's original Julia adapter proposal (`archive/2026-07-15-add-julia-adapter/`)
planned a static adapter that discovered `Test.jl` `@testset`s and computed
dependency edges by regex-parsing `include("...")` calls. That design's own risk
table named its weakest points: `import`/`using` resolution was "deferred to a
follow-up change" (an explicit under-approximation), and `include` resolution was
flagged as "complex (relative paths, `LOAD_PATH`, `Project.toml` source paths)."

Separately, **Testimonial.jl** (`github.com/sashakile/Testimonial.jl`) is a
Julia-native test impact analysis tool (spec-only, pre-code as of 2026-07-15)
whose Phase 1 design already solved the same sub-problem with a mechanically
stronger approach: run each `@testitem` (from `ReTestItems.jl`) in an isolated
subprocess with `--code-coverage=user`, and parse the resulting `.jl.cov` sidecar
via `Coverage.jl` to get an exact, empirical file→line→test map. No static
resolution, no `import`/`using` risk, no relative-path guesswork — the edges are
ground truth from an actual execution.

**Building two independent tools that both compute test-selection for Julia is
strictly more work and more risk than building one adapter that reuses the part
of Testimonial.jl that testaruda has no equivalent for, and discarding the parts
that duplicate what testaruda's Rust core already does.**

## What Changes

1. **testaruda side:** Treat `archive/2026-07-15-add-julia-adapter/` as superseded.
   The 3 requirements it added (`TIA-ADAPT-014..016`) are replaced by the
   reworked requirements below, which describe coverage-based edges from a
   `ReTestItems.jl`/`@testitem` adapter instead of `Test.jl`/`@testset` +
   `include()`-parsing. No change to testaruda's core (`src/adapter.rs`,
   `src/main.rs`) beyond what any new adapter needs — the JSON protocol already
   supports runtime edges via the existing `ingest` command.

2. **Testimonial.jl side:** The adapter lives inside Testimonial.jl as a new
   entry point (`Testimonial.run_adapter_protocol()` or a standalone executable
   script). Testimonial.jl contributes:
   - `@testitem` discovery (parsing `ReTestItems.jl` source)
   - Per-item subprocess coverage recording with `--code-coverage=user`
   - `.jl.cov` parsing via `Coverage.jl`
   - SHA-256 content fingerprinting (stdlib dependency — avoids adding blake3)

   Testimonial.jl's own proposals for confidence scoring, safety invariants,
   provenance/explainability, runtime feedback, and component-boundary analysis
   are **retired as superseded-by-integration** — testaruda's Rust core already
   implements the equivalent concepts.

3. **No new repository, no new binary name.** The "adapter" is a thin JSON
   dispatcher added directly to Testimonial.jl's own package, pointed to by
   `testaruda.toml` via a shim like `julia --project=. bin/testaruda_adapter.jl`.
   testaruda's Rust codebase gains zero Julia-specific code or Julia toolchain
   dependency.

## Impact

- **Affected specs:** `adapter-protocol` — replaces TIA-ADAPT-014..016 with
  reworked requirements reflecting `@testitem`-based discovery, coverage-based
  runtime edges, and the Testimonial.jl-hosted architecture.
- **Affected project metadata:** `openspec/project.md` — requirement-count
  entries updated for SEL, LOCAL, AGENT, OBS, PORT, and ADAPT rows.
- **Affected code:** None in testaruda core. The adapter code lives entirely
  in Testimonial.jl's repository.
- **External:** No new repo. Testimonial.jl gains a `bin/testaruda_adapter.jl`
  entry point. testaruda's install docs gain a note: "for Julia support, also
  install `Testimonial.jl` via `Pkg.add`."
- **Migration:** testaruda's default extension mapping and project detection
  (from the superseded proposal) are not adopted. The adapter is opt-in via
  manual `testaruda.toml` configuration rather than auto-detected, because
  `ReTestItems.jl` is not Julia stdlib — requiring it as a hard dependency for
  auto-detection would be misleading.

## Success criteria

This change is complete when:

1. `testaruda select --files <changed.jl>` against a real `ReTestItems.jl`
   project returns a non-empty, correct selection with a witness edge of origin
   `Runtime`.
2. The adapter passes a **seeded-fault recall check**: deliberately break a
   covered line's behavior, confirm the test that should catch it is still
   selected (mirroring `TIA-VER-004`'s pattern for the Rust/Python adapters).
3. Multi-package monorepo scoping (see `design.md`, Decision 5) has an explicit,
   tested answer — not silent single-package behavior.
4. `ah check`-equivalent coverage exists for the 3 rewritten requirements
   (`TIA-ADAPT-014..016`).
