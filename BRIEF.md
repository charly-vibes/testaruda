# Project status brief

## Purpose / current state

none

## Focus

none

## In flight

- testaruda-2t3n — docs: apply README front-door standard v0.4 (§5.1) (status: open; priority: 1; age: 2d)
- testaruda-pevk — Julia adapter discovery must stay within the invoking project (status: open; priority: 1; age: 43d)
- testaruda-47ws — Docs workflow fails on tag pushes: github-pages environment protection rejects tag-triggered deploys (status: open; priority: 2; age: 35d)
- testaruda-7r9l — [bug] exec ingest in CI resolves only 60/333 junit results against a store with 335 discovered items (fresh store, same junit source resolves 321/333 locally) (status: open; priority: 2; age: 35d)
- testaruda-7tip — Dual-format decision: 21 deployed specs are plain openspec format (specodelic block now present, lint skips all) (status: open; priority: 2; age: 41d)
- testaruda-kx1t — Reject malformed existing configuration during selection (status: open; priority: 2; age: 43d)
- testaruda-p7m0 — adoption: genesis provenance footer + ManagedBlockDrift drift lint (status: open; priority: 2; age: 48d)
- testaruda-qybw — typescript adapter mangles vitest discovery ids into file::t1::t2::t3 chains (status: open; priority: 2; age: 41d)
- testaruda-srqu — pre-push gate environmentally broken: julia adapter (MbedTLS precompile) + python fixtures (2 unknown results) fail testaruda select --safe; blocks pushing fixes (status: open; priority: 2; age: 42d)
- testaruda-xcgs — Execute safe full-suite fallback consistently across output formats (status: open; priority: 2; age: 43d; blocked by: testaruda-yo3i)
- testaruda-ymzg — adapter-julia (Testimonial.jl): run-args runner can't load [extras] test-target deps; ReTestItems.runtests files= kwarg rejected (status: open; priority: 2; age: 49d)
- testaruda-yo3i — Choose and approve a full-suite fallback contract for unavailable adapter evidence (status: open; priority: 2; age: 43d)
- testaruda-7rz7 — exec: stale-exec lockfile detection — report a previous wedged exec on start (status: open; priority: 3; age: 35d)
- testaruda-di1s — openspec 1.x upgrade blocked: 8 validation failures under @fission-ai/openspec@1.14.2 (status: open; priority: 3; age: 33d)
- testaruda-i5oe — adapter-typescript: detect_runner misses bun test — runs vitest on bun repos (status: open; priority: 3; age: 49d)
- testaruda-n0hc — adapter-dotnet/titi: discover returns 0 test items on cold store (round-3 regression?) (status: open; priority: 3; age: 49d)
- testaruda-r9ce — [backlog] genesis upgrade: pin 0.8.x, latest is 0.13.1 (gh-70 et al.) (status: open; priority: 3; age: 35d)
- testaruda-ysvm — debug aid: log adapter cmdline + exit status at -v; consider 'testaruda ps' (status: open; priority: 3; age: 35d)

## Blocked / waiting

- testaruda-xcgs — Execute safe full-suite fallback consistently across output formats (status: open; priority: 2; age: 43d; blocked by: testaruda-yo3i)

## Next / ready

- testaruda-2t3n — docs: apply README front-door standard v0.4 (§5.1) (status: open; priority: 1; age: 2d)
- testaruda-pevk — Julia adapter discovery must stay within the invoking project (status: open; priority: 1; age: 43d)
- testaruda-47ws — Docs workflow fails on tag pushes: github-pages environment protection rejects tag-triggered deploys (status: open; priority: 2; age: 35d)
- testaruda-7r9l — [bug] exec ingest in CI resolves only 60/333 junit results against a store with 335 discovered items (fresh store, same junit source resolves 321/333 locally) (status: open; priority: 2; age: 35d)
- testaruda-7tip — Dual-format decision: 21 deployed specs are plain openspec format (specodelic block now present, lint skips all) (status: open; priority: 2; age: 41d)
(12 more omitted)

## Parked

none

### Needs re-triage

none

## Recent

- 0776a1f9fe35a81b3ea94bea97ef924ce20e7dd3 chore(gates): render the managed gate block from the canonical commands (2026-11-18T16:03:39Z)
- cd0244b65b126a25520d08501e75bac0f82ec730 chore(openspec): migrate project.md into config.yaml; pin 2.1.0 (2026-11-18T15:02:45Z)
- de79b7e911711c82d05483f219a0dc08e28cc8ca chore(openspec): adopt the 2.1 artifact layout (2026-11-18T15:02:45Z)
- f5260a541fb4a2ab9d31e5563d6201deae05f4f3 docs: apply issue-review fixes to README/release tickets (2026-11-16T12:04:37Z)
- d97569dab588d0ca7a7428bf039f822abdd27dcd docs: apply issue-review fixes to README/release tickets (2026-11-16T11:47:17Z)
(195 more omitted)

## Health + confidence

Health: **amber** — basis: signals — owner: none — reason: orphan_commits: 13 commits in the 14-day window link no tracker id or open work-item path: 0776a1f9fe35a81b3ea94bea97ef924ce20e7dd3 (chore(gates): render the managed gate block from the canonical commands); 1b6ef4ee24da22d0019e353b924568427e6cd355 (refactor(quality): dedupe TestUnwrap traits, shared test_util, lazy-cluster fixes); 2c0f28a0708881806aff213241401f7c4972cc8d (docs(brief): pretender debt fix brief (testaruda) — orchestrator artifact); 4244df564b8861b51d8ec4eb895314276f4c08eb (refactor(quality): extract ordering scenario helpers, split adapter-typescript hotspots); 547847525cc2336bb96fdd03948b455efb63ba81 (refactor(quality): eliminate bare unwrap/expect in test code (1658 sites)); 999ebd9f2968c27c5a3819f4dba3f0805f4943be (docs: file README front-door and release tickets); c2e9f1a078586e74300fed96eee77e16c182b7ab (refactor(quality): pretender gate green — final debt sweep); cd0244b65b126a25520d08501e75bac0f82ec730 (chore(openspec): migrate project.md into config.yaml; pin 2.1.0); d349fb7545eb5852441dc4ab564e6e7d432a33e0 (fix(lint): drop dead TestUnwrap::require across workspace — zero-call-site trait method broke clippy -D warnings (pre-push lint gate); needless borrow in resolver, unused tids param in ordering); d4bd96b2a958de5360de520285792fc0cb140a0a (refactor(quality): merge lazy test clusters, real fixture assertions); d97569dab588d0ca7a7428bf039f822abdd27dcd (docs: apply issue-review fixes to README/release tickets); de79b7e911711c82d05483f219a0dc08e28cc8ca (chore(openspec): adopt the 2.1 artifact layout); f5260a541fb4a2ab9d31e5563d6201deae05f4f3 (docs: apply issue-review fixes to README/release tickets)
- signal: orphan commits — 13 commits in the 14-day window link no tracker id or open work-item path: 0776a1f9fe35a81b3ea94bea97ef924ce20e7dd3 (chore(gates): render the managed gate block from the canonical commands); 1b6ef4ee24da22d0019e353b924568427e6cd355 (refactor(quality): dedupe TestUnwrap traits, shared test_util, lazy-cluster fixes); 2c0f28a0708881806aff213241401f7c4972cc8d (docs(brief): pretender debt fix brief (testaruda) — orchestrator artifact); 4244df564b8861b51d8ec4eb895314276f4c08eb (refactor(quality): extract ordering scenario helpers, split adapter-typescript hotspots); 547847525cc2336bb96fdd03948b455efb63ba81 (refactor(quality): eliminate bare unwrap/expect in test code (1658 sites)); 999ebd9f2968c27c5a3819f4dba3f0805f4943be (docs: file README front-door and release tickets); c2e9f1a078586e74300fed96eee77e16c182b7ab (refactor(quality): pretender gate green — final debt sweep); cd0244b65b126a25520d08501e75bac0f82ec730 (chore(openspec): migrate project.md into config.yaml; pin 2.1.0); d349fb7545eb5852441dc4ab564e6e7d432a33e0 (fix(lint): drop dead TestUnwrap::require across workspace — zero-call-site trait method broke clippy -D warnings (pre-push lint gate); needless borrow in resolver, unused tids param in ordering); d4bd96b2a958de5360de520285792fc0cb140a0a (refactor(quality): merge lazy test clusters, real fixture assertions); d97569dab588d0ca7a7428bf039f822abdd27dcd (docs: apply issue-review fixes to README/release tickets); de79b7e911711c82d05483f219a0dc08e28cc8ca (chore(openspec): adopt the 2.1 artifact layout); f5260a541fb4a2ab9d31e5563d6201deae05f4f3 (docs: apply issue-review fixes to README/release tickets)

## Sources & freshness

| source | status | observed_at | staleness | detail |
| --- | --- | --- | --- | --- |
| beads | present | 2026-11-16T11:53:02Z | fresh | 24 malformed line(s) skipped in .beads/issues.jsonl |
| git | present | 2026-11-18T16:03:39Z | fresh | none |
| openspec | present | 2026-08-05T20:16:41Z | stale | none |
| whisper | present | 2026-10-14T22:44:23Z | stale | none |
| curation | present | 2026-11-18T21:12:08Z | fresh | none |

Curation: updated_at 2026-11-18T21:12:08Z; confirmed_at none.

Freshness: generated_at 2026-11-18T21:12:08Z; per-source observed_at and staleness are listed above.
