# CLI Reference

## Usage

```
testaruda <COMMAND>
```

## Commands

### `init`

Initialize the store and configuration in the current project.

```
testaruda init
```

Creates `.testaruda/store.db` (SQLite schema) and `testaruda.toml` (default adapter config).

### `select`

Select affected tests from a code change. Runs the **adapter pipeline** (discovers tests, computes static dependencies) before running the engine query.

```
testaruda select [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--base <REF>` | Base revision (git ref) |
| `--head <REF>` | Head revision (git ref) |
| `--files <LIST>` | Explicit changed-file list (comma-separated) |
| `--shadow` | Shadow mode (TIA-CI-007): compute but signal full run |
| `--json` | Emit machine-readable `CiPlan` JSON (TIA-CI-006) |

If no options are provided, uses uncommitted working tree changes.

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Selection complete — run the selected tests |
| 10 | Low confidence or shadow mode — run all tests |
| 20 | Empty selection — safe to skip |
| 1+ | Error (distinct from 10 and 20) |

### `discover`

Discover tests via configured adapters. Scans the project for test files and stores them in the database.

```
testaruda discover [--files <LIST>]
```

| Option | Description |
|--------|-------------|
| `--files <LIST>` | Scope discovery to specific files (comma-separated) |

### `ingest`

Ingest test run results to update the dependency model.

```
testaruda ingest <PATH>
```

Where `<PATH>` is a JSON file with run results (`{"run_id": "...", "tests": [...]}`).

### `graph`

Export the current dependency graph as JSON.

```
testaruda graph
```

### `explain`

Explain why a test was or was not selected.

```
testaruda explain <TEST_ID> [--change <REF>]
```

### `oracle`

Run Soufflé Datalog oracle for cross-validation.

```
testaruda oracle --program <PATH>
```