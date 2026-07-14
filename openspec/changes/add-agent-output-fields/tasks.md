## 1. Specification work
- [ ] 1.1 Modify TIA-AGENT-001 in `agent-mode/spec.md` to include `node_id` and `fallback_reason` fields in per-test entries

## 2. Implementation
- [ ] 2.1 Add `node_id` field to the `TestSelection` struct or equivalent output model
- [ ] 2.2 Populate `node_id` from the store's test item lookup when serializing agent output
- [ ] 2.3 Add `fallback_reason` (Option<String>) to the per-test output entry
- [ ] 2.4 Populate `fallback_reason` when `always_run` is true due to missing edge data

## 3. Validation
- [ ] 3.1 Update existing agent output test fixtures with new fields
- [ ] 3.2 Verify `--agent` output is valid JSON with new fields
- [ ] 3.3 Verify backward compatibility (old consumers that ignore unknown fields still work)