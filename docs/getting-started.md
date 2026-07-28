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

The Julia adapter discovers `@testitem` tests through ReTestItems/TestItems. If
a project mixes `@testitem` and plain `@test` blocks, only the `@testitem` tests
are available for selection.

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

# Machine-readable JSON plan
testaruda select --json

# Shadow mode: compute selection but signal "run all tests"
testaruda select --shadow

# Explicit file list
testaruda select --files "src/lib.rs,src/main.rs"
```

A successful selection prints the selected test records and the reason for any
safety fallback. If no dependency data exists yet, testaruda intentionally
over-selects rather than risking a missed test.

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
