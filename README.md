> *"Se me acaba el argumento y la metodología*
> *Cada vez que se aparece frente a mí tu anatomía*
> *Porque este amor ya no entiende de consejos, ni razones*
> *Se alimenta de pretextos y le faltan pantalones"*
> — Shakira

# testaruda

> **Why:** test suites grow monotonically while the code they cover moves —
> agents (and humans) either run everything (slow) or guess (flaky).
> testaruda derives the minimal test set from what actually changed,
> language-agnostically, via adapters.
> **Status:** [beta](docs/src/status.md) · select/doctor/explain shipped, five adapters · [Motivation & design](docs/src/index.md) · [charly-vibes Tool Ecosystem](https://charly-vibes.github.io/dulce-de-leche/ecosystem-map.html)
[![tracked with wai](https://img.shields.io/badge/tracked%20with-wai-blue)](https://github.com/charly-vibes/wai)

**testaruda** is a language-agnostic test selection engine. Given a code change,
it computes the set of tests that must run — modeled as the transpose of a
provenance-semiring dependency relation, evaluated incrementally, under a
recall-first soundness invariant.

## Installation

### Binary (curl)

```bash
V=$(basename "$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
  https://github.com/charly-vibes/testaruda/releases/latest)" | sed 's/^v//')
TGT="$(uname -s | tr '[:upper:]' '[:lower:]')_$(uname -m | sed 's/^x86_64$/amd64/; s/^aarch64$/arm64/')"
curl -fsSL "https://github.com/charly-vibes/testaruda/releases/download/v${V}/testaruda_${V}_${TGT}.tar.gz" | tar xz
chmod +x testaruda testaruda-adapter-rust testaruda-adapter-python && sudo mv testaruda testaruda-adapter-rust testaruda-adapter-python /usr/local/bin/
```

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

See [**Getting Started →**](docs/src/getting-started.md) for a full walkthrough
with all commands, or [**CLI Reference →**](docs/src/cli.md) for detailed
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
| `testaruda-adapter-clojure` | Clojure | Scans `deftest` and `deftest-` forms via tree-sitter |
| `testaruda-adapter-typescript` | TypeScript | Scans `vitest.config.*` or `jest.config.*` for test files |

The Rust, Python, TypeScript, and Clojure adapters ship with testaruda. The Julia adapter is installed
through Testimonial.jl. See [Getting Started](docs/src/getting-started.md) for setup
and [Configuration](docs/src/configuration.md) for adapter registration.

### TypeScript

The TypeScript adapter (`testaruda-adapter-typescript`) discovers tests by detecting vitest
or jest configuration files and scanning for test file patterns.

**Prerequisites:**
- `npx vitest` or `npx jest` available (the adapter delegates to the runner)
- The adapter is built automatically with `cargo build`

**Configuration in `testaruda.toml`:**
```toml
[adapters.extensions]
".ts" = "testaruda-adapter-typescript"
".tsx" = "testaruda-adapter-typescript"
".mts" = "testaruda-adapter-typescript"
".cts" = "testaruda-adapter-typescript"
```

**Detecting TypeScript projects:**
testaruda auto-detects TypeScript projects by looking for `vitest.config.ts`,
`vitest.config.js`, `jest.config.ts`, or `jest.config.js` at the project root,
or a `package.json` with `vitest` or `jest` in `devDependencies`.

> **Note:** Monorepo projects may have vitest/jest configs in subpackages.
> Use the `--test-dir` flag with `stress-test.sh` to target a specific subpackage.

### Clojure

The Clojure adapter (`testaruda-adapter-clojure`) discovers tests by parsing `.clj`,
`.cljs`, and `.cljc` files with tree-sitter and running a query for `deftest`/
`deftest-` forms. It supports both deps.edn (Cognitect) and project.clj (Leiningen)
project configurations.

**Prerequisites:**
- Clojure CLI tools (`clj`) or Leiningen (`lein`) on PATH
- The adapter is built automatically with `cargo build`

**Configuration in `testaruda.toml`:**
```toml
[adapters.extensions]
".clj" = "testaruda-adapter-clojure"
".cljs" = "testaruda-adapter-clojure"
".cljc" = "testaruda-adapter-clojure"
```

**Detecting Clojure projects:**
testaruda auto-detects Clojure projects by looking for `deps.edn` or
`project.clj` in the project root.

## Requirements

See `docs/src/tia-srs-ears.md` for the full Software Requirements Specification
(EARS notation, draft v0.3). The SRS describes normative target behavior; use
the user guides and generated CLI reference for the currently available surface.

Contributor setup and quality checks are documented in
[Contributing](docs/src/contributing.md).

## License

Apache 2.0
