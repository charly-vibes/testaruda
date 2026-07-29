# Configuration

testaruda uses a `testaruda.toml` file in the project root.

```toml
# Select every test in a component when its minimum reachable confidence falls
# below this value. Default: 0.5.
confidence_threshold = 0.5

[adapters]
# Adapter used when no extension mapping matches.
default = "testaruda-adapter-rust"

[adapters.extensions]
# Adapter binaries must be on PATH or specified with absolute paths.
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
".jl" = "testaruda-adapter-julia"
".ts" = "testaruda-adapter-typescript"
".tsx" = "testaruda-adapter-typescript"
".mts" = "testaruda-adapter-typescript"
".cts" = "testaruda-adapter-typescript"
".clj" = "testaruda-adapter-clojure"
".cljs" = "testaruda-adapter-clojure"
".cljc" = "testaruda-adapter-clojure"
".cs" = "titi testaruda-adapter"         # command string: shell-split into (titi, testaruda-adapter)
".fs" = "titi testaruda-adapter"         # see TIA-ADAPT-024
".vb" = "titi testaruda-adapter"
".csproj" = "titi testaruda-adapter"
".sln" = "titi testaruda-adapter"
".slnx" = "titi testaruda-adapter"

[must_run]
# Map a changed-path glob to test node IDs that must run.
"ci/*" = ["ci-contract-tests"]
".github/**" = ["workflow-tests"]

[periodic_full_run]
# Hours between mandatory full-suite runs. Zero disables the trigger.
interval_hours = 168

[environment]
# Scope runtime observations to this named environment.
name = "default"

[discover]
# Directory or file names excluded from adapter discovery walks.
exclude = ["target", ".git", "node_modules", ".venv", "venv",
           "__pycache__", ".mypy_cache", ".pytest_cache",
           "build", "dist", ".tox"]
```

The example shows every supported setting. `testaruda init` writes the adapter
and discovery sections, selecting a default adapter from project markers such
as `Cargo.toml`, `pyproject.toml`, or `Project.toml`; omitted sections retain the
defaults below. If the file is absent, testaruda uses in-memory defaults. The
store location is fixed at `.testaruda/`; it is not currently configurable.

## Adapters

Rust and Python adapters are installed with testaruda. The Julia adapter is
provided by [Testimonial.jl](https://github.com/sashakile/Testimonial.jl); see
[Getting Started](getting-started.md#optional-julia-adapter) for installation.

The Julia adapter discovers `@testitem` tests (ReTestItems/TestItems.jl) and
`@testset` blocks (Base.Test). In a project that mixes `@testitem` with plain
`@test` blocks, both the `@testitem` and `@testset` tests are available for
selection. For files with no test blocks, a file-level fallback is used.

### .NET adapter (titi)

The .NET adapter is provided by [titi](https://github.com/charly-vibes/titi).
Unlike the language-native adapters, titi is invoked through a multi-token
command string: `titi testaruda-adapter`. This is configured by setting the
adapter binary to `"titi testaruda-adapter"` in the extension mapping.
testaruda's shell-split infrastructure (TIA-ADAPT-024) automatically splits the
command string into the binary (`titi`) and its argument (`testaruda-adapter`).

```toml
[adapters.extensions]".cs" = "titi testaruda-adapter"
```

When `titi` is not on `PATH`, testaruda falls back to full-suite selection per
TIA-ADAPT-012. The .NET adapter is opt-in — no automatic detection is attempted.

## Defaults

| Setting | Default |
|---|---|
| `confidence_threshold` | `0.5` |
| `adapters.default` | `testaruda-adapter-rust` when no project marker is detected |
| `periodic_full_run.interval_hours` | `0` (disabled) |
| `environment.name` | `default` |
| `discover.exclude` | The list shown above |
| `must_run` | No rules |
