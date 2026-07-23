# testaruda

[![tracked with wai](https://img.shields.io/badge/tracked%20with-wai-blue)](https://github.com/charly-vibes/wai)

**testaruda** is a language-agnostic test selection engine. Given a code change,
it computes the set of tests that must run — modeled as the transpose of a
provenance-semiring dependency relation, evaluated incrementally, under a
recall-first soundness invariant.

## Installation

### Cargo

```bash
cargo install testaruda
```

### Homebrew (macOS & Linux)

```bash
brew tap charly-vibes/charly
brew install testaruda
```

### Scoop (Windows)

```powershell
scoop bucket add charly https://github.com/charly-vibes/scoop-charly.git
scoop install testaruda
```

### From source

```bash
git clone https://github.com/charly-vibes/testaruda
cd testaruda
cargo install --path .
# Installs testaruda, testaruda-adapter-rust, and testaruda-adapter-python
```

## Quick Start

```bash
# Install
cargo install testaruda

# Initialize the store and config
testaruda init

# Discover tests through the configured language adapters
testaruda discover

# Select tests affected by uncommitted changes
testaruda select
```

See [**Getting Started →**](docs/getting-started.md) for a full walkthrough
with all commands, or [**CLI Reference →**](docs/cli.md) for detailed
option descriptions.

## Architecture

testaruda uses a three-layer architecture:

1. **Core engine**: Ascent-embedded Datalog selection query with provenance-semiring
   support for Boolean selection, Viterbi confidence scoring, and Tropical distance
2. **Store**: SQLite-backed persistence for the dependency graph
3. **Adapters**: Language-specific binaries that communicate via JSON over stdin/stdout

## Adapters

testaruda discovers tests by spawning language-specific adapter processes:

| Adapter | Language | Discovery Method |
|---------|----------|-----------------|
| `testaruda-adapter-rust` | Rust | Scans `#[test]` and `#[tokio::test]` attributes |
| `testaruda-adapter-python` | Python | Scans `test_*.py` / `*_test.py` files |
| `testaruda-adapter-julia` | Julia | Uses Testimonial.jl to discover `@testitem` tests |

The Rust and Python adapters ship with testaruda. The Julia adapter is installed
through Testimonial.jl. See [Getting Started](docs/getting-started.md) for setup
and [Configuration](docs/configuration.md) for adapter registration.

## Requirements

See `docs/tia-srs-ears.md` for the full Software Requirements Specification
(EARS notation, draft v0.3). The SRS describes normative target behavior; use
the user guides and generated CLI reference for the currently available surface.

Contributor setup and quality checks are documented in
[Contributing](docs/contributing.md).

## License

Apache 2.0
