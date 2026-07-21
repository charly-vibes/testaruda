# Getting Started

## Prerequisites

- Rust 1.75+
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

## Installation

```bash
cargo install testaruda
```

Or from source:

```bash
git clone https://github.com/charly-vibes/testaruda
cd testaruda
cargo build --release
# Binaries: testaruda, testaruda-adapter-rust, testaruda-adapter-python
cp target/release/testaruda* ~/.local/bin/
```

## First Run

```bash
# Initialize store + write default config
testaruda init

# Discover tests via adapters (scans project for #[test], test_*.py, etc.)
testaruda discover

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