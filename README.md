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
cargo build --release
# Installs: testaruda, testaruda-adapter-rust, testaruda-adapter-python
```

## Quick Start

```bash
# Initialize the store and config
testaruda init

# Discover tests via language adapters
testaruda discover

# Select tests affected by uncommitted changes
testaruda select

# Select tests between two revisions
testaruda select --base main --head feature

# Machine-readable JSON plan (for CI)
testaruda select --json

# Shadow mode: compute but signal "run all tests"
testaruda select --shadow

# Ingest run results
testaruda ingest results.json

# Explain why a test was selected
testaruda explain <test-id>
```

## Exit Codes

When used in CI pipelines:

| Code | Meaning |
|------|---------|
| 0 | Selection computed — run the selected tests |
| 10 | Low confidence or shadow mode — run all tests |
| 20 | Empty selection — safe to skip |
| 1+ | Error (distinct from 10 and 20) |

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
| `testaruda-adapter-rust` | Rust | Scans `#[test]` attributes |
| `testaruda-adapter-python` | Python | Scans `test_*.py` / `*_test.py` files |

Configure adapters in `testaruda.toml`:

```toml
[adapters]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
default = "testaruda-adapter-rust"
```

## Requirements

See `docs/tia-srs-ears.md` for the full Software Requirements Specification
(EARS notation, draft v0.2).

## Tools

This project uses:

| Tool | Purpose |
|------|---------|
| [wai](https://github.com/charly-vibes/wai) | Workflow tracking |
| [beads](https://github.com/gastownhall/beads) | Issue tracking |
| [openspec](https://github.com/gastownhall/openspec) | Spec-driven development |
| [pretender](https://github.com/charly-vibes/pretender) | Code quality checks |
| [dont](https://github.com/charly-vibes/dont) | Grounded claims |
| [espectacular](https://github.com/charly-vibes/espectacular) | Spec-test verification |

## License

Apache 2.0