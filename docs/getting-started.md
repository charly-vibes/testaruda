# Getting Started

## Prerequisites

- Rust 1.75+
- Git (for change detection)
- Soufflé (optional, for oracle validation)

## Installation

### From source

```bash
git clone https://github.com/charly-vibes/testaruda
cd testaruda
cargo build --release
cp target/release/testaruda ~/.local/bin/
```

### From crates.io

```bash
cargo install testaruda
```

## First Run

```bash
# Initialize the store (creates .testaruda/ directory)
testaruda init

# Create some test content units
testaruda select --files "src/main.rs"

# See the dependency graph
testaruda graph
```

## Local Development Workflow

```bash
# After making changes, select affected tests
testaruda select

# Run selected tests (with your test runner)
cargo test  # or pytest, jest, etc.

# Ingest results to improve future selection
# (requires a results.json file)
testaruda ingest results.json
```