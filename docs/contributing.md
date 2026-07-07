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
```

## Project Conventions

- Rust 2021 edition
- No `unwrap()` in library code
- Reference SRS requirement IDs in doc comments
- Commit messages: conventional commits