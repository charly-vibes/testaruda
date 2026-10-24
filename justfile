# Testaruda CLI — language-agnostic test selection engine

# Default: list all recipes
default:
    @just --global-justfile --list

# Build (debug)
build:
    cargo build

# Build (release), whole workspace — shipped adapter binaries are release
# artifacts (testaruda-h7bv.1)
build-release:
    cargo build --workspace --release

# Alias for backward compatibility
release: build-release

# Run all tests, whole workspace — shipped adapters gate like the engine
# (testaruda-h7bv.1); parallel-safe, adapter children get explicit cwd

# TDD loop (most obvious entry, DDL test-runner standard — see DDL-85p in
# the dulce-de-leche repo): testaruda exec — select the tests the working
# diff affects, run only those, ingest, calibrate. Tolerance contract:
# exit 0/10/20 green; NOT CALIBRATED with 0 actual failures is a vacuous
# pass (tambor-cpd policy), with failures it blocks. Local store
# (.testaruda/, gitignored) initializes on first use; a stale store
# self-heals (wild-z56).
test:
    #!/usr/bin/env bash
    set -uo pipefail
    # Init gates on the STORE, not the dir: any command that merely opens
    # the default store creates an empty .testaruda/ (Store::open is
    # create_dir_all), and a dir-gate skipped init in CI while the schema
    # never existed → exec died with "Store has not been initialized"
    # (testaruda-3x56).
    if [ ! -d .testaruda ] || [ ! -f .testaruda/store.db ]; then
      testaruda init || { echo "❌ testaruda init failed" >&2; exit 1; }
    fi
    code=0
    testaruda exec || code=$?
    case "$code" in
      0|10|20) exit 0 ;;
      4)
        echo "⚠️ runner exit 4 — pruning stale local calibration store and retrying once (wild-z56)" >&2
        rm -rf .testaruda
        testaruda init || { echo "❌ testaruda re-init failed" >&2; exit 1; }
        testaruda exec ;;
      *) exit "$code" ;;
    esac

# Blast-radius preview: which tests a change WOULD affect — runs nothing.
# Useful before a refactor; optionally name files: just test-blast src/foo.rs
test-blast *files:
    #!/usr/bin/env bash
    if [ -z "{{files}}" ]; then
      testaruda select --pre-edit --human
    else
      testaruda select --pre-edit --human --files "{{files}}"
    fi

test-full:
    cargo test --workspace

# Run tests with output
test-v:
    cargo test --workspace -- --nocapture

# Lint (clippy with warnings denied), whole workspace
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Alias for lefthook
lint: clippy

# Check formatting
fmt-check:
    cargo fmt --check

# Pre-push checks (fast gate)
pre-push: fmt-check lint test gate
    @echo "✅ Pre-push checks passed"

# Full CI gate: fmt + lint + test + build + gate
ci: fmt-check lint test build-release gate check-claims
    @echo "✅ CI checks passed"


# Format code
fmt:
    cargo fmt

# Run pretender code quality checks (staged/diff only — see lefthook.yml)
check:
    pretender check src/

# Validate specs against tests with espectacular
ah:
    ah check

# Epistemic gate (dont-bpuo ADR): terminal check — exits 1 while any claim
# is Doubted, regardless of project mode. Wired into pre-commit (lefthook)
# and CI.
check-claims:
    dont prime

# Gate checks: pretender (src/ only) + ah
# Runs full scan in tiered mode (advisory for existing issues, catches regressions)
gate: check ah
    @echo "✅ Gate checks passed"

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
    @echo "Regenerating docs/src/cli.md from clap definitions..."
    @cargo run --bin testaruda -- gen-cli-docs > docs/src/cli.md
    @echo "✅ docs/src/cli.md regenerated"

# Check that CLI reference docs are in sync with --help
doc-cli-check:
    @echo "Checking CLI docs are in sync..."
    @cargo run --bin testaruda -- gen-cli-docs > /tmp/testaruda-cli-doc-check.md
    @diff docs/src/cli.md /tmp/testaruda-cli-doc-check.md > /dev/null \
        && echo "✅ docs/src/cli.md is up to date" \
        || (echo "❌ docs/cli.md is out of date — run 'just doc-cli' to regenerate" && exit 1)