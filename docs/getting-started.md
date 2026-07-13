# Getting Started

## Prerequisites

- Rust 1.75+
- Git (for change detection)

## Installation

```bash
cargo install testaruda
```

Or from source:

```bash
git clone https://github.com/charly-vibes/testaruda
cd testaruda
cargo build --release
# Binaries: testaruda, testaruda-adapter-rust, testaruda-adapter-python
cp target/release/testaruda* ~/.local/bin/
```

## First Run

```bash
# Initialize store + write default config
testaruda init

# Discover tests via adapters (scans project for #[test], test_*.py, etc.)
testaruda discover

# Select tests affected by uncommitted changes
testaruda select

# Select tests between two revisions
testaruda select --base main --head feature

# Machine-readable JSON plan
testaruda select --json

# Shadow mode: compute selection but signal "run all tests"
testaruda select --shadow

# Explicit file list
testaruda select --files "src/lib.rs,src/main.rs"
```

## Exit Codes

When used in CI pipelines:

| Code | Meaning |
|------|---------|
| 0 | Selection computed — run the selected set |
| 10 | Full run required (low confidence or shadow mode) |
| 20 | No tests affected — safe to skip |
| 1+ | Error (unrelated to 10 or 20) |

## Adapters

testaruda discovers tests by spawning language-specific adapter binaries:

```
testaruda (core) ←─── JSON over stdin/stdout ───→ testaruda-adapter-rust
                                              ───→ testaruda-adapter-python
```

Configure adapters in `testaruda.toml`:

```toml
[adapters]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
default = "testaruda-adapter-rust"
```