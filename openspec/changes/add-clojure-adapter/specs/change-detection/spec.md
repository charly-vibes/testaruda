## MODIFIED Requirements

### Requirement: TIA-CHG-009 — Init-time language detection

The adapter SHALL probe for `deps.edn` and `project.clj` in addition to the
existing markers. When `deps.edn` or `project.clj` is found (and no other
project file takes priority), the core SHALL set the default adapter to
`testaruda-adapter-clojure`.

Well-known project files to probe:
- Python: `pyproject.toml`, `setup.py`, `setup.cfg`
- Rust: `Cargo.toml`
- JavaScript/TypeScript: `package.json`
- Go: `go.mod`
- Clojure: `deps.edn`, `project.clj`

#### Scenario: Clojure project detection via deps.edn
- **GIVEN** a directory with `deps.edn` but no `Cargo.toml` or `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-clojure"`

#### Scenario: Clojure project detection via project.clj
- **GIVEN** a directory with `project.clj` but no `deps.edn`, `Cargo.toml`,
  or `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-clojure"`