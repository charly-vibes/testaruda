# testaruda

**testaruda** is a language-agnostic test selection engine. Given a code change,
it computes the set of tests that must run — modeled as the transpose of a
provenance-semiring dependency relation, evaluated incrementally, under a
recall-first soundness invariant.

## Quick Start

```bash
cargo install testaruda

# Initialize the store
testaruda init

# Discover tests through configured adapters
testaruda discover

# Select tests affected by uncommitted changes
testaruda select

# Select tests between two revisions
testaruda select --base main --head feature-branch

# Ingest run results
testaruda ingest results.json
```

## Key Concepts

- **Transpose selection**: affected = Δ · (D*)ᵀ ∪ always_run
- **Soundness**: over-approximation — missed selections are bugs
- **Semiring abstraction**: Boolean (selection), Viterbi (confidence), Tropical (distance)
- **Incrementality**: the active path scopes evaluation to changed components;
  component-cache primitives exist but are not yet integrated into selection

## Continue

- Follow the complete [Getting Started](getting-started.md) tutorial.
- Look up commands in the [CLI Reference](cli.md).
- Configure language adapters and safety rules in [Configuration](configuration.md).
- Integrate structured output using [Agent Mode](agent-mode.md).
