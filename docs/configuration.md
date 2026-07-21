# Configuration

testaruda uses a `testaruda.toml` file in the project root.

```toml
[store]
path = ".testaruda"

[confidence]
# Threshold below which full-run fallback triggers
threshold = 0.5

[semiring]
# Default semiring for selection
default = "boolean"

[ci]
# Enable shadow mode (compute but don't gate)
shadow = false

# Periodic full-run interval in days
full_run_interval = 7

[adapters]
# Map file extensions to adapter binaries.
# Adapters must be installed on PATH or specified as absolute paths.
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
".jl" = "testaruda-adapter-julia"

# Default adapter when no extension matches
default = "testaruda-adapter-rust"

# Built-in adapters are Rust binaries compiled by cargo (testaruda-adapter-rust,
# testaruda-adapter-python). The Julia adapter lives in Testimonial.jl and is
# accessed via a shell wrapper — see getting-started.md for installation.

[always_run]
# Path globs that always trigger affected tests
patterns = ["ci/*", ".github/**"]
```