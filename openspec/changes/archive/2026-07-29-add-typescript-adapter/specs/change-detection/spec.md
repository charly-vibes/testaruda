## MODIFIED Requirements

### Requirement: TIA-CHG-009 — Init-time language detection

The adapter SHALL probe for the following project files in well-known locations:

- Python: `pyproject.toml`, `setup.py`, `setup.cfg`
- Rust: `Cargo.toml`
- JavaScript/TypeScript: `package.json`, `vitest.config.ts`, `jest.config.ts`, `tsconfig.json`
- Go: `go.mod`
- Clojure: `deps.edn`, `project.clj`

When a marker is found (and no other project file takes priority), the core SHALL set the default adapter to the corresponding binary.

Priority order (first match wins, most specific first):
1. `Cargo.toml` → Rust (`testaruda-adapter-rust`)
2. `vitest.config.ts` or `jest.config.ts` → TypeScript (`testaruda-adapter-typescript`)
3. `package.json` (with `vitest` or `jest` in devDependencies) → TypeScript (`testaruda-adapter-typescript`)
4. `pyproject.toml`, `setup.py`, `setup.cfg` → Python (`testaruda-adapter-python`)
5. `deps.edn`, `project.clj` → Clojure (`testaruda-adapter-clojure`)
6. `go.mod` → Go adapter (when implemented)
7. `package.json` (without `vitest`/`jest`) → fallback (no auto-detection)

#### Scenario: TypeScript project detection via vitest.config.ts
- **GIVEN** a directory with `vitest.config.ts` but no `Cargo.toml` or `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-typescript"`

#### Scenario: TypeScript project detection via jest.config.ts
- **GIVEN** a directory with `jest.config.ts` but no `vitest.config.ts`, `Cargo.toml`, or `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-typescript"`

#### Scenario: TypeScript project detection via package.json devDependencies
- **GIVEN** a directory with `package.json` containing `"vitest"` or `"jest"` in `devDependencies` but no `vitest.config.ts`, `jest.config.ts`, `Cargo.toml`, or `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-typescript"`

#### Scenario: TypeScript config wins over Python
- **GIVEN** a directory with both `vitest.config.ts` and `pyproject.toml`
- **WHEN** `init` is invoked
- **THEN** the generated `testaruda.toml` SHALL have `default = "testaruda-adapter-typescript"`
- **AND** the Python adapter SHALL still be registered as a secondary extension mapping for `.py` files