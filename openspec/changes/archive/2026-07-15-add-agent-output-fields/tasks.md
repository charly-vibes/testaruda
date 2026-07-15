## 1. Specification work
- [x] 1.1 Modify TIA-AGENT-001 in `agent-mode/spec.md` to include `node_id` and `fallback_reason` fields in per-test entries

## 2. Implementation
- [x] 2.1 Add `node_id` field to the `TestSelection` struct or equivalent output model — `SelectedTestInfo.node_id` in `src/agent.rs`
- [x] 2.2 Populate `node_id` from the store's test item lookup when serializing agent output — via `test_node_ids` map in `AgentOutput::from_selection`
- [x] 2.3 Add `fallback_reason` (Option<String>) to the per-test output entry — `SelectedTestInfo.fallback_reason`
- [x] 2.4 Populate `fallback_reason` when `always_run` is true due to missing edge data — in `AgentOutput::from_selection` with 3 cases: no deps, confidence floor, quarantined

## 3. Validation
- [x] 3.1 Update existing agent output test fixtures with new fields — `test_agent_output_format_serialization`, `test_agent_output_fallback_reason_no_deps`, `test_agent_output_fallback_reason_quarantined`, `test_agent_output_node_id_included`
- [x] 3.2 Verify `--agent` output is valid JSON with new fields — all 8 agent tests pass
- [x] 3.3 Verify backward compatibility (old consumers that ignore unknown fields still work) — `#[serde(default, skip_serializing_if = "Option::is_none")]` on `fallback_reason`