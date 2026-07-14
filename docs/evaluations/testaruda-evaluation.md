# testaruda — implementation-state and specification evaluation

**What this document is.** An independent, self-contained evaluation of **testaruda**, a
software project that was scaffolded using the `charly-vibes` developer-tool suite as its
mandated workflow. It is a companion to [`dev-tooling-evaluation.md`](dev-tooling-evaluation.md)
(which evaluates the *tools*); this document evaluates the *project those tools produced* —
both the state of its code and, in depth, the quality of its written specification. It is
written to the "amnesia test": every tool, term, and scenario is defined here, so it is
understandable with no prior context.

**Date of evaluation:** 2026-07-07.
**Evaluator:** an autonomous LLM coding agent ("pi").
**Subject commit:** `75414cb` ("initial scaffold: testaruda test selection engine").
**Verdict in one line:** the *specification* is A-grade in form and the *core algorithm* is
correct, but the *implementation is a non-functional scaffold*, and the specification hides
several genuine correctness defects in its safety-critical core that its polished EARS
formatting can mask.

---

## 1. What testaruda is

**testaruda** is a Rust command-line program that aims to be a **language-agnostic test
selection engine**: given a code change (a diff), it computes the subset of a test suite
that must run, so CI and local developers can skip tests that a change cannot possibly
affect. Its stated model is mathematically specific — it treats the dependency data as a
**provenance-semiring K-relation** (see the glossary below) and computes the affected tests
as the *transpose* of the transitive closure of that relation, under a **recall-first**
invariant (it must never skip a test that a change could have broken).

The repository contains two very different things, and the gap between them is the story of
this evaluation:

1. **A specification** — `docs/tia-srs-ears.md`, a ~130-requirement Software Requirements
   Specification written in **EARS** notation (Easy Approach to Requirements Syntax, a
   template discipline where every requirement is typed as Ubiquitous / Event-driven /
   State-driven / Optional / Unwanted-behaviour). It is titled "`tia` — SRS" (an earlier
   name for the project) and is thorough, literature-grounded, and well-structured.

2. **An implementation** — ~816 lines of Rust across `src/` that builds cleanly and passes
   6 trivial unit tests, but, as shown below, cannot actually select a test end-to-end.

### Glossary (terms used throughout)

- **Semiring** — an algebraic structure `(K, ⊕, ⊗, 0, 1)`: a set with an "add" operation
  `⊕`, a "multiply" operation `⊗`, and identity elements. Unlike ordinary arithmetic, a
  semiring has **no subtraction** (no additive inverse). This missing-subtraction property
  is central to finding **S1** below.
- **Provenance semiring** — the "free" semiring `ℕ[Edges]` (formally, polynomials whose
  variables are dependency edges). A classic result (Green-Karvounarakis-Tannen, *Provenance
  Semirings*, PODS 2007) says this is *universal*: for **positive** database queries (no
  negation, no aggregation), every other semiring result can be obtained from it by a unique
  structure-preserving map (a **homomorphism**). testaruda calls this its "master" semiring.
- **K-relation** — the dependency graph represented as a matrix of semiring values.
- **Transpose / reverse reachability** — dependencies point *test → code*; to find which
  tests a *code* change affects you follow the edges backwards. This reversed traversal is
  the transpose, and it is the correct core idea.
- **Recall-first / over-approximation** — the system must model *at least* every true
  dependency (it may model spurious extra ones). This guarantees no affected test is ever
  missed ("recall"), at the cost of sometimes running more than strictly necessary.
- **Ascent** — a Rust library that embeds a Datalog-style logic language (rules + a
  fixpoint solver) directly in a Rust binary, with support for **lattice** columns (values
  that combine by a least-upper-bound). testaruda uses Ascent as its selection engine.
- **EDB / IDB** — in Datalog, the **E**xtensional **D**ata**B**ase is the input facts; the
  **I**ntensional **D**ata**B**ase is the relations derived by the rules.

### The `charly-vibes` tools wired into the project

testaruda is set up to be driven by the same four-tool suite evaluated in the companion
report. For self-containment: **wai** (`wai`) tracks the *why* behind decisions;
**dont** (`dont`) forces claims to be grounded in file+line evidence; **pretender**
(`pretender`) checks per-function code complexity; **espectacular** (`ah`) verifies that
written Given/When/Then specs map to real tests. Two supporting tools also appear:
**beads** (`bd`, a git-native issue tracker) and **openspec** (a spec-driven-development
scaffolder whose `openspec/` directory `espectacular` reads).

---

## 2. Evaluation scenario, method, environment

- **What was evaluated:** the checked-in state of the testaruda repository at commit
  `75414cb` — its Rust sources, its build/test behaviour, its own tooling wiring, and its
  specification document.
- **Method:** (a) build the binary (`cargo build --release`) and run its own test suite;
  (b) exercise every CLI subcommand end-to-end against a throwaway git repo; (c) run the
  project's own quality tools (`pretender`, `ah`); (d) read the full SRS and check its
  mathematical and logical claims against the referenced literature and against the
  project's own reference rule set.
- **Environment:** macOS on Apple Silicon; Rust toolchain installed from source (the
  `charly-vibes` tools ship no prebuilt binaries — see companion report F9). `clippy` was
  **not** installable in the toolchain, so lint could not be run.

Severity legend: 🔴 blocker · 🟡 friction/defect · 🟢 papercut/documentation · 💡 suggestion.
Findings are namespaced: **I#** = implementation, **S#** = specification.

---

## 3. Implementation-state findings

Build and unit tests pass:

```
$ cargo build --release        # clean, ~53s
$ cargo test --release         # 6 passed (all trivial: semiring algebra + arg parsing)
```

The 6 tests only exercise pure helper functions (the three semiring `add`/`mul` laws, an
argument-split, and "the Ascent program type exists"). None exercises an actual selection.

### 🔴 I1 — There is no data-ingress path for the dependency graph; selection can never fire.

The entire purpose is to select tests from a dependency graph. But every `INSERT` statement
in the codebase writes to only two of the six tables — `content_units` and `run_history`.
**Nothing** ever writes `test_items`, `dependency_edges`, or `reverse_index`. Consequently
the `test_dep` relation the engine reads is always empty, so the rule that selects a test
(`affected(t) <-- test_dep(t, c, _, _), impacted(c)`) can never produce output. The
language-adapter protocol — the only specified mechanism to populate edges (see §5.8 of the
SRS) — is entirely unimplemented (no subprocess is ever spawned). Demonstrated:

```
$ testaruda select --files "src/foo.rs,src/bar.rs"
{ "changed_count": 0, "selected_count": 0, "tests": [] }
```

Selection is structurally incapable of returning a non-empty set. This is the headline
defect: the tool cannot do the one thing it exists to do.

### 🔴 I2 — `ingest` is broken by a foreign-key violation.

`ingest` writes to `run_history`, which has a foreign key to `test_items`, but never ensures
the referenced test item exists (and per I1 nothing else creates test items):

```
$ echo '{"run_id":"r1","tests":[{"id":1,"outcome":"failed"}]}' > res.json
$ testaruda ingest res.json
Error:   × Failed to insert run result: FOREIGN KEY constraint failed
```

The runtime-feedback path (SRS §5.5) and the always-run set (which reads previously-failed
tests from `run_history`) are therefore both unreachable.

### 🔴 I3 — CI exit codes are not implemented (violates SRS §5.11).

The SRS and `docs/cli.md` promise a CI contract: exit `20` = "no tests affected, safe to
skip", exit `10` = "low confidence, run everything", other-nonzero = hard error. Actual:

```
$ testaruda select --files "x.rs"; echo $?   # no tests affected → spec requires 20
0
```

Only `0` (success) and `1` (error) are ever returned. The safety-critical exit semantics —
including the rule "treat unknown conditions as run-all, never run-none" — are absent.

### 🔴 I4 — `select` misclassifies genuinely-changed files as "unresolved" on first sight.

On first encounter a changed path is not yet in `content_units`, so it is bucketed as
`unresolved` (and inserted). A *second* run of the same file then reports it as `changed`:

```
$ testaruda select --files "src/foo.rs"   # 1st run → changed_count: 0
$ testaruda select --files "src/foo.rs"   # 2nd run → changed_count: 1
```

Same input, store-dependent output that flips classification. Worse, `unresolved` files are
supposed to trigger conservative force-include or component fallback (the recall guard); here
they are silently created and produce nothing, and the count is never surfaced. This is a
recall-relevant bug against the cardinal invariant.

### 🟡 I5 — espectacular (`ah`) is wired but the project scaffold is incomplete.

The project ships `.espectacular/config.toml` pointing `specs = "openspec/specs"` and
`changes = "openspec/changes"`, but those directories were never created (the `openspec/`
folder contains only `AGENTS.md` and `project.md`). So the project's declared spec-gate
cannot run:

```
$ ah check
No such file or directory (os error 2)   # exit 2 (correct failure code)
```

Two sub-points: (a) this is primarily a *project* defect — the scaffold is half-finished;
(b) it exposes a *tool* papercut recorded separately as **F26** in the companion report:
`ah check`'s error is a bare `errno` message that names neither the missing path nor the
misconfigured setting, whereas `ah doctor` diagnoses it precisely
(`missing-path: specs directory not found: …/openspec/specs`). `ah check` exits `2`
correctly — there is **no** silent-success bug here (an earlier note to the contrary was a
shell-pipe artifact and is retracted).

### 🟡 I6 — The `justfile` does not parse.

`just` is the task runner referenced by the README and the project's own instructions. Its
`justfile` has Markdown prose and a fenced ```` ```bash ```` code block prepended above the
real recipes, so the parser dies:

```
$ just --list
error: unknown start of token '<'  ——▶ justfile:36:1   (an HTML comment on line 36)
```

Every recipe (`just test`, `just build`, `just check`, `just ah`) is unusable. This looks
like a documentation snippet accidentally committed into the recipe file.

### 🟢 I7 — A fully-documented config file is never read.

`docs/configuration.md` specifies a detailed `testaruda.toml` (`[store]`, `[confidence]
threshold`, `[semiring] default`, `[ci] shadow`, `[adapters]`, `[always_run] patterns`), and
the crate docs advertise "Config: `testaruda.toml`". No code parses it — the `toml` and
`serde_yaml` crates are unused, `init` never writes it, and the confidence-threshold fallback
it configures is unimplemented. `init`'s message "Create store and config" is false; it only
creates the store.

### 🟢 I8 — Several declared dependencies are entirely unused.

`blake3` (content fingerprinting — a *specified* feature), `similar` (diffing), `uuid`,
`rayon` + `ascent-byods-rels` (the parallel / union-find engine path the SRS reference
architecture mandates), `toml`, and `serde_yaml` are all in `Cargo.toml` but referenced
nowhere in `src/`. Content fingerprinting is stubbed to the literal string `"unknown"`, so
change-detection-by-fingerprint (a core requirement) does not exist.

### 🟢 I9 — `pretender` advisories (minor, non-blocking).

`pretender check src/` flagged 5 functions over the 40-line / ABC-30 advisory thresholds
(`main` 67 lines, `load_selection_context` 68, `from_diff` 64, `initialize` 53,
`engine::select` 49) and exited `0`. All advisory; reasonable to defer. This is the one tool
that worked exactly as intended here (a corroboration of the companion report's positive note
on `pretender`).

### 🟢 I10 — The `oracle` subcommand is a no-op stub.

`testaruda oracle` prints a banner and exits `0`; with a program argument it shells out to
`souffle` and dumps stdout with no parsing or cross-validation. The Soufflé validation oracle
and shadow-mode divergence check (SRS §8.3) are not implemented.

---

## 4. Specification evaluation

The SRS was reviewed on its own merits (independent of the code) for **correctness**
(are the claims true?), **coherence** (do requirements contradict each other?), and
**completeness** (what is missing?). The verdict is **NEEDS RESOLUTION**: the form is
excellent, but there are real correctness defects concentrated in the safety-critical core,
where they are easy to miss precisely because the surrounding formatting is so clean.

### 🔴 S1 — The homomorphism claim (ARCH-003) is provably too strong; the safety machinery lives outside it.

Requirement **TIA-ARCH-003** (a *binding* Ubiquitous requirement) states that concrete
results are derived "by semiring homomorphism, such that **no derived result can disagree
with the master** computation." The universal property that would justify this
(Green-Karvounarakis-Tannen) holds **only for positive queries** — no negation, no
aggregation. But the spec's own safety features are exactly negation/aggregation:

- the **always-run set** (SAFE-007) is defined by "tests with **no recorded history**",
  "**newly added** tests", "test that **failed in its last** run" — negation-as-failure and
  temporal argmax;
- the **confidence fallback** (SAFE-002) is a threshold predicate over an aggregate;
- **duration ranking** (SEL-006) is an aggregation.

The spec's own reference rule set proves the point: it selects always-run tests with the rule
`affected(t) <-- always_run(t)`, where `always_run` is an *independent input relation*, not
something derived from the provenance polynomial. So the Boolean "affected" set already
contains members with **no provenance in the master** — a derived result exceeds the master
by construction. ARCH-003 is contradicted by SAFE-007 and by the spec's own Appendix G.
(§8.4 already concedes the "no additive inverses" problem when rejecting a streaming engine;
the same missing-subtraction gap invalidates the universality claim wherever negation enters.)
**Fix:** scope ARCH-003 to "the positive reachability core", and state explicitly that
always-run and fallback are unioned *outside* the semiring derivation.

### 🔴 S2 — The confidence column inverts the very signal the fallback depends on.

The spec's reference rule set seeds confidence at the maximum value (`ONE` = 1.0) for
`unresolved` content units (files the analyzer *could not* resolve) and for `always_run`
tests (forced in *because there is no evidence*). But confidence is what gates the safety
fallback (SAFE-002: "if confidence < threshold, run everything"). So the rule set assigns
**highest confidence exactly where the model is weakest**, which would *suppress* the
fallback that is meant to protect those cases. This is a concrete, latent soundness bug in
the specification's own "verified" reference rule set. **Fix:** unresolved / always-run nodes
must *lower* confidence (or be excluded from the confidence column), never set it to 1.0.

### 🔴 S3 — "Confidence" is defined two incompatible ways.

- Appendix A + SEL-003 + the reference rule set define confidence as a **Viterbi path-product**
  of edge weights (a per-test graph quantity; the worked example computes `0.8 × 0.8 = 0.64`).
- **CONF-002** defines confidence as derived from "coverage freshness, adapter resolution
  ratio, history depth, and environment match" — a per-selection aggregate of four *external*
  signals, none of which appears anywhere in the rule set.

These are different quantities with different types. SAFE-002 triggers fallback on
"confidence" without saying **which** — and it is the safety-critical trigger. **Fix:** pick
one definition or name them distinctly (e.g. `path_confidence` vs `selection_confidence`) and
state which one gates fallback.

### 🔴 S4 — The "safe to skip" exit code (CI-003) contradicts the always-run rule (SAFE-007).

**CI-003** says: "When **no tests are affected**, the CLI shall exit `20`" (safe to skip the
test stage). But **SAFE-007** forces the always-run set to include every previously-failed,
newly-added, no-history, and quarantined test. On a fresh store *every* test has no history,
so always-run is the whole suite; in general it is rarely empty. "No tests affected" (an empty
reachability set) can be true while always-run is non-empty — yet CI-003 would emit "safe to
skip", dropping tests SAFE-007 mandates. For a recall-first tool, telling CI to run *nothing*
is the single highest-risk output, and it is guarded only on the reachability set. **Fix:**
condition exit-`20` on the **full** selection (reachability ∪ always-run ∪ fallback) being
empty.

### 🟡 S5 — ARCH-006 mandates *derivative* incrementality; the reference engine only does *scoped batch*.

**ARCH-006** (binding) requires computing selection "as the change-propagation (**derivative**)
of the reachability query, recomputing **only relations affected by the change set**." The
reference-engine requirements (ENG-007/008) and Appendix F instead redefine "incremental" as
change-scoping + a result cache over a residual subgraph, explicitly "**rather than** a
streaming engine". A batch Ascent run over a scoped subgraph still recomputes relations the
change did not touch *within that scope* — that is re-scoping, not the derivative ARCH-006
literally requires. The spec resolves this by *redefining the word* in an appendix, not by
satisfying the requirement as written; as stated ARCH-006 is unverifiable against the chosen
engine. **Fix:** soften ARCH-006 to "scope-bounded recomputation", or state that the reference
build only partially satisfies it.

### 🟡 S6 — Unconditional byte-stability (AGENT-002) versus optional ordering (SEL-005) under mandated parallelism (COMP-008).

**AGENT-002** demands byte-stable output for identical inputs — always. **SEL-005** makes
stable ordering happen only "**where requested**." **COMP-008/ENG-006** mandate *parallel*
per-component evaluation, which reorders results. Byte-stability requires a total order the
spec only promises on request, so AGENT mode must implicitly force SEL-005 or AGENT-002 is
unsatisfiable under parallelism. The dependency is not wired.

### 🟡 S7 — Symbol-granularity narrowing (CHG-004) has an unstated soundness precondition.

**CHG-004** lets the system fingerprint individual symbols so that "unrelated edits within a
file yield **no** affected tests for unchanged symbols." This narrows selection below file
granularity and is sound **only if** the symbol-level dependency model is complete for that
file — but reflection, macros, dynamic dispatch, and string references routinely defeat
symbol resolution. A test depending on symbol *X* through an unmodeled path, when sibling
symbol *Y* changes, would be wrongly excluded, violating the cardinal recall invariant. The
existing guard (SAFE-004) covers *unresolved files*, not *partially-resolved* ones. **Fix:**
state the precondition — symbol narrowing is permitted only under a declared-complete symbol
model, else fall back to file granularity.

### 🟡 S8 — Idempotency (RUN-005) and crash-safety (REL-002) are asserted with no enabling mechanism.

Ingesting a run updates running statistics (mean duration, failure rate, flakiness) — inherently
non-idempotent — yet **RUN-005** requires idempotent re-ingestion with no required run-identity
key or dedup/event-log mechanism to make it achievable. **REL-002** (crash-safe, consistent
after interruption) has the same shape: a property with no stated transactional boundary.

### 🟢 Lower-severity specification gaps

- **No retraction/deletion semantics.** When a content unit or test is deleted, nothing
  specifies pruning the graph or reverse index; retraction appears only as a future scale-up
  path. Stale edges over-select (recall-safe) but corrupt explanations and metrics. State
  "no retraction in v1; stale = conservative" explicitly.
- **Scope coherence.** The spec calls this a "single responsibility" CLI, then specifies a
  daemon, watch mode, remote cache, sharding, ML ranking, multi-repo federation, and a blob
  store. The non-goals exclude "test runner / build system", but the surface rivals one —
  flag for minimum-viable-product scoping.
- **Naming drift.** The SRS is still headed "`tia` — SRS" with all `TIA-*` requirement IDs,
  though the project was renamed to `testaruda`; the document itself was never re-headed.
  Since the project conventions ask for `TIA-*` IDs in code doc-comments, this is a
  traceability smell.

---

## 5. What is genuinely strong

- **EARS discipline is real.** Every requirement is typed and carries an explicit
  verification method (Inspection / Analysis / Demonstration / Test). This is better than
  most production SRSs.
- **The abstraction/reference split** (binding, engine-independent architecture in §4 versus
  a separately-justified, explicitly-reversible reference engine in §8) is excellent design
  hygiene.
- **The core selection semantics are correct.** Reverse reachability = transpose closure for
  selection is right; the Boolean / tropical-distance / Viterbi-confidence lattice
  instantiation is a legitimate and elegant realization; the engine-evaluation appendix is
  honest about trade-offs.
- **Recall-first framing is coherent** as a philosophy — which is exactly why the
  confidence-inversion (S2) and exit-code (S4) defects matter: they quietly betray the stated
  invariant.
- **pretender** worked exactly as intended against this code (I9).

---

## 6. Prioritized recommendations

1. **Resolve the safety-core specification defects first (S1–S4).** They are small textual
   fixes, but each one currently undermines the recall-first invariant that is the tool's
   entire reason to exist. Do these before any further implementation, because they change
   what "correct" means.
2. **Then the coherence items (S5–S8):** reconcile ARCH-006 with the batch engine, wire
   AGENT-002 to SEL-005, state CHG-004's precondition, and give RUN-005/REL-002 an enabling
   mechanism.
3. **For the implementation, close the ingress gap (I1) and the ingest FK bug (I2) before
   anything else** — until edges and test items can be created, the engine is a demo. Then
   wire the CI exit-code contract (I3, small and safety-critical) and fix the
   changed-vs-unresolved classification (I4, a soundness bug).
4. **Finish or delete the half-built scaffolding:** complete the `openspec/` layout so `ah`
   runs (I5), repair the `justfile` (I6), and either implement `testaruda.toml` (I7) and the
   unused dependencies (I8) or remove the promises so the docs match reality.

---

*This document is the self-contained, standalone companion to `dev-tooling-evaluation.md`.
The tool-level papercut surfaced during this evaluation is recorded there as finding F26.*

---

## 7. Retest round — 2026-07-10: the project has become spec-and-ticket-only

**Subject commit:** `94fcc5c` (up from `75414cb`). **Read-only constraint:** per this
evaluation round's scope, `testaruda`'s repository (like the other four `charly-vibes`
tools) was treated as read-only — this section is a pure retest against the pulled state,
with zero commits or edits made to the project.

**Headline finding: in the five commits since the last evaluation, zero lines of `src/`,
`Cargo.toml`, or `justfile` changed.** `git diff --stat 75414cb..94fcc5c` touches **177
files / +3223 −2 lines**, and every one of them is under `openspec/specs/`,
`.espectacular/` (contract files), `.beads/` (the issue tracker's own store), or docs
(`docs/tia-srs-ears.md`). The implementation is **byte-for-byte the same 816-line, 6-test
skeleton** evaluated in §2–3 above. In plain terms: **all work in this window was
specification and ticket-writing; the code did not move.** This matches the project's own
beads backlog exactly — 19 of 20 tracked issues are still `open`/`in_progress`, and 4 of
those are priority-1 `feature` tickets covering exactly the missing pieces this section
re-confirms (`testaruda-6js` adapter protocol, `testaruda-gbs` CI exit codes,
`testaruda-9uw` adapter subprocess management, `testaruda-imo` content fingerprinting).

### The specification surface grew by 17 formal spec documents — and confirms 0/155 scenarios have real test coverage

The `openspec/specs/` tree now has **18 requirement areas** (adapter-protocol,
agent-mode, architectural-constraints, change-detection, ci-integration, composability,
confidence-scoring, core-data-model, engine, local-mode, non-functional, observability,
persistence, provenance, runtime-feedback, safety, selection-engine, verification;
1,539 total lines), each with an `espectacular` (`ah`) contract file per scenario —
**154 scenario contracts**, one more than the SRS's original ~130 EARS requirements. This
is a genuine specification-completeness improvement: I5 ("scaffold half-finished, `ah
check` cannot even run") from §3 is **fixed** — the `specs/` and `changes/` directories
`.espectacular/config.toml` requires now exist and `ah check` runs (does not error out).

However, running it end-to-end (`ah check`) surfaces the real state precisely:

```
$ ah check
... (155 findings, all kind "no-tests-ran")
summary: { "structural": 0, "execution": 155, "passed": 0,
           "counts_by_kind": { "no-tests-ran": 155 } }
```

**0 of 155 declared scenarios have a passing, scenario-specific test.** Every contract's
`test.command` is the same blanket `cargo test` invocation (verified by inspecting the
contract TOML files: each declares `command = "cargo test"` rather than a filtered test
name), and since the codebase's only 6 real tests are generic semiring-algebra/arg-parsing
unit tests with no per-scenario naming convention, `ah`'s discovery logic cannot match any
of them to a declared scenario — so every one of the 155 reports the `no-tests-ran`
execution failure, regardless of whether the underlying feature exists. This is exactly
what "only specs and tickets" means in practice: **a fully-formed, EARS-grounded
requirements tree and a matching contract-per-requirement scaffold, wired to zero verified
behavior.** The polish of the specification (still deserved — see §5) makes this easy to
miss if one reads only the spec tree and not `ah check`'s output.

### I1–I4 (the safety-critical implementation blockers) all still reproduce, unchanged

Retested against a fresh scratch repo with the rebuilt `94fcc5c` binary:

- **I1 (selection can never fire)** — `testaruda select --files "foo.rs"` still returns
  `{"changed_count": 0, "selected_count": 0, "tests": []}` on first sight of a file.
- **I4 (changed-vs-unresolved flip)** — running the identical `select` a second time still
  flips `changed_count` from `0` to `1` for the same input, confirming the store-dependent
  misclassification is unchanged.
- **I2 (ingest FK violation)** — `testaruda ingest res.json` still fails identically:
  `Error: × Failed to insert run result: FOREIGN KEY constraint failed`.
- **I6 (justfile does not parse)** — `just --list` still fails with the identical
  `error: unknown start of token '<'` at the same line (a leftover Markdown/HTML comment
  block above the real recipes).
- **I3, I7–I8** were not independently re-run this round but are corroborated by the zero
  `src/`/`Cargo.toml` diff: the CI exit-code contract, `testaruda.toml` parsing, and the
  unused-dependency list (`blake3`, `similar`, `uuid`, `rayon`, `ascent-byods-rels`, `toml`,
  `serde_yaml`) are line-for-line the same code, so they are unchanged by construction.

### Net assessment

The user-facing framing for this round ("testaruda should now have only specs and
tickets") is **factually confirmed**, with one caveat worth flagging: the repository is
not *purely* specs and tickets — the original 816-line implementation skeleton from §2–3
is still checked in and still builds — but no implementation *work* has occurred since the
last evaluation; every commit in the intervening window added specification, contracts, or
ticket-tracking metadata. The specification-side investment (§4’s S1–S8 findings, still
unresolved and unretested this round since no spec-content commits touched those specific
clauses per a quick diff check) has not yet been spent on paying down I1/I2/I4/I6, which
remain the actual blockers to the tool doing anything. The most valuable next increment,
unchanged from §6's recommendation, is still: wire a data-ingress path (I1) so the 154
freshly-written scenario contracts have *something* to test against — at that point,
`ah check`'s 155/155 "no-tests-ran" figure becomes a real, trustworthy completeness metric
instead of a placeholder.

---

## 8. Deep evaluation — is the specification actually going to produce a correct tool?

**Scope of this section.** §7 established that testaruda is currently "specs and tickets
only" (zero implementation movement across the last 5 commits). This section asks the
next question: if the current backlog is executed as written, will it actually produce a
*correct*, safe test-selection engine — or does the plan itself have defects that would
carry through into the code? This required (a) verifying that the SRS fixes claimed in
the v0.3 changelog are real and coherent, not just changelog text; (b) checking the
`openspec/specs/` tree (which drives the 154 test contracts) is not stale relative to the
SRS; (c) reading the beads ticket backlog's dependency graph and acceptance criteria for
whether they actually close the implementation gaps (I1–I10) in the right order; and
(d) hunting for *new* defects the previous two evaluation rounds didn't have reason to look
for. Read-only throughout — no changes made to the project.

### 8.1 The headline positive finding: SRS v0.3 genuinely fixed all eight §4 specification defects (S1–S8)

This is not just a changelog claim. Every fix was independently verified against the
actual requirement text and, for S1/S2, against the reference Ascent rule set in
Appendix G:

- **S1 (ARCH-003 homomorphism claim too strong)** — now explicitly scoped: "The
  always-run set (TIA-SAFE-007), confidence-threshold fallback (TIA-SAFE-002), and
  duration ranking (TIA-SEL-006) ... are not derived by semiring closure but contributed
  as input facts ... and are not required to be homomorphic images." Exactly the
  suggested fix.
- **S2 (confidence inversion for unresolved nodes)** — verified in the actual rule set,
  not just prose: `impact_conf(c, 0) <-- unresolved(c);` (was seeded at `ONE` before).
  Unresolved nodes now correctly get **zero** confidence, which *triggers* the SAFE-002
  fallback instead of suppressing it.
- **S3 (two incompatible confidence definitions)** — CONF-002 is now explicitly a
  *modifier* of CONF-001's Viterbi value ("such that the effective path confidence
  reported by TIA-CONF-001 reflects the quality of the dependency evidence"), not a
  competing definition. The dual-definition contradiction is resolved by subordination,
  not by picking one arbitrarily.
- **S4 (exit-20 contradicts always-run)** — CI-003 now reads "When the full selection set
  (reachability ∪ always-run ∪ fallback) is empty, ... exit `20`" — the exact union the
  original review said was missing.
- **S5 (ARCH-006 derivative-incrementality overclaim)** — softened to "scope-bounded
  recomputation restricted to the subgraph ... rather than the full graph," with an
  informative note naming the reference engine's actual mechanism (component cache) and
  differential dataflow as an explicit future scale-up path, not a present claim.
- **S6 (AGENT-002/SEL-005/COMP-008 unwired)** — AGENT-002 now reads "Agent mode SHALL
  implicitly enforce deterministic output ordering (TIA-SEL-005) regardless of parallel
  evaluation (TIA-COMP-008), without requiring a separate flag."
- **S7 (CHG-004 missing precondition)** — now gated on a new capability flag,
  `symbol_model_complete`, wired through matching new sub-requirements TIA-ADAPT-002/005,
  with an explicit fallback to file-level granularity when the flag is absent or false.
- **S8 (RUN-005/REL-002 no enabling mechanism)** — RUN-005 now specifies a mandatory
  run-identity key with reject-if-absent semantics; REL-002 now specifies a transaction/WAL
  boundary and explicitly delegates idempotency to RUN-005's key rather than asserting it
  independently.

**This is an unusually disciplined correction pass** — every fix addresses the *root
cause* named in the original finding (not a surface patch), and none introduces an
observable new contradiction (checked pairwise against the related requirements each one
touches, e.g. SAFE-002 ↔ SAFE-007, CONF-001 ↔ CONF-002, RUN-005 ↔ REL-002).

### 8.2 The `openspec/specs/` tree is in perfect sync with the SRS — no drift

Given the SRS and the openspec specs are two separate documents (the SRS predates
openspec adoption; openspec's 154 contracts are generated from `openspec/specs/*.md`,
not from the SRS directly), there was a real risk that the SRS v0.3 fixes landed in one
document but not the other. Checked exhaustively:

```
$ # requirement IDs defined in openspec/specs/*/spec.md
$ rg -o 'Requirement: (TIA-[A-Z]+-[0-9]+)' -r '$1' openspec/specs/*/spec.md | sort -u | wc -l
149
$ # requirement IDs with a [tag] in the SRS
$ rg -o '\*\*(TIA-[A-Z]+-[0-9]+) \[' -r '$1' docs/tia-srs-ears.md | sort -u | wc -l
149
$ # set difference both directions
$ comm -23 srs_ids.txt openspec_ids.txt   # in SRS, missing from openspec
(empty)
$ comm -13 srs_ids.txt openspec_ids.txt   # in openspec, missing from SRS
(empty)
```

**149/149 exact correspondence, zero missing either direction.** Spot-checked the
requirement *text* (not just the ID) for all eight S1–S8-affected requirements
(ARCH-003, CI-003, CHG-004, AGENT-002, RUN-005, REL-002, CONF-001/002) — all are
byte-identical or effectively identical between the two documents. Additionally checked
every `TIA-*` cross-reference used in prose *within* `openspec/specs/` (e.g. CI-003's
reference to `TIA-SAFE-003`, CHG-004's reference to `TIA-ADAPT-002`) — **zero dangling
references**: every ID referenced anywhere resolves to a real, defined requirement
somewhere in the tree. This is a genuinely well-maintained specification corpus; the risk
that the project is building 154 test contracts against a stale or diverged copy of the
requirements is not realized.

### 8.3 The ticket backlog's dependency graph correctly targets I1 (the core blocker) in the right order

The four priority-1 tickets are, encouragingly, exactly the right ones:

- **`testaruda-9uw`** (Implement adapter protocol subprocess management) — acceptance
  criteria explicitly include "`discover` enumerates test items stored in DB," which
  directly closes the "nothing ever writes `test_items`" half of I1.
- **`testaruda-6js`** (static-deps + fingerprint commands), correctly `blocks`-dependent
  on `9uw` — its acceptance would populate `dependency_edges`/`reverse_index`, the other
  half of I1.
- **`testaruda-p84`** (run-args + ingest commands), correctly dependent on `6js` —
  completes the adapter protocol's write path with runtime edges.
- **`testaruda-gbs`** (CI exit codes + shadow mode), correctly dependent on `x1i`
  (deterministic ordering) — targets I3 directly, and its acceptance criteria
  (`0`/`10`/`20`/error-distinct) match the now-fixed CI-003 union condition exactly, not
  the old broken version.

This sequencing is sound: the original I1 finding was specifically that the engine's
*read*-side Datalog rules were already correct (`affected(t) <-- test_dep(t, c, _, _),
impacted(c)`) — the bug was purely that nothing ever populated the EDB. The ticket plan
does not propose touching the (already-correct) rule set; it proposes wiring up exactly
the missing write path, via the adapter protocol, in the minimal necessary order
(discover → static-deps → ingest). This is the right fix shape, not a rewrite in search of
one.

The reference rule set itself (Appendix G) also shows a specific, deliberate design choice
that reduces future risk: confidence is computed in **fixed-point `ppm` integers**
(parts-per-million), not floating point. Combined with Ascent's lattice-join semantics
being genuinely commutative/associative for `max`/`min`/multiply-along-path, this means
the mandated parallel evaluation (TIA-COMP-008) cannot introduce floating-point
non-associativity nondeterminism into confidence scores — a failure mode that would have
been easy to miss and hard to diagnose (silently different confidence values between two
parallel runs of the identical selection). This was not explicitly called out as a risk
in the original review; it is verified here as a risk that was avoided.

### 8.4 New findings: three gaps in the plan that weren't visible in §3/§4

**S9 (new, 🟡) — no specification requirement covers a content unit's classification on
its very first observation, which is exactly the condition under which I4 (the
changed-vs-unresolved flip) manifests.** TIA-CHG-003 defines the change set as "content
units whose fingerprint differs between base and head" — this presupposes a *prior*
fingerprint of record to compare against. TIA-REL-001 promises deterministic selections
"for identical store state" — but the store state genuinely differs between I4's two
runs (the first run inserts a new, previously-absent `content_units` row), so REL-001
does not formally cover this case either. **No contract or scenario in any of the 18 spec
areas addresses "what classification does a never-before-seen content unit receive, and
is that classification stable on immediate re-invocation with no intervening change?"**
Practically: even after `testaruda-imo` (content fingerprinting) lands, there is no
acceptance criterion or espectacular scenario that would catch a regression of I4 — the
bug could be "fixed" by accident as a side effect of proper fingerprinting, or could
persist silently, and either way nothing in the current plan would notice. **Suggested
fix:** add a `TIA-CHG-009` (or similar) requiring idempotent classification of a content
unit across repeated invocations absent any intervening change, with an explicit rule for
the cold-start case (no prior fingerprint of record ⇒ treat as `unresolved`, consistently,
not `changed` on a coin-flip of insertion order).

**Backlog gap A (🟡) — `testaruda-mfx` (idempotent ingestion) has no formal dependency on
`testaruda-9uw`, but its correctness depends on `test_items` already existing.** I2 (the
original FK-violation defect: `ingest` writes to `run_history`, which has a foreign key to
`test_items`, but nothing ensures the referenced row exists) is not explicitly re-targeted
by any single ticket — the closest candidate, `testaruda-mfx`, is scoped purely to
dedup/idempotency-key semantics and crash safety (TIA-RUN-005/REL-002); its acceptance
criteria test "duplicate key skipped," "missing key rejected," and "crash leaves
pre-ingestion state" — **none of them test "ingest succeeds once the referenced test item
exists"**, i.e. none of them re-verify I2 is actually closed. In practice `mfx` would very
likely be implemented after `9uw` simply because it is priority-2 and `9uw` is
priority-1, so the FK-violation symptom will probably disappear as a side effect — but the
beads dependency graph does not *enforce* this ordering (`mfx` shows `deps: []`), and no
acceptance criterion anywhere explicitly re-tests the original I2 reproduction command.
**Suggested fix:** add `mfx --deps blocks:9uw` (or equivalent, using beads' explicit
dependency syntax — see the companion `dev-tooling-evaluation.md` finding **F14** for the
direction-of-`--deps` footgun to watch for when adding this) and add an acceptance
criterion to `mfx` (or a new contract) that directly re-runs I2's original reproduction:
`testaruda ingest` against a run payload referencing a test item that *does* exist, after
`discover` has populated it.

**Backlog gap B (🟡) — I6 (the justfile that cannot be parsed, blocking every `just`
recipe including `just test`/`just check`) has no ticket anywhere in the backlog and is
not mentioned in any tracked document.** Searched the full beads issue set (all 20
issues, titles and descriptions) and every tracked file in the repository for "justfile"
outside of the `justfile` itself — zero hits. This is a previously-documented (§3, I6),
independently-reproduced (§7) defect that blocks the project's own stated task-runner
workflow, and it has fallen out of tracking entirely between evaluation rounds — neither
carried forward as a ticket nor superseded by a spec requirement (task-runner hygiene is
not modeled by any `TIA-*` requirement, correctly, since it's tooling rather than product
behavior — but it still needs a ticket). **Suggested fix:** file a beads issue for I6; it
is a small, self-contained fix (remove the accidentally-committed Markdown/HTML block
above the real recipes).

---

### 8.5 The empirical validation mechanism for the core invariant exists and is correctly scoped, but not yet elevated

**TIA-VER-004** ("Seeded-fault recall test... SHALL be verified by a seeded-fault recall
test in which every seeded regression's fault-revealing test is selected") is exactly the
right mechanism to empirically validate the tool's entire reason to exist — the
recall-first / over-approximation invariant (TIA-SAFE-001) — via mutation-style testing
rather than by inspection alone. Its ticket, `testaruda-q39`, correctly frames it as "the
verification gate for the soundness invariant." It is scoped at priority 2, which is
defensible (it needs the adapter protocol's priority-1 tickets to exist first, since a
seeded-fault test needs real dependency edges to select against) — but it is worth calling
out explicitly, since it is the one ticket in the entire backlog whose job is to *prove*
that everything else actually achieves the invariant the whole tool exists to guarantee,
and it should not slip behind priority-3 feature work (predictive ranking, Soufflé oracle,
environment fingerprinting) once priority-1/2 adapter work lands.

### 8.6 Updated verdict

**The plan, as currently written, is sound and is targeting the right things in the right
order.** The specification is complete (149/149 SRS↔openspec correspondence), internally
consistent (zero dangling cross-references), and — most importantly — the eight
safety-critical defects found in the previous evaluation round were genuinely fixed at the
root cause, not patched over, and the fixes propagate correctly through the derived
contract tree. The ticket backlog's priority-1 sequence (discover → static-deps/fingerprint
→ run-args/ingest, then CI exit codes) is exactly the minimal path to closing I1, the
single defect that made the tool structurally incapable of its core job. The three new
findings in §8.4 (S9, and backlog gaps A/B) are all real but all low-severity and
cheap to fix — they are the kind of gaps a careful pre-implementation review is *supposed*
to catch, which is itself a point in favor of the specification discipline on display here.
**Net recommendation: proceed with the current priority-1 ticket sequence as planned; file
three small follow-up tickets (S9's cold-start classification rule, mfx's dependency edge
onto 9uw plus a direct I2 regression check, and I6's justfile fix) before or alongside
that work, and keep `testaruda-q39` (seeded-fault recall) from sliding behind priority-3
feature tickets once the adapter protocol lands.**

---

## 9. Retest round — 2026-07-14: a real implementation landed, but one wire-format bug still blocks the core loop end-to-end

**Subject commit:** `da0072b` (tagged `v0.1.0`), up from `94fcc5c` — 16 commits, +7,520/−320
lines across `src/adapter.rs`, `src/agent.rs`, two new adapter binaries
(`src/bin/adapter-{rust,python}.rs`), `src/config.rs`, `src/store.rs`, and two new test files
(`tests/ordering.rs`, `tests/seeded_fault.rs`). Unlike the §7 retest (spec/tickets only, zero
code movement), this window is the opposite: real, substantial implementation work. Read-write
for the update step only — `git pull --ff-only` then `cargo build --release`; no source edits
were made to testaruda for this evaluation, everything below is black-box CLI exercise plus
read-only source inspection to find root causes. Unit test count rose from 6 to **86** (77 lib +
7 bin + 1 ordering + 1 seeded-fault), all passing. The beads backlog is **18/21 issues closed**
(17 more than §7's snapshot, which recorded 1 closed of 20 total); only 3 open, all priority-3 (Soufflé oracle bridge, environment
fingerprinting, graph export/import) — i.e., by the tracker's own account, every priority-1/2
safety-critical item is done.

### 9.1 Confirmed fixed: I3, I4, I6, I7, and part of I8

Each retested end-to-end against a fresh scratch git repo with the rebuilt `da0072b` binary:

- **I3 (CI exit codes absent)** — ✅ **fixed**. `select` with an empty selection now exits
  `20`; `select --shadow` exits `10`. Matches the SRS's now-fixed CI-003 union condition (S4).
- **I4 (changed-vs-unresolved classification flip)** — ✅ **fixed**. Running `select` twice on
  the same unchanged file now reports `changed_count: 0` both times (was `0` then `1`); editing
  the file's content correctly flips it to `changed_count: 1` on the next run. This matches the
  S9 fix suggested in §8.4 (idempotent cold-start classification) — verified via the store's
  `content_units` table, which now records one stable row with a real fingerprint (see I8)
  rather than being created inconsistently.
- **I6 (justfile does not parse)** — ✅ **fixed**. `just --list` now lists all 16 recipes
  cleanly (`ah`, `build`, `check`, `clean`, `clippy`, `default`, `doc`, `explain`, `fmt`,
  `fmt-check`, `ingest`, `init`, `release`, `select`, `test`, `test-v`); the stray
  Markdown/HTML block above the recipes is gone.
- **I7 (testaruda.toml never read)** — ✅ **fixed**. `src/config.rs` now parses
  `testaruda.toml` via `toml::from_str` and drives adapter-extension resolution; confirmed by
  reading the file and by `init`'s `testaruda.toml` actually being consulted at adapter-spawn
  time.
- **I8 (unused dependencies)** — 🟡 **partially fixed**. `blake3` (content fingerprinting,
  called via `blake3::hash(...)` in 3 files — both adapter binaries and `store.rs`) and `toml`
  (config parsing, called via `toml::from_str(...)` in 1 file, `config.rs`) are now genuinely
  used — the two dependencies that mattered most for I1/I7 are wired in. `similar`, `uuid`,
  `rayon`, `ascent-byods-rels`, and `serde_yaml` remain declared in `Cargo.toml` and referenced
  nowhere in `src/` (checked via each crate's actual `::` call syntax, not a bare-word search —
  a bare-word grep for "toml"/"blake3" overcounts by also matching unrelated mentions of the
  filename `testaruda.toml` in comments and strings).

### 9.2 🔴 I11 (new) — I1 and I2 still reproduce end-to-end, but the root cause has moved: a single response-envelope mismatch breaks every adapter handshake

This is the headline finding. The discover/static-deps/fingerprint/run-args/ingest logic that
I1 originally found completely absent has genuinely been implemented (`src/adapter.rs`,
932 lines; two real adapter binaries that each correctly discover tests and emit fingerprints
when invoked directly) — but the pipeline still cannot select a single test against a real
adapter (a separate binary, one per language, that testaruda spawns as a subprocess and talks
to over JSON lines on stdin/stdout — see §1's "Adapters" table), because of one concrete bug in
the very first exchange of that conversation, the **handshake**: before doing anything else,
testaruda sends `{"command":"handshake"}` and expects the adapter to answer with its name,
protocol version, and capabilities, so testaruda knows it is safe to proceed. Here is that
exchange failing:

```
$ testaruda select --files "src/foo.rs"
  ⚠️  Failed to spawn testaruda-adapter-rust: JSON error: missing field `name`
ℹ️  no tests selected
{ "changed_count": 1, "selected_count": 0, "tests": [] }
```

Root cause, isolated by reading `src/adapter.rs` and invoking the adapter binary directly:

```
$ echo '{"command":"handshake"}' | testaruda-adapter-rust
{"ok":true,"result":{"capabilities":{...},"granularity":"symbol",
  "languages":["rust"],"name":"rust-adapter","protocol":1,"version":"0.1.0"}}
```

Both adapter binaries correctly wrap **every** response in an `{"ok": bool, "result": {...}}`
envelope (verified in `src/bin/adapter-rust.rs`: `json!({"ok": true, "result": result})`, and
this envelope shape is exactly what `DiscoverResponse`/`StaticDepsResponse`/etc. in
`adapter.rs` expect — each has a `result: Option<...>` field). But the handshake path is the
one exception: `AdapterIO::spawn` does

```rust
let hs: Handshake = io.send(&AdapterCommand::handshake())?;   // adapter.rs:159
```

and `Handshake` is defined with `name`/`version`/`protocol` as **top-level** fields (no
`result` wrapper) — so deserializing the real envelope into `Handshake` fails with exactly the
observed `missing field \`name\`` error, `spawn()` returns `Err` before `discover()` is ever
called, and the entire adapter session is aborted at the first round-trip. Confirmed with a
minimal Python repro that deserializes the adapter's raw stdout directly: `'name' in resp` is
`False`, `'name' in resp['result']` is `True`. The bug is in `AdapterIO::spawn` itself, which is
shared code invoked identically regardless of which adapter binary is spawned, and both
`testaruda-adapter-rust` and `testaruda-adapter-python` were independently confirmed (by piping
the handshake command to each directly) to answer with the same enveloped shape — so the defect
affects every configured adapter, not just Rust's. One caveat worth naming precisely: in a
scratch repo with both a `.rs` and a `.py` file staged, `discover`/`select` only ever reported
the `testaruda-adapter-rust` failure and silently never attempted `testaruda-adapter-python` at
all (confirmed by temporarily removing the rust binary from `PATH`, which changed the error
from a JSON-parse failure to a "binary not found" failure — still only for rust). Whether that
is a second, adapter-fan-out defect or simply this test repo's layout not tripping the python
path was not tracked down further here; it does not change the diagnosis above, which was
verified directly against both binaries in isolation, not only inferred by analogy.

**This one bug is now the sole cause of both I1 and I2:** dropped a test item directly into
`store.db` with `sqlite3` (bypassing the broken handshake) and re-ran `ingest` against the
same payload that previously hit `FOREIGN KEY constraint failed` — it succeeded immediately
(`✅ Run ingested`, exit `0`). So I2 is not an independent defect anymore; it is a downstream
symptom of I11, exactly as I1 always was. Fixing the envelope mismatch in the handshake
deserialization path (either strip the envelope before decoding, or give `Handshake` the same
`result: Option<...>` shape as every other response type) should resolve both I1 and I2 in one
change.

**Why this survived 86 passing tests and a closed, fully-checked-off ticket
(`testaruda-9uw`, closed 2026-07-13, acceptance criterion "discover enumerates test items
stored in DB" checked `[x]`):** the unit test that exercises this exact deserialization,
`test_handshake_deserialization` (adapter.rs:637), hand-constructs the JSON **without** the
envelope and asserts it decodes — it never round-trips through a spawned subprocess, so it
cannot see the mismatch. The espectacular contract meant to guard this scenario,
`.espectacular/adapter-protocol/handshake-response.toml`, runs exactly that unit test
(`cargo test adapter::tests::test_handshake`) and nothing that spawns a real adapter binary.
This generalizes a concern from §7/§8 in a sharper form: even where a contract *was* narrowed
from the old blanket `cargo test` (closing part of the earlier "no-tests-ran everywhere"
criticism — `ah check` now reports 8/155 scenarios passing, up from 0/155), the specific named
test it was narrowed to still never crosses the process boundary the feature is actually about.
A contract can be "passing" and specific while still not testing the thing that matters.
**Suggested fix:** (a) fix the envelope bug (small, one struct or one deserialization step);
(b) add — and point the `handshake-response`/`test-discovery` contracts at — an integration
test that spawns the **real** `testaruda-adapter-rust` binary via `AdapterIO::spawn` and
asserts a live discover call returns test items, not a hand-built JSON string fed straight to
`serde_json::from_str`.

### 9.3 🟡 I12 (new) — the beads backlog reports the safety-critical work as fully done; it is not

`testaruda-9uw`, `testaruda-6js`, `testaruda-p84`, and `testaruda-gbs` — the four priority-1
tickets §8.3 identified as "exactly the right ones" to close I1 — are **all `closed`**, each
with every acceptance-criterion checkbox marked `[x]`, including `9uw`'s "discover enumerates
test items stored in DB." Per §9.2, that specific criterion is currently false in end-to-end
use: `testaruda discover` against a real project discovers **zero** test items
(`✅ Discovered 0 test items`) because the handshake fails before `discover()` is reached. The
ticket tracker and reality have diverged, and nothing in the tracker currently flags it — the
only open issues are unrelated priority-3 features. This is not a beads defect (the tool
faithfully recorded what the closing commit's author asserted); it is a project-process gap:
the acceptance criteria were verified against unit tests rather than the live CLI path they
describe. **Suggested fix:** reopen `testaruda-9uw` (or file a new P1 regression ticket) for
the handshake envelope bug, and add a live-CLI smoke check (`testaruda init && testaruda
discover` against a real fixture repo, asserting a non-zero discovered count) to the
acceptance criteria template for any future adapter-protocol ticket.

### 9.4 Other observations

- **`ah check` improved from 0/155 to 8/155 passing**, and gained 2 new `orphan-toml`
  findings for the two S9-motivated cold-start contracts — real, if incremental, progress on
  the specification-coverage gap flagged in §7.
- **`dont` was never initialized for this project** (`dont check --ungrounded` →
  `error: no .dont/ project found`) — a sharper version of the companion report's F20
  (`dont` decaying to zero use): here it never even reached day-1 init, despite the README's
  own "Tools" table listing it as part of the required suite. Zero claims exist to ground any
  of the correctness assumptions this evaluation had to independently re-derive (e.g., "the
  handshake response is enveloped like every other response" would have been exactly the kind
  of claim `dont ground --file src/bin/adapter-rust.rs` was built to pin down before it
  diverged from `adapter.rs`'s expectations).
- **`pretender check src/` still returns advisory-only findings, exit 0** (mostly long test
  functions, e.g. `test_quarantined_flag_in_selection_result` at 88 lines / ABC 56.5) —
  consistent with I9's original corroboration that pretender behaves correctly here; nothing
  new to report.

### 9.5 Updated verdict

**testaruda crossed the line from "non-functional scaffold" (§3) and "specs-and-tickets-only"
(§7) to "a real, substantially-implemented tool with one precise, high-leverage bug blocking
its core loop."** That is genuine, measurable progress: 86 passing tests (up from 6), 18/21
backlog issues closed, four of the eight §4 specification defects' code-level implications
(I3/I4/I6/I7) now demonstrably fixed, and `ah check` showing real (if small) movement off zero.
But the tool **still cannot select a single test against a real adapter today** — the single
handshake-envelope mismatch found in §9.2 sits directly on the critical path every other fix
in this round depends on, and it was invisible to both the test suite and the ticket tracker
that both currently report the adapter protocol as complete and verified. **Net recommendation:
fix the I11 envelope bug next — it is small, isolated to one deserialization call, and its
fix should mechanically also close I2 — then add the live-subprocess integration test
suggested in §9.2 before closing any further adapter-protocol tickets, so "tests pass" and
"the feature works" stop being able to diverge this far apart again.**
