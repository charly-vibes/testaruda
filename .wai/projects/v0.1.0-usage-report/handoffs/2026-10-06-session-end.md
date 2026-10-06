---
date: 2026-10-06
project: v0.1.0-usage-report
phase: review
---

# Production readiness evaluation

Verdict: NEEDS_HARDENING. Do not promote selective execution to the sole production CI gate until the P1 correctness and execution findings below are fixed. This is a bounded repository review of checkout aa0e402, not certification of all adapters or deployed release artifacts.

## Domain 1: Logic and correctness — FAIL

[RED-1.1] HIGH — src/store.rs:770-810 — Selection consumes the change baseline.
A selection request updates stored fingerprints before any tests execute. In an isolated warm SQLite fixture, the first select --files source.foo selected its dependent test; the same request then returned changed_count=0, selected_count=0, exit 20 without any execution. Failed execution likewise leaves the fingerprint advanced. Fix: tie baseline advancement to validated evidence and regression-test repeated selection and failed execution. Ticket: testaruda-yvyb.

[RED-1.2] HIGH — src/store.rs:687-725; src/commands.rs:225-239 — Conservative fallback is not wired to incomplete evidence.
SelectionContext.test_comp starts empty and is never populated by the store; confidence-based component fallback cannot resolve component membership. Unknown files become impacted units without dependency edges. StaticDepsResult.unresolved is discarded by the pipeline. In a warm store with a passing known test, an unknown changed source yielded zero selected tests; select --safe --human exited 0 saying no tests were affected. Fix: load membership and force conservative selection on unknown/failed adapter evidence. Ticket: testaruda-0bwk.

Passes: Ascent explicitly models reachability, always-run categories, confidence and distance; revision-range handling avoids comparing HEAD changes only against current working-tree fingerprints. Existing seeded-fault and recall tests passed in the non-Julia run.

## Domain 2: Failure modes and reliability — FAIL

[RED-2.1] HIGH — src/commands.rs:986-1050 — Missing execution can succeed.
A selected group is skipped when its adapter cannot spawn; run-args and runner-launch failures also return None. A missing group only triggers exit when the selection outcome was already nonzero. Reproduced select --safe with two known tests and one selected no-history test: exit 0 after logging that the selected test was skipped because its registered adapter was missing. Fix: distinguish successful execution from failure and reject incomplete execution independently of selection outcome. Ticket: testaruda-oeft.

[RED-2.2] HIGH — src/adapter.rs:293-345 — Configured IO timeout cannot interrupt blocking reads.
The deadline is checked before BufReader.read_line on an ordinary blocking child pipe. A live silent child or partial response can block indefinitely. Piped stderr is not drained concurrently and can block a verbose adapter. Source-confirmed; no timed process reproduction performed. Fix: bounded interruptible reads, drained stderr, kill/reap on deadline; test silent and partial-line adapters. Ticket: testaruda-v9ey.

[RED-2.3] MEDIUM — src/config.rs:159 — Invalid configuration silently becomes defaults.
load_or_default discards parse/read errors as well as a missing file. A malformed config can silently lose adapter mappings, environment and must-run rules. Source-confirmed. Fix: permit defaults only for absent configuration; report malformed existing configuration. Ticket: testaruda-kx1t.

[RED-2.4] MEDIUM — src/commands.rs:136,677,866 — Safe fallback depends on output mode and assumes Cargo.
Fallback unconditionally runs cargo test, including non-Rust projects. Safe handling of outcome 10/20 lives in the human emitter; JSON and agent output take their own exit paths. Non-TTY output defaults to JSON, so execution policy changes with output format. Fix: centralize safe policy before formatting and obtain language-appropriate full-suite commands. Ticket: testaruda-xcgs.

Passes: CI attempts result ingestion before propagating runner failure; native runners use argument vectors, and stored dependency inserts use SQL parameters.

## Domain 3: Security and attack surface — PARTIAL

No concrete injection exploit was established in this pass. The core uses rusqlite parameters and Command argument vectors. Configured adapter binaries are executable code and require the user's trust. Adapter responses can provide an empty runner_args vector, which is indexed directly at commands.rs:1082; validation should accompany execution hardening (testaruda-oeft).

Web authentication, CORS, browser tokens and AI prompt injection are not applicable to this local CLI. This review did not include a dependency vulnerability scan, malicious adapter fuzzing, or resource-exhaustion measurements; no security certification is implied.

## Domain 4: CI/CD and deployment — FAIL

[RED-4.1] HIGH — justfile:16-27; .github/workflows/release.yml — Release evidence does not cover every advertised adapter.
just ci uses cargo test, cargo clippy and cargo build without --workspace. Workspace-wide Clippy fails in the TypeScript/Clojure crates while root-only Clippy passes. Release jobs build/package only engine, Rust and Python binaries; README says TypeScript and Clojure ship with testaruda. Publication depends only on build jobs, not a quality/test job; the normal CI push trigger covers main, not version tags. Fix: gate the shipped workspace, explicitly couple publication to passing checks, and align artifacts with installation claims. Ticket: testaruda-h7bv.

Additional source observations: GitHub Actions use mutable version tags; archive downloads in CI lack checksum verification; release-wide contents:write exceeds build-job requirements. Scope these supply-chain improvements separately when implementing the release hardening ticket. Existing Homebrew publishing auth blocker: testaruda-ehse.

Passes: release archives include checksums and restore executable permissions; CI also runs formatting, claim and structural spec checks.

## Verification and limits

- cargo fmt --all --check: PASS.
- ah check: PASS, zero structural/execution findings. Contract tests were skipped by this command; this is not full spec-behavior verification.
- CCACHE_DISABLE=1 cargo clippy --all-targets --locked -- -D warnings: PASS for root package.
- CCACHE_DISABLE=1 cargo clippy --workspace --all-targets --locked -- -D warnings: FAIL on TypeScript/Clojure warnings and test imports/dead code.
- CCACHE_DISABLE=1 cargo test --workspace --locked: FAIL at Julia integration tests (1 passed, 21 failed). Direct handshake exposed a read-only ~/.julia compiled-cache directory; this is an environment limitation, not evidence of 21 Julia product regressions. Cargo stopped before later integration suites.
- CCACHE_DISABLE=1 cargo test --workspace --locked -- --skip julia_adapter: PASS, 485 passed, zero failed, 22 filtered. Exclusion is explicit and is not a Julia readiness claim.
- Initial build attempt failed because ccache wrote outside permitted directories; disabling ccache recovered the build.
- Three isolated fixtures reproduced baseline consumption, missing execution with success, and unresolved-change empty selection. Probe and logs are under /tmp/testaruda-prod-*; these temporary artifacts are not durable repository tests.
- No production performance benchmark, soak/concurrent-store workload, cross-platform artifact smoke test, dependency audit, or live hosted release verification was performed.

## Readiness criteria and handoff

Fix the four P1 runtime issues (testaruda-oeft, testaruda-yvyb, testaruda-0bwk, testaruda-v9ey) with failing regression tests first. Establish workspace/release gates (testaruda-h7bv), then repair configuration/fallback behavior (testaruda-kx1t, testaruda-xcgs). Validate real installed adapters and cross-platform artifacts, and run shadow comparisons against full suites before relying on selection as the sole CI gate. Preserve the recall-first invariant; use OpenSpec for any required semantic changes.

Quality ledger: Changed — seven Beads tickets and this handoff; no implementation changes. Verified — formatting, structural specs, root/workspace lint, workspace tests and isolated CLI probes as listed. Review — one-agent adversarial review of core change/store/adapter/command boundaries and CI/release configuration. Risks — four P1 runtime findings, incomplete Julia environment evidence and unmeasured operational/security behavior. Next — implement tickets through TDD, review, fix and commit cycles.

Pre-existing dirty pretender history and docs/src/https:/ artifacts were left untouched. No pushes or external messages were sent during evaluation.

## Issue-review edits applied

User authorized applying the review. Updated all seven readiness tickets with reproduction steps, explicit Must/Meter acceptance, source/spec references, base_commit/file metadata, file-header criteria, anti-goals and AFK/HITL boundaries. Preserved existing issue IDs.

- testaruda-0bwk now covers incomplete evidence; confidence fallback is testaruda-2bxl.
- testaruda-h7bv is an epic with workspace gate (testaruda-h7bv.1), complete release archives (testaruda-h7bv.2), and publication protection (testaruda-h7bv.3). Publication depends on the first two outcomes.
- testaruda-xcgs depends on full-suite contract decision testaruda-yo3i. The decision isolates protocol/schema approval without blocking unrelated AFK fixes.
- Shared-file notes coordinate edits without unnecessary serialization. Existing credential issue testaruda-ehse remains separate from local release checks.

Verification: bd lint passed for all 12 reviewed/created records; bd dep cycles found none; all 12 have nonempty files/base_commit/execution_mode metadata, source/anti-goal sections and Must criteria; nested shell Meter syntax checked. No implementation changes or regression tests executed during tracker editing. Test commands referring to planned tests are explicitly RED-after-test-creation and reject zero-test success.

Quality ledger: Changed — 7 existing issues, 5 new issues, dependency links, tracker export and this handoff. Verified — tracker lint, cycles, metadata/acceptance checks and Meter shell syntax. Review — five-pass findings mapped to issue edits; no new scope/dependency blockers beyond the explicit contract-decision prerequisite. Risks — tickets remain unimplemented; full-suite protocol decision and external credentials require later human input. Next — work ready AFK runtime slices through TDD/review/fix/commit; resolve testaruda-yo3i before claiming testaruda-xcgs. Git branch/commit/push remain unavailable under the current .git write restriction.

## Discovery-scope repair continuation

Created `fix/production-readiness` from `origin/main`, excluding the obsolete local Codex documentation commit. Git writes now work. Existing pretender history, lock and stray docs artifacts remain untouched.

- User approved expanding testaruda-1hnh to correct discovery scope, with separate adapter tickets.
- Completed testaruda-vpnf: Python discovery now uses the existing project configuration filter. A subprocess regression first failed by returning an excluded fixture, then passed. All 53 Python adapter tests, root Clippy, formatting and structural `ah check` passed.
- Updated repository discovery exclusions to include fixture projects. Backed up the local SQLite store to `/tmp/testaruda-store-before-scope-fix.db`, then removed only 818 external Testimonial test identities and 3 Python fixture identities, dependent records and cached selections. The database remains an uncommitted build artifact.
- Filed testaruda-pevk: Testimonial.jl discovery falls back to its own test directory when the invoking project has no `test/` directory. Its source lies outside the writable workspace; no external edits or messages were sent.
- Safe selection now executes both local adapter groups, ingests 53 Rust and 7 Python results and skips no selected group. It still exits 10 because successful `run_adapter_group` calls return `None`, which `run_ci_tests` interprets as unavailable execution. Resolve this through testaruda-oeft before closing testaruda-1hnh or pushing.
- Julia dependency repair remains unresolved. A temporary writable depot timed out during loading; disabling compiled modules exposed a missing MbedTLS artifact. The full workspace suite stalled in Julia tests and was interrupted with exit 130. No full-workspace success is claimed. User chose to pause after the validated Python fix; do not continue Julia dependency repair without a new instruction.

Review: five self-review passes checked scope, configuration reuse, included-test preservation, hidden/default exclusions and subprocess cleanup; fixed Rustdoc paragraph separation flagged by Clippy. Findings were not verified by TypeSafe; self-reported validation applies, with false-positive estimates unmeasured.

Quality ledger: Changed — Python adapter, regression test, local exclusion configuration, current tracker export and this handoff. Verified — RED/GREEN adapter tests, root Clippy, formatting, structural specs, required safe-selection reproduction and SQLite scope counts. Risks — Julia workspace validation and safe-selection exit status remain blocked; no push occurred. Next — on resume, implement testaruda-oeft with TDD and review; rerun the exact pre-push gate and obtain repository-required push approval.

## PR preparation update

User authorized creating a PR, waiting for passing checks and merging. Completed testaruda-oeft with six controlled execution regressions; CLI, exit-code, recall-fault and seeded-fault suites pass. Root Clippy, formatting, structural specs and exact safe-selection pre-push command pass. Closed testaruda-oeft and testaruda-1hnh; all 514 workspace tests now pass, including 22 Julia integration tests, and testaruda-pevk remains open. All implementation and reviewed tracker changes are committed separately, within the 400-line PR limit. Push succeeded with all hooks enabled using freshly built binaries on PATH, a writable temporary Julia depot with the missing MbedTLS artifact installed, and a current titi build in /tmp. External source checkouts are unchanged. Next: verify hosted CI, merge and record the result in branch knowledge.
