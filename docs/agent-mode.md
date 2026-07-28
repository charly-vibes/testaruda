# Agent Mode

testaruda's **agent mode** (`--agent` flag on `select`) produces structured JSON
designed for consumption by LLM coding agents and automated tooling. This document
explains the output format and how to interpret each field.

## Usage

```bash
testaruda select --agent [--base main] [--head HEAD] [--files <list>]
```

The `--agent` flag conflicts with `--json` and `--pre-edit`. Agent mode also
implies `--ordering deterministic` to ensure stable output across runs
(TIA-AGENT-007).

## Output Format

Agent output is a single JSON object on stdout with the following top-level
fields:

| Field | Type | Description |
|-------|------|-------------|
| `format` | string | Protocol version identifier. Currently `"testaruda-agent-v1"`. |
| `summary` | object | Summary statistics (see below). |
| `changed_units` | array | Content units that changed in the code change set. |
| `selected` | array | Tests selected by the engine, with reason chains. |
| `skipped` | array | Candidate tests that were *not* selected, with exclusion reasons. |
| `coverage_gaps` | array | Symbols with no covering test (coverage gaps). |

### Summary (`summary`)

| Field | Type | Description |
|-------|------|-------------|
| `changed_count` | integer | Number of content units that changed. |
| `selected_count` | integer | Number of tests selected to run. |
| `candidate_count` | integer | Tests with direct stored dependencies on the changed units. Safety fallbacks can make this smaller than `selected_count`. |
| `has_coverage_gaps` | boolean | Whether any coverage gaps were detected. |

### Changed Unit (`changed_units[]`)

Each changed unit describes a file or symbol that the agent is proposing to modify.

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Internal store identifier. |
| `path` | string | File path relative to the project root. |
| `symbol` | string or null | Symbol name (function, class, etc.) if applicable. |
| `kind` | string | Content kind (e.g., `"source"`, `"config"`, `"test"`). |
| `unresolved` | boolean | True if the unit had no dependency data (cold-start or missing file). |

### Selected Test (`selected[]`)

Each selected test includes a **reason chain** that explains *why* the engine
chose it.

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Internal store identifier. |
| `node_id` | string or null | Adapter-assigned human-readable test identifier (e.g., `tests/test_model.py::test_model`). |
| `confidence` | number | Selection confidence, 0.0–1.0. |
| `distance` | integer or null | Minimum number of dependency hops from a changed unit. |
| `always_run` | boolean | Whether this test is a safety fallback: confidence is `1.0` and no dependency distance is available. |
| `quarantined` | boolean | Whether the test runs for monitoring but is excluded from pass/fail trust calculations. |
| `fallback_reason` | string or null | Human-readable explanation for *why* the test is in always-run state. Present when `always_run` is true. |
| `reason_chain` | array | List of witness edges that form the selection reason. |

#### Reason Edge (`reason_chain[]`)

Each edge explains one step in the dependency chain.

| Field | Type | Description |
|-------|------|-------------|
| `content_unit_id` | integer | Store ID of the content unit that connects to this test. |
| `origin` | string | Dependency origin: `"static"` (code import), `"runtime"` (execution trace), or `"manual"` (user-defined). |
| `path` | string or null | File path of the content unit (if resolved from store). |

### Skipped Test (`skipped[]`)

Tests that were candidates (they import from changed units) but were not deemed
affected.

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Internal store identifier. |
| `node_id` | string or null | Human-readable test identifier. |
| `exclusion_reason` | string | Why the test was excluded (e.g., "no transitive path to changed units"). |

### Coverage Gap (`coverage_gaps[]`)

Symbols that changed but no test covers them (TIA-AGENT-006).

| Field | Type | Description |
|-------|------|-------------|
| `symbol` | string | Symbol (function, class) that changed. |
| `file` | string | File path where the symbol is defined. |
| `changed_unit_id` | integer | Store ID of the changed content unit. |

## Design Decisions

1. **Versioned format string** — The `format` field allows future format changes
   without breaking existing consumers. Agents should check it and reject
   unknown versions.

2. **Reason chains** — Each selected test carries a `reason_chain` that traces
   back to changed units. This enables agents to make targeted decisions:
   *"test_user_model changed because it imports User from src/models.py which
   you modified"*.

3. **Coverage gaps** — Gaps alert the agent to untested changed code, so the
   agent can add tests or note the risk in its response.

4. **Skipped tests** — When an agent modifies a widely-imported utility, the
   engine may exclude transitive dependents with zero distance. The
   `exclusion_reason` explains why.

## Error Handling

If the store has not been initialized, the agent receives:

```
Error: Store has not been initialized. Run `testaruda init` first.
```

If the change set cannot be computed (e.g., invalid git ref), the agent receives
a descriptive error on stderr and a non-zero exit code.

## Pre-Edit Mode

The `--pre-edit` flag (TIA-AGENT-005) emits a simpler JSON format for
proposed (not yet committed) changes:

```
testaruda select --pre-edit [--files <list>]
```

Output format: `testaruda-pre-edit-v1` with `changed_files` and
`selected_tests` arrays plus summary stats. Its contract is published in
`schemas/pre-edit-output-v1.json`. The full agent-mode contract is published
separately in `schemas/agent-output.schema.json`.
