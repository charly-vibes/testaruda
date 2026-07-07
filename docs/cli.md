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

Creates a `.testaruda/` directory with the SQLite database schema.

### `select`

Select affected tests from a code change.

```
testaruda select [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--base <REF>` | Base revision (git ref) |
| `--head <REF>` | Head revision (git ref) |
| `--files <LIST>` | Explicit changed-file list (comma-separated) |

If no options are provided, uses uncommitted working tree changes.

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Selection computed — run selected set |
| 10 | Low confidence / fallback — run all tests |
| 20 | No tests affected — safe to skip |

### `ingest`

Ingest test run results to update the dependency model.

```
testaruda ingest <PATH>
```

Where `<PATH>` is a JSON file with run results.

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

Run Soufflé oracle for cross-validation.

```
testaruda oracle --program <PATH>
```