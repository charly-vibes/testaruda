# Project Context

## Purpose

**testaruda** is a language-agnostic CLI whose single responsibility is to compute, from a code change, the set of tests that must run — modeled as the transpose of a provenance-semiring dependency relation, evaluated incrementally, under a recall-first soundness invariant.

Binary: `testaruda` · Config: `testaruda.toml` · License: Apache 2.0

## Tech Stack

- **Rust** (edition 2021)
- **Ascent** — embedded Datalog-style logic engine with lattice support for semiring instantiation
- **SQLite (rusqlite)** — persistence of the dependency graph and test history
- **clap** — CLI argument parsing
- **serde + serde_json** — serialization for the adapter protocol and output
- **rayon** — parallel per-component selection
- **blake3** — content fingerprinting
- **miette + thiserror + anyhow** — error handling

## Project Conventions

### Code Style
- Rust 2021 edition
- `rustfmt` for formatting
- `clippy` with warnings denied in CI
- No unwrap() in library code (tests/fixtures may justify it)
- Reference the SRS requirement IDs in doc comments

### Architecture Patterns

The system has three layers:
1. **Core engine** — Ascent-embedded selection query (the provenance-semiring K-relation)
2. **Store** — SQLite-backed persistence of the dependency graph
3. **CLI** — user-facing commands that bridge the two

Language/framework adapters communicate via JSON over stdin/stdout (TIA-ADAPT-001).

### Testing Strategy
- Unit tests for semiring operations and core logic
- Integration tests for the CLI against fixture files
- Shadow-mode verification against Soufflé oracle (TIA-ENG-011)
- Seeded-fault recall test for soundness invariant (TIA-VER-004)

### Key Design Decisions
- Reference engine: **Ascent** (in-process, lattice support, embeddable)
- Provenance oracle: **Soufflé** (out-of-process, for validation only)
- Cross-invocation incrementality: change scoping + content-addressed component cache
- Scale-up path: re-target to DBSP/Feldera for true IVM

## Important Constraints
- The core must contain no language- or framework-specific logic (TIA-ARCH-008)
- The core must not execute tests (TIA-ARCH-009)
- Soundness = over-approximation: missed selections are bugs (TIA-SAFE-001)
- Full provenance semiring is the master; concrete semirings are homomorphic images (TIA-ARCH-003)

## References
- [GKT07] Green, Karvounarakis, Tannen. *Provenance Semirings*, PODS 2007
- [BSalC] Mokhov, Mitchell, Peyton Jones. *Build Systems à la Carte*, ICFP 2018
- Spec: `docs/tia-srs-ears.md` (EARS notation, status: draft v0.2)
