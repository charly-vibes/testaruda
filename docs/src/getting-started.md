# Getting Started

## Prerequisites

- A stable Rust toolchain
- Git (for change detection)

### Optional: Julia adapter

To use testaruda with Julia projects, you additionally need:

- [Julia](https://julialang.org/) 1.9+ (coverage recording requires Julia 1.12+ for LCOV tracefile support, or any 1.x for `.jl.cov` sidecar files)
- [Testimonial.jl](https://github.com/sashakile/Testimonial.jl) — the adapter lives inside this package

```bash
julia -e 'using Pkg; Pkg.add("Testimonial")'
# Link the adapter wrapper onto PATH
ln -s ~/.julia/packages/Testimonial/*/bin/testaruda-adapter-julia ~/.local/bin/
```

The Julia adapter discovers `@testitem` tests (ReTestItems/TestItems.jl) and
`@testset` blocks (Base.Test). In a project that mixes `@testitem` with plain
`@test` blocks, both the `@testitem` and `@testset` tests are available for
selection. For files with no `@testitem` or `@testset` blocks, a file-level
fallback is used — the entire file is run via `include()`.

### Optional: .NET adapter (titi)

To use testaruda with .NET projects, you additionally need:

- [titi](https://github.com/charly-vibes/titi) — a .NET monorepo orchestrator
  that speaks testaruda's adapter protocol via its `testaruda-adapter` subcommand

```bash
# Follow the titi installation instructions at the link above
# Then configure the extension mapping in testaruda.toml:
# [adapters.extensions]
# ".cs" = "titi testaruda-adapter"
```

The .NET adapter is an external binary (not a workspace crate) and is opt-in
(not auto-detected), following the same pattern as the Julia adapter. See the
[Configuration Guide](configuration.md#net-adapter-titi) for the full list of
.NET extension mappings.

## Installation

```bash
cargo install testaruda
```

Or from source:

```bash
git clone https://github.com/charly-vibes/testaruda
cd testaruda
cargo install --path .
# Installs testaruda, testaruda-adapter-rust, and testaruda-adapter-python
```

## First Run

```bash
# Initialize store + write default config
testaruda init

# Discover tests via adapters (scans project for #[test], test_*.py, etc.)
testaruda discover
# Prints the number of test items stored in .testaruda/

# Select tests affected by uncommitted changes
testaruda select

# Select tests between two revisions
testaruda select --base main --head feature

# Machine-readable JSON plan (both orderings accepted; global flag preferred)
testaruda --json select

# Shadow mode: compute selection but signal "run all tests"
testaruda select --shadow

# Explicit file list
testaruda select --files "src/lib.rs,src/main.rs"
```

A successful selection prints the selected test records and the reason for any
safety fallback. If no dependency data exists yet, testaruda intentionally
over-selects rather than risking a missed test.

## The calibration ramp (first cycles run everything — by design)

Selection quality depends on run history in the store. A **cold** store (fresh
`init`) has no evidence about which tests failed recently or how tests relate
to each other at runtime, so testaruda errs on the side of recall (SAFE-007):
every test with no recorded history lands in the always-run set and the engine
falls back to the full suite. Expect this sequence:

| Cycle | Command | What you see |
|---|---|---|
| 1. `init` + `discover` | `testaruda init && testaruda discover` | Store + test inventory, no selection data |
| 2. First full run | `testaruda exec` (or CI with ingest) | Full suite runs, results are ingested |
| 3. Warm selection | `testaruda exec` again after a change | Small, surgical selection |

On the first `exec` cycle a full-suite run is **expected, not a bug**. If
selection falls back with

```
exit code 10: confidence below threshold
```

that is the confidence-threshold fallback (TIA-SAFE-002) telling you history
is still too thin — run the suite once more and the ramp completes. Real
measured ramps from the Rust benchmark suite (warm cycle after 1–2 exec runs):

| Repo | Selected / Total | Notes |
|---|---|---|
| dont | 5 / 950 | full history |
| genesis | 6 / 502 | full history |
| espectacular | 17 / 401 | includes inline-test self-edges |
| vampiro | 91 / 856 | threshold-gated until history accumulates |

Repositories with partial git history (shallow clones, squashed imports) ramp
slower: SAFE-007 keeps tests without any recorded run in the always-run set
until each has executed at least once. Two to three `exec` cycles is the
normal ramp; `testaruda calibrate` reports the ranking gate if you use
predictive ranking.

## CI safety mode

Use safe mode when selection should execute tests in CI. It performs preflight
checks and falls back to the full Cargo test suite if configuration, store data,
Git revisions, or confidence are insufficient:

```bash
testaruda select --safe --base origin/main --head HEAD
```

## Next Steps

- See the [**CLI Reference**](cli.md) for all commands and options (including
  `calibrate`, `ingest`, `graph`, `import`, `explain`, `oracle`, `discover`,
  `metrics`, `completions`, and their flags).
- See the [**Configuration Guide**](configuration.md) for adapter setup and
  `testaruda.toml` reference.
- See the [**Agent Mode Guide**](agent-mode.md) for structured JSON output
  intended for LLM coding agents.
- See the [**Architecture Overview**](architecture.md) for the high-level
  system design.
