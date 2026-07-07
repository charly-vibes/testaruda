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
- **Incrementality**: change-scoping + content-addressed component cache

## Tools

| Tool | Purpose |
|------|---------|
| wai | Workflow tracking |
| beads | Issue tracking |
| openspec | Spec-driven development |
| pretender | Code quality checks |
| dont | Grounded claims |
| espectacular | Spec-test verification |