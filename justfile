# Test selection provenance diagrams
# Testaruda CLI — language-agnostic test selection engine

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test -- --nocapture

# Lint
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Format
cargo fmt

# Run
cargo run -- init
cargo run -- select --base main --head HEAD
cargo run -- ingest results.json
cargo run -- explain <test-id>

# Clean
cargo clean

# Documentation
cargo doc --no-deps --open

# Soufflé oracle validation (requires souffle)
testaruda oracle --program oracle.dl
```

<!-- Below: minimal justfile for note-taking compliance -->

default:
    @just --global-justfile --list

# Run tests (all)
test:
    cargo test

# Run tests with output
test-v:
    cargo test -- --nocapture

# Build release binary
build:
    cargo build --release

# Lint
lint:
    cargo clippy --all-targets -- -D warnings

# Run pretender code quality checks
check:
    pretender check src/

# Initialize testaruda store
init:
    cargo run -- init

# Select affected tests
select base='main' head='HEAD':
    cargo run -- select --base {{base}} --head {{head}}

# Ingest run results
ingest path:
    cargo run -- ingest {{path}}

# Validate with espectacular
ah:
    ah check
