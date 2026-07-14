# Change: Add `node_id` and `fallback_reason` to agent output

## Why

The `--agent` output format provides numeric test IDs but no `node_id` (file path), forcing agents to run a separate `graph` query to map IDs back to test files. Additionally, when dependency edges are missing (the common case in v0.1.0), every test shows `always_run: true` with an empty `reason_chain`, making it impossible for agents to distinguish "proven to be affected" from "couldn't prove safe, so run everything."

## What Changes

- **MODIFY** TIA-AGENT-001 — Add `node_id` (string file path) to each per-test entry in agent output
- **MODIFY** TIA-AGENT-001 — Add `fallback_reason` (optional string) explaining why a test is in `always_run` state when edges are missing
- Update the CLI agent output serialization in `main.rs` to include these fields

## Impact

- Affected specs: `agent-mode` (AGENT-001)
- Affected code: `main.rs` — agent output serialization
- Non-breaking: the new fields are additive; existing consumers that ignore unknown JSON fields will be unaffected