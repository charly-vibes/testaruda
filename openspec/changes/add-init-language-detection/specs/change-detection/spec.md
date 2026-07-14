## ADDED Requirements

### Requirement: TIA-CHG-009 — Init-time language detection

When the `init` command is invoked, the core SHALL probe the current directory for well-known project files to detect the project's primary language. The detected language SHALL determine the default adapter in `testaruda.toml`. When no known project file is found, the core SHALL fall back to `testaruda-adapter-rust`. A user-supplied adapter configuration SHALL always take precedence over auto-detection.

Well-known project files to probe:
- Python: `pyproject.toml`, `setup.py`, `setup.cfg`
- Rust: `Cargo.toml`
- JavaScript/TypeScript: `package.json`
- Go: `go.mod`

#### Scenario: Python project detection
- **GIVEN** a directory with `pyproject.toml` but no `Cargo.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-python"`

#### Scenario: Rust project detection
- **GIVEN** a directory with `Cargo.toml` but no `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-rust"`

#### Scenario: Unknown project fallback
- **GIVEN** a directory with no known project files
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-rust"`

#### Scenario: User override takes precedence
- **GIVEN** a Python project
- **WHEN** the user explicitly specifies adapter config during `init`
- **THEN** the user's configuration SHALL take precedence over auto-detection