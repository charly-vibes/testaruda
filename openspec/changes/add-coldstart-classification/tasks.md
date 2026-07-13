## 1. Specification work
- [x] 1.1 Add TIA-CHG-009 requirement in `change-detection/spec.md` with two scenarios (cold-start unresolved, idempotent re-classification)

## 2. Implementation
- [x] 2.1 In `src/store.rs`, ensure `load_selection_context` treats a content unit with `"unknown"` fingerprint as `unresolved` (cold-start guard — TIA-CHG-009). Confidence 0 propagates through the Ascent `impacted(c) <-- unresolved(c)` rule already in place.
- [x] 2.2 Verified: the `"unknown"` fingerprint check in `load_selection_context` applies every invocation — second call with no intervening change produces the same `unresolved` classification.

## 3. Validation
- [x] 3.1 Write an espectacular contract covering the cold-start scenario (`coldstart-unresolved.toml`)
- [ ] 3.2 Run `ah check` to confirm the contract detects the behavior (initially `no-tests-ran`, then passing after 2.1)
- [ ] 3.3 `cargo test` passes