# Testaruda CLI — language-agnostic test selection engine

# Default: list all recipes
default:
    @just --global-justfile --list

# Build (debug)
build:
    cargo build

# Build (release)
build-release:
    cargo build --release

# Alias for backward compatibility
release: build-release

# Run all tests (single-threaded — adapter tests use global cwd)
test:
    cargo test -- --test-threads=1

# Run tests with output
test-v:
    cargo test -- --nocapture

# Lint (clippy with warnings denied)
clippy:
    cargo clippy --all-targets -- -D warnings

# Alias for lefthook
lint: clippy

# Check formatting
fmt-check:
    cargo fmt --check

# Pre-push checks (fast gate)
pre-push: fmt-check lint test
    @echo "✅ Pre-push checks passed"

# Full CI gate: fmt + lint + test + build
ci: fmt-check lint test build-release
    @echo "✅ CI checks passed"


# Format code
fmt:
    cargo fmt

# Run pretender code quality checks
check:
    pretender check src/

# Validate specs against tests with espectacular
ah:
    ah check

# Initialize testaruda store
init:
    cargo run -- init

# Select affected tests
select base='main' head='HEAD':
    cargo run -- select --base {{base}} --head {{head}}

# Ingest run results
ingest path:
    cargo run -- ingest {{path}}

# Explain a test selection
explain test-id:
    cargo run -- explain {{test-id}}

# Clean build artifacts
clean:
    cargo clean

# Install locally to ~/.cargo/bin
install:
    cargo install --path .

# Build documentation
doc:
    cargo doc --no-deps --open

# Sync CLI reference docs with real --help output
doc-cli:
    @echo "Regenerating docs/cli.md from clap definitions..."
    @cargo run --bin testaruda -- gen-cli-docs > docs/cli.md
    @echo "✅ docs/cli.md regenerated"

# Check that CLI reference docs are in sync with --help
doc-cli-check:
    @echo "Checking CLI docs are in sync..."
    @cargo run --bin testaruda -- gen-cli-docs > /tmp/testaruda-cli-doc-check.md
    @diff docs/cli.md /tmp/testaruda-cli-doc-check.md > /dev/null \
        && echo "✅ docs/cli.md is up to date" \
        || (echo "❌ docs/cli.md is out of date — run 'just doc-cli' to regenerate" && exit 1)