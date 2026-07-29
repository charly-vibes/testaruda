# Change: Add cold-start content-unit classification rule

## Why

The specification (CHG-001–008) defines how the change set Δ is computed from fingerprints, but says nothing about how a content unit is classified on its **very first observation** — when no prior fingerprint of record exists. This gap is the root cause of I4 (changed-vs-unresolved flip on repeated invocation) from the testaruda evaluation (§3 I4, §8.4 S9). Currently, the first invocation of `select` sees a content unit absent from the store, inserts it, and the second invocation finds it present — producing different classifications for identical inputs.

Without specifying this case, the recall-first invariant (TIA-SAFE-001) has a hole: the store state determines classification, not the fingerprint comparison, and a cold-start unit can oscillate between `unresolved` and `changed` based on insertion-order timing. A requirement is needed to close this gap.

## What Changes

- **ADD** `TIA-CHG-009` — Cold-start content-unit classification: a content unit with no prior fingerprint of record SHALL be classified as `unresolved` (consistent every invocation), with an explicit idempotency constraint: repeated invocations with no intervening change SHALL produce the same classification.
- **Mention** `TIA-SAFE-002` (confidence-based fallback) and `TIA-CHG-005` (dependency fingerprint check) as the downstream consumers that rely on this classification being stable.

## Impact

- Affected specs: `change-detection` (new requirement CHG-009)
- Affected code: `src/engine.rs` — the `impacted(c)` derivation in the Datalog rule set may need an explicit cold-start rule
