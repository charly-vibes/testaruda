# Change: Add JuliaLang adapter

## Why

Julia is a growing language in scientific computing, data science, and machine learning — domains where test selection is especially valuable due to long-running test suites. The core currently ships with default adapter mappings for Rust (`.rs`) and Python (`.py`), but no Julia support. Users working on Julia projects must manually configure the adapter binary and extension mapping, and `testaruda init` cannot auto-detect Julia projects.

Adding Julia as a first-class language removes friction for Julia users and aligns with the project's goal of language-agnostic test selection (TIA-PORT-001).

## What Changes

- **Core config defaults** — Add `.jl` → `testaruda-adapter-julia` to the default extension-to-adapter mapping in `AdapterConfig::default()`.
- **Project detection** — Add Julia project detection in `detect_project_language()` by checking for `Project.toml` (primary) and falling back to `test/runtests.jl` existence for non-standard projects.
- **Init template** — Update `write_default()` to include the `.jl` mapping in the generated `testaruda.toml`.
- **Discover excludes** — Add Julia-specific artifact names (`Manifest.toml`, `JuliaArtifacts`) to the default discover exclude list to avoid walking generated content.
- **External adapter** — The `testaruda-adapter-julia` binary (implemented in Julia, separate project) that implements the standard JSON protocol (TIA-ADAPT-001 through TIA-ADAPT-013) for Julia's `Test.jl` framework.

## Impact

- Affected specs: `adapter-protocol` — adds 3 new requirements (TIA-ADAPT-014 through TIA-ADAPT-016)
- Affected code: `src/config.rs` (defaults, detection, template, excludes)
- External: New `testaruda-adapter-julia` project/repo