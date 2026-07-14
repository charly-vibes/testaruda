# testaruda v0.1.0 — Usage Report

**Date:** 2026-07-14
**Project:** cositos (Python, pytest-based, 22 source files, 21 test files)
**Repo:** https://github.com/charly-vibes/testaruda
**Installed from:** `cargo install --path .` at `277155c` (local clone)
**Source analyzed at (local clone):** `277155c`

**What is testaruda?** A language-agnostic test selection engine. Given a code change
(git diff or file list), it computes the minimal test set that needs to re-run to
validate that change. It uses a pluggable **adapter** architecture — language-specific
binaries (`testaruda-adapter-python`, `testaruda-adapter-rust`) that communicate with
the engine via JSON over stdin/stdout. Adapters translate project test structures into
a shared **dependency graph** stored in SQLite (`store.db`). Selection is evaluated
using provenance-semiring analysis (embedded Datalog via Ascent) to determine which
tests are affected by a change, with a recall-first soundness guarantee.

## Key Findings

> **CRITICAL [DRAFT-001]:** Static-deps directionality bug — the Python adapter records
> edges from test files *to* the source files they import, but selection needs the reverse
> (from changed source *to* depending tests). `select` always falls back to "run all tests."

> **HIGH [DRAFT-002]:** The `--files` flag on `testaruda discover` is silently ignored.
> The discover pipeline always walks the full project tree via `walkdir`.

> **HIGH [DRAFT-003]:** The Python adapter's discover walks into `.venv/` with no
> exclusion filters. 98 of 119 discovered tests came from vendored dependencies.

---

## Terminology

| Term | Meaning |
|------|---------|
| **Adapter** | A binary that translates project-specific test structures into testaruda's protocol. Spawned once per language. |
| **Discover** | Adapter command that finds all test files in the project and returns their `node_id` (file path). |
| **Content unit** | A changed source/config/fixture file with a blake3 fingerprint. |
| **Static-deps** | Adapter command that computes dependency edges from test items to the content units they exercise (via `import` analysis). |
| **Ingest** | The process of recording test run results (passed/failed, duration, coverage) back into the store to update the model. |
| **`run_adapter_pipeline`** | The `select` command's adapter invocation path — runs discover + static-deps on *changed files only*. |
| **`run_discover_pipeline`** | The `discover` command's adapter invocation path — walks the full project tree. |

---

## 1. Onboarding / init

**`testaruda init`** creates `testaruda.toml` and the `.testaruda/` store (SQLite DB
with tables for test items, content units, dependency edges, run history). Default
config maps `.rs` → `testaruda-adapter-rust`, `.py` → `testaruda-adapter-python`, with
`testaruda-adapter-rust` as the fallback default.

**Issue:** On a Python-only project, `testaruda discover` runs the default (Rust)
adapter and finds 0 tests. The user must manually edit `testaruda.toml` to change
`default = "testaruda-adapter-python"`.

**Action:** Have `init` auto-detect the project language (check for `pyproject.toml`
vs `Cargo.toml`), or default to the first configured adapter alphabetically rather
than hard-coding `testaruda-adapter-rust`.

---

## 2. Discover

**`testaruda discover`** walks the project tree, finds the first file matching a
registered extension, spawns the adapter, and calls the adapter's `discover` command.
The adapter walks `"."` from its own working directory.

### 2a. `.venv` / dependency test pollution

**Problem:** The Python adapter's `cmd_discover` walks `"."` with no ignore filters:

```rust
for entry in walkdir::WalkDir::new(".")
    .into_iter()
    .filter_map(|e| e.ok())
{
    // matches test_*.py and *_test.py — no exclusions
}
```

This picks up every `test_*.py` in `.venv/lib/python3.12/site-packages/` — in our case
**98 out of 119 discovered tests** came from vendored dependencies (jsonschema, mypy,
tornado, traitlets, etc.).

> **Context:** The `run_discover_pipeline` in `main.rs` does filter `target`, `.git`,
> and `node_modules`, but `.venv` is not in that list. The adapter itself has no
> exclusion mechanism.

**Action:** Add an exclusion pattern list to the adapter's discover walk (`.venv`,
`venv`, `__pycache__`, `.mypy_cache`, `.pytest_cache`, `build/`, `dist/`), or pass
the ignore list from the engine config. Note that this mainly affects projects with
inline virtualenvs; CI runners with separate venv paths may not hit this issue.

**Reproduction:**
```bash
cd /path/to/python/project
testaruda discover
# Output: "python-adapter discovered 119 test items"
# Expected: ~21 (project tests only)
# The extra 98 come from .venv/lib/python3.12/site-packages/
```

### 2b. `--files` flag silently ignored

**Problem:** The `--files` flag on `testaruda discover` is accepted by the CLI but
has no effect. `run_discover_pipeline` assigns it to `_delta` (underscore prefix =
intentionally unused):

```rust
pub fn run_discover_pipeline(
    store: &testaruda::Store,
    registry: &testaruda::adapter::AdapterRegistry,
    _delta: &testaruda::ChangeSet,  // ← never consulted
) -> std::result::Result<(), String> {
    // walks all files via walkdir, ignores _delta
```

**Action:** Either make `--files` actually scope discovery (pass file paths to the
adapter's discover command, or filter results server-side), or remove the flag to
avoid misleading users.

**Reproduction:**
```bash
testaruda discover --files src/main.py
# Still discovers all tests in the project, not just those related to main.py
```

### 2c. Adapter pipeline only processes changed files

The `select` command's `run_adapter_pipeline` iterates only over the **changed files**
in the delta, not the full project tree. This means:

- The adapter is spawned only when a changed file matches its extension.
- Static-deps runs only on those changed files.
- Test files that are NOT in the change set never have their import edges recorded.
- The store's dependency graph is always incomplete.

**Action:** `run_adapter_pipeline` should also run discover + static-deps on the full
project tree on first invocation (or cache the results of a prior `discover`), not
just on the changed file set.

---

## 3. Ingest

**`testaruda ingest <path>`** parses a JSON file and expects a `run_id` field plus a
`tests` array with per-test `id` (numeric), `outcome` (passed/failed), and `duration_ms`.

> **Important:** The numeric `id` values come from the store's `test_items` table
> (visible in `testaruda graph`), not from filenames or `node_id` strings. You must
> run `testaruda discover` first, then map test node_ids to numeric IDs from the graph
> output.

### 3a. Adapter-level vs CLI-level ingest — protocol disconnect

The Python adapter has its own `ingest` command (separate from the CLI `testaruda ingest`).
It accepts `params.run_output` as a raw string and parses pytest's `v` verbose output
looking for `PASSED`/`FAILED` line suffixes. The adapter's ingest returns
`runtime_edges: []` (always empty — `capabilities.runtime_edges: false` in handshake).

**Problem:** The CLI `ingest` command doesn't call the adapter's ingest — it goes
directly to `Store::ingest()`. Adapter-level ingest produces `per_test_results` and
`runtime_edges`, but there's no path to propagate those into the store's dependency
graph.

**Action:** Either (a) have the CLI ingest delegate to the adapter's ingest and use
the adapter's returned data to populate edges, or (b) deprecate the adapter-level
ingest if it's unused, and document the CLI-level format clearly.

### 3b. Ingest doesn't create dependency edges

After `testaruda ingest <run.json>`:

```
testaruda graph → content_units: 1 (the source file), edges: 0, run_history: 0
```

The ingest creates a content unit for a covered source file but creates **zero edges**
connecting test items to that content unit. The `Store::ingest` function only inserts
into `run_history` — it never reads coverage data or creates `dependency_edges`.

**Action:** Either (a) accept coverage metadata (e.g., a `coverage` field mapping
test IDs to files covered) in the ingest payload and create edges from it, or (b)
make the adapter's `ingest` produce runtime edges from traced coverage data and have
the CLI propagate them.

**Reproduction:**
```bash
# Run discover + ingest
testaruda discover
uv run pytest tests/test_model.py --cov=src/cositos --cov-report=json
# Manually build run.json with { run_id, tests: [{ id, outcome, duration_ms }] }
testaruda ingest run.json
testaruda graph | jq '.edges | length'
# Expected: > 0 (test_model.py depends on model.py, protocol.py, etc.)
# Actual: 0
```

---

## 4. Static-deps — the critical gap

The `static-deps` command in the Python adapter has a **fundamental directionality
issue** for the "source file changed → find affected tests" use case.

### Current behaviour

```rust
fn cmd_static_deps(cmd) {
    let changed_files = params["changed_files"];
    let tests_by_file = cmd_discover();  // all test files keyed by path

    for file in &changed_files {
        let test_ids = tests_by_file.get(file).unwrap_or_default();
        //       ^ only returns results if `file` IS a test file itself
        let imports = parse_python_imports(&content);
        for test_id in &test_ids {
            for _imp in &imports {
                // edge FROM test TO the file it imports
                edges.push({ "from": test_id, "to": file, ... });
            }
        }
    }
}
```

**Problem:** `tests_by_file.get(file)` only returns a result when the changed file
**is itself a test file**. If `src/cositos/model.py` is changed, `get()` returns
`None`, and **no edges are created**. The engine then falls back to "run all tests"
because it has no data to narrow the selection.

Even in the working case (changed test file), **only that one test file's imports
are recorded** — every other test file in the project never gets its dependencies
entered into the graph.

**Action:** The static-deps logic needs to do the **reverse lookup**: for each changed
source file, find all test files that import from it. Two approaches:

1. **Build the full import graph at discover time** — scan all .py files once, record
   which test files import which source files, store all edges in the initial discover
   pass. More work upfront but selection is a simple query.

2. **Reverse lookup at select time** — when a source file is in the changed set, scan
   all test files to find those that import the changed file. Lighter upfront but
   slower per selection.

Approach 1 is more scalable for repeated selections. The adapter could return both
test items and their import edges from the `discover` command in a single pass.

### Current static-deps also doesn't handle relative imports

`parse_python_imports` only handles two patterns at the top level of indentation:

- `import X` and `from X import Y` — absolute imports only
- Not handled: `from .module import ...`, `from ..package import ...`, conditional
  imports inside `try/except`, imports inside function bodies, dynamic `__import__()` calls

**Action:** Use a proper Python import parser (stdlib `ast` module with a Rust binding,
or a lightweight regex that handles relative imports) rather than line-based string
parsing.

**Reproduction:**
```bash
# Change a source file
echo -e "\n# new function\ndef new_helper(): pass" >> src/cositos/model.py
git diff > /tmp/change.diff
testaruda select --diff /tmp/change.diff
# Expected: tests/test_model.py (which imports from model.py)
# Actual: all 119 tests selected (could not narrow down)
```

---

## 5. Select — output formats

### Summary table

| Flag | Output | Known issue |
|------|--------|-------------|
| *(none)* | JSON `Selection` struct | Hard to read without jq |
| `--json` | `CiPlan` with exit codes (0/10/20) | Cannot combine with `--pre-edit` — silently conflicting |
| `--agent` | `testaruda-agent-v1` format | `reason_chain` is empty when no edges exist; `always_run: true` indistinguishable from "computed affected" |
| `--pre-edit` | Human-readable blast radius | Good UX, works as described |

**Detailed notes:**

- **`--json`** outputs exit codes per TIA-CI: `0` = subset, `10` = full run needed,
  `20` = empty selection. Works correctly.
- **`--agent`** outputs structured JSON for LLM agent consumption. When edges are
  missing (the common case in v0.1.0), every test shows `always_run: true` with an
  empty `reason_chain`. This makes it impossible for an agent to distinguish
  "proven to be affected" from "couldn't prove safe, so run everything."
- **`--pre-edit`** prints a concise blast radius summary. Good for developer workflow.

**Action for `--agent`:** Include the `node_id` in each selected test's output
(currently only numeric `id`). This lets agents map back to test file paths without
a separate `graph` query. Add a `fallback_reason` field that explains *why* the
test is in `always_run` state (e.g., "no edge data available").

---

## 6. Soufflé oracle

`testaruda init` checks for `souffle` and warns if absent. The oracle generates
Datalog from the current store state and can optionally write it to a file for
validation via Soufflé's reference evaluator. This serves as a formal-verification
check that the embedded Ascent engine produces correct selection results.

```bash
testaruda oracle --program oracle.dl
souffle oracle.dl    # cross-validate against reference evaluator
```

This report did not test the oracle with an actual Soufflé installation.

---

## 7. Config file

`testaruda.toml` is minimal:

```toml
[adapters]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
default = "testaruda-adapter-rust"
```

Recommended additions:
- `[discover]` section with `exclude = ["venv/", ".venv/", "__pycache__/"]` — controls
  which directories the adapter's discover walk should skip.
- `[environment]` section for multi-env setups (CI vs local, different toolchains).
- `[select]` section with `confidence_threshold` — configurable via config rather than
  hard-coded `ONE`.

---

## 8. Summary of findings

### Working well
- `init` creates store and config cleanly.
- `discover` finds tests correctly (the .venv pollution is a quality issue, not a
  correctness failure — the tests DO exist on disk).
- `ingest` accepts JSON and records to `run_history`.
- `select --json`, `--agent`, `--pre-edit` all produce structurally valid output.
- `graph` exports the full store state faithfully.
- Adapter protocol (JSON over stdin/stdout, single-command-per-line) is clean,
  debuggable, and testable.

### Issues requiring attention

| # | Severity | Issue | Location | Action |
|---|----------|-------|----------|--------|
| 1 | **CRITICAL** | Static-deps directionality creates forward (test→source) edges instead of the reverse (source→test) needed for selection | `adapter-python.rs` `cmd_static_deps` | Reverse the lookup direction, or build full import graph at discover time |
| 2 | **HIGH** | `.venv` / `build/` pollution in adapter's discover walk | `adapter-python.rs` `cmd_discover` | Add exclusion patterns (`.venv`, `__pycache__`, etc.) |
| 3 | **HIGH** | `--files` flag on `discover` silently ignored | `main.rs` `run_discover_pipeline` | Either implement file-scoped discovery or remove the flag |
| 4 | **HIGH** | Adapter pipeline in `select` only processes changed files, never full project | `main.rs` `run_adapter_pipeline` | Run discover + static-deps on full project tree before selection |
| 5 | **MEDIUM** | CLI `ingest` and adapter `ingest` are disconnected — no path for adapter's per-test results to populate the store | `store.rs` `ingest` vs `adapter-python.rs` `cmd_ingest` | Unify the two ingest paths or document the gap |
| 6 | **MEDIUM** | `--agent` output lacks `node_id` and `fallback_reason` fields, making it hard for agents to interpret results | `main.rs` agent output format | Add `node_id` and `fallback_reason` to agent output |
| 7 | **LOW** | `parse_python_imports` doesn't handle relative imports (`from .module import ...`) | `adapter-python.rs` | Use a proper Python import parser |
| 8 | **LOW** | `--json` and `--pre-edit` flags conflict silently | `main.rs` CLI argument handling | Add mutual exclusion validation |

### Nice-to-haves
- `init` auto-detection of project language (check for `pyproject.toml` / `Cargo.toml`)
- `ingest` that accepts coverage data (pytest-cov JSON format) and builds edges from it
- `select --files` accepting multiple file paths (currently comma-separated string only)
- Cross-language dependency support (e.g., Python tests exercising a Rust binary)

---

## Reproduction checklist

To reproduce any finding in this report:

```bash
# Setup
git clone https://github.com/charly-vibes/testaruda
cargo install --path .

# Test project
git clone https://github.com/sk/cositos  # or any Python pytest project
cd cositos
testaruda init
# Edit testaruda.toml: default = "testaruda-adapter-python"

# Discover
testaruda discover                 # Finds 119 tests (21 project + 98 .venv)
testaruda graph | jq '.edges | length'   # 0

# Run + ingest
uv run pytest tests/test_model.py --cov=src/cositos --cov-report=json
# Build run.json with numeric IDs from graph output
testaruda ingest run.json
testaruda graph | jq '.edges | length'   # Still 0

# Select
testaruda select --files src/cositos/model.py --json | jq '.selected_count'
# Expected: ~22 (only test_model.py)
# Actual: 119 (all)
```
