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
# Command to invoke adapters
command = "testaruda-adapter"
timeout = 30

[always_run]
# Path globs that always trigger affected tests
patterns = ["ci/*", ".github/**"]
```