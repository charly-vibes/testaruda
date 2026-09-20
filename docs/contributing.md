# Contributing

## Workflow

This project uses spec-driven development via OpenSpec.

1. Check existing specs: `openspec list --specs`
2. Create a change proposal: see `openspec/AGENTS.md`
3. Implement following TDD
4. Validate: `openspec validate <change-id> --strict`
5. Archive after deployment

## Development

```bash
# Build
cargo build

# Test
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Run code quality checks
pretender check src/

# Run the epistemic gate (dont claims)
just check-claims
```

## Epistemic Gate (dont)

This repo keeps a real claim corpus in `.dont/` and has adopted the
repo-local epistemic gate per the dont-bpuo ADR (see
[dont's enforcement model](https://github.com/charly-vibes/dont/blob/main/docs/enforcement.md)):

- `just check-claims` runs `dont prime`, the terminal check — it exits 1
  while any claim in `.dont/` is `Doubted`, regardless of project mode.
- A lefthook **pre-commit** hook runs `just check-claims`; commits are
  rejected while a claim is doubted (run `dont undoubt <claim-id>` after
  resolving the doubt, or `dont flag` to verify with evidence).
- CI runs `check-claims` as part of `just ci`, so a doubted claim fails
  the pipeline.
- Register claims as you make them during development (see dont's
  [grounding workflow](https://github.com/charly-vibes/dont/blob/main/docs/grounding-workflow.md));
  unverified claims do not block in permissive
  mode, doubted ones always do.

## Project Conventions

- Rust 2021 edition
- No `unwrap()` in library code
- Reference SRS requirement IDs in doc comments
- Commit messages: conventional commits