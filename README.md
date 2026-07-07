# testaruda

[![tracked with wai](https://img.shields.io/badge/tracked%20with-wai-blue)](https://github.com/charly-vibes/wai)

**testaruda** is a language-agnostic test selection engine. Given a code change, it computes the set of tests that must run — modeled as the transpose of a provenance-semiring dependency relation, evaluated incrementatively, under a recall-first soundness invariant.

## Quick Start

```bash
# Initialize the store
cargo run -- init

# Select tests affected by uncommitted changes
cargo run -- select

# Select tests between two revisions
cargo run -- select --base main --head feature-branch

# Ingest run results
cargo run -- ingest results.json

# Explain why a test was selected
cargo run -- explain <test-id>
```

## Architecture

testaruda is built on three layers:

1. **Core engine**: Ascent-embedded Datalog selection query (provenance-semiring K-relation)
2. **Store**: SQLite-backed persistence for the dependency graph
3. **CLI**: User-facing commands

Selection is computed by evaluating the change set Δ against the transpose of the transitive-closure of the dependency relation (TIA-SEL-001). The reference implementation uses **Ascent** with native lattice support for semiring instantiation.

## Requirements

See `docs/tia-srs-ears.md` for the full Software Requirements Specification (EARS notation, draft v0.2).

## Tools

This project uses:

| Tool | Purpose |
|------|---------|
| [wai](https://github.com/charly-vibes/wai) | Workflow tracking — know *why* it was built |
| [beads](https://github.com/gastownhall/beads) | Issue tracking |
| [openspec](https://github.com/gastownhall/openspec) | Specification-driven development |
| [pretender](https://github.com/charly-vibes/pretender) | Code quality checks |
| [dont](https://github.com/charly-vibes/dont) | Grounded claims and evidence |
| [espectacular](https://github.com/charly-vibes/espectacular) | Spec-test correspondence verification |

## License

Apache 2.0
