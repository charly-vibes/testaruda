# `tia` — Software Requirements Specification

**Notation:** EARS (Easy Approach to Requirements Syntax) · **Status:** Draft v0.2 · **Date:** 2026-06-15
**Changelog:** v0.2 adds §8 (Engine & Reference Architecture design constraints, `TIA-ENG-*`) and Appendix F (engine evaluation).
**Subject:** A language-agnostic CLI whose single responsibility is to compute, from a code change, the set of tests that must run — modeled as the transpose of a provenance-semiring dependency relation, evaluated incrementally, under a recall-first soundness invariant.

---

## 1. Conventions

### 1.1 EARS requirement patterns
Every requirement is written in one of the EARS templates. The generic form is
`<precondition(s)> <trigger> the <entity> shall <response>`.

| Tag | Pattern | Skeleton |
|-----|---------|----------|
| **[U]** | Ubiquitous | The `<entity>` **shall** `<response>`. |
| **[S]** | State-driven | **While** `<state>`, the `<entity>` **shall** `<response>`. |
| **[E]** | Event-driven | **When** `<trigger>`, the `<entity>` **shall** `<response>`. |
| **[O]** | Optional-feature | **Where** `<feature included>`, the `<entity>` **shall** `<response>`. |
| **[UW]** | Unwanted-behavior | **If** `<condition>`, **then** the `<entity>` **shall** `<response>`. |
| **[C]** | Complex | Combination of the above (e.g. While…When…). |

### 1.2 System entities
`the core` (language-agnostic engine), `the CLI` (user-facing command surface), `an adapter` (per-language/per-framework plugin), `the store` (persistence layer), `the selector` (selection engine), `the ingestor` (runtime-feedback path).

### 1.3 Identifiers and verification
Requirement IDs: `TIA-<GROUP>-<NNN>`. Each requirement carries a verification method:
**I** = Inspection, **A** = Analysis, **D** = Demonstration, **T** = Test. "shall" denotes a binding requirement.

### 1.4 References
[GKT07] Green, Karvounarakis, Tannen, *Provenance Semirings*, PODS 2007. · [BSalC] Mokhov, Mitchell, Peyton Jones, *Build Systems à la Carte*, ICFP 2018. · [CA19] Alvarez-Picallo, Eyers-Taylor, Peyton Jones, Ong, *Fixing Incremental Computation*, ESOP 2019. · [AI77] Cousot & Cousot, *Abstract Interpretation*, POPL 1977. · [FCA-RTS] Leelahapant, *Predictive Regression Test Selection via Formal Concept Analysis*, Concordia 2006.

---

## 2. Glossary

- **Artifact** — any tracked node: source file, symbol/block, config, fixture, lockfile, or external resource.
- **Content Unit (CU)** — an artifact at the chosen granularity, identified by `(component, path, symbol?)` and carrying a content **fingerprint**.
- **Test item** — an individually selectable/executable test, identified by `(component, adapter, node_id)`.
- **Edge** — a dependency `test_item → CU` carrying an **origin** ∈ {`static`, `runtime`, `manual`}, a confidence, and an environment binding.
- **Semiring K** — `(K, ⊕, ⊗, 0, 1)`. `⊕` fuses alternative dependency evidence; `⊗` composes along dependency paths.
- **Provenance semiring** — the free/polynomial semiring `ℕ[Edges]` (the master semiring); concrete semirings are obtained from it by homomorphism [GKT07].
- **K-relation** — the dependency data as a matrix over `K`.
- **Closure / star** `D*` — reflexive-transitive closure of the dependency relation; the least fixpoint of the reachability rule.
- **Transpose** `(D*)ᵀ` — the reversed relation; **selection** evaluates the change vector against the transpose.
- **Change set Δ** — the set of CUs whose fingerprint differs between base and head.
- **Affected set** — `Δ · (D*)ᵀ ⊕ always_run`, the tests selected to run.
- **Always-run set** — previously-failed ∪ newly-added ∪ no-history ∪ quarantined tests.
- **Environment** — a toolchain/OS/lockfile/env-var partition, keyed by a fingerprint.
- **Component** — a coarse package/service node (the parallelization and cross-repo unit).
- **Manifest** — a component's exported interface fingerprints + version, for cross-repo edges.
- **Soundness / over-approximation** — the modeled relation ⊒ the true semantic relation; guarantees recall.
- **Confidence** — a `[0,1]` signal that gates fallback; not a probability that the suite is correct.
- **Missed-selection incident** — a full run fails a test that selection would have skipped.
- **Shadow mode** — compute and record a selection but still run everything (non-gating).

---

## 3. Goals and Non-Goals (informative)

**Goals:** compute the affected test set from a diff; fuse static, runtime, and historical dependency evidence; serve CI, local-dev, and LLM-agent consumers; compose across monorepos and multiple repos; guarantee recall by construction and bound residual risk.
**Non-Goals:** the system is not a test runner, build system, coverage tool, or flaky-test fixer; it does not enforce hermetic builds; it does not mandate machine learning.

---

## 4. Architectural Constraints (ARCH)

> These bind the design to the unifying abstraction; downstream requirements instantiate them. They are **engine-independent**; the reference engine that realises them is pinned separately in §8 (`TIA-ENG-*`).

- **TIA-ARCH-001 [U]** — The core **shall** represent all dependency data as a single K-relation valued in a configured commutative semiring. *(V: I, A)*
- **TIA-ARCH-002 [U]** — The core **shall** treat the provenance (polynomial) semiring as the master representation from which all other semiring values are derived. *(V: I, A)*
- **TIA-ARCH-003 [U]** — The core **shall** derive concrete semiring results (selection, confidence, distance, cost) from the provenance representation by semiring homomorphism, such that no derived result can disagree with the master computation. *(V: A, T)*
- **TIA-ARCH-004 [U]** — The selector **shall** compute the affected set by evaluating the change set against the transpose of the transitive-closure relation. *(V: I, T)*
- **TIA-ARCH-005 [U]** — The core **shall** compute transitive dependency as the least fixpoint (semiring star) of the one-step dependency relation. *(V: I, T)*
- **TIA-ARCH-006 [U]** — The core **shall** compute selection incrementally as the change-propagation (derivative) of the reachability query, recomputing only relations affected by the change set rather than the whole graph. *(V: A, T)*
- **TIA-ARCH-007 [U]** — The core **shall** fuse static, runtime, and manual edges by semiring addition over a common edge set, without origin-specific merge logic. *(V: I, T)*
- **TIA-ARCH-008 [U]** — The core **shall** contain no language- or framework-specific logic; all such logic **shall** reside behind the adapter interface. *(V: I)*
- **TIA-ARCH-009 [U]** — The core **shall not** execute tests, compile, or build; it **shall** only select tests and emit native-runner arguments for an external executor. *(V: I, D)*
- **TIA-ARCH-010 [U]** — The core **shall** compute selection across components and repositories by associative composition of per-unit K-relations, such that the result is independent of composition order. *(V: A, T)*

---

## 5. Functional Requirements

### 5.1 Core data model (CORE)
- **TIA-CORE-001 [U]** — The store **shall** identify each content unit by the tuple `(component, path, symbol?)`. *(V: I)*
- **TIA-CORE-002 [U]** — The store **shall** record, for each content unit, a content fingerprint computed from the normalized unit content. *(V: T)*
- **TIA-CORE-003 [U]** — The store **shall** classify each content unit by kind ∈ {`source`, `config`, `fixture`, `lockfile`, `external`}. *(V: I)*
- **TIA-CORE-004 [U]** — The store **shall** identify each test item by the tuple `(component, adapter, node_id)`. *(V: I)*
- **TIA-CORE-005 [U]** — The store **shall** record each dependency edge as `(test_item, content_unit, environment, origin, K-value)`. *(V: I)*
- **TIA-CORE-006 [U]** — The store **shall** permit static, runtime, and manual edges to coexist for the same `(test_item, content_unit, environment)` triple. *(V: I, T)*
- **TIA-CORE-007 [U]** — The store **shall** maintain a reverse index from content unit to dependent test items. *(V: T)*
- **TIA-CORE-008 [U]** — The store **shall** partition all edges and statistics by environment fingerprint. *(V: T)*
- **TIA-CORE-009 [U]** — The store **shall** record per-test run history including outcome, attempt count, duration, and error signature. *(V: I)*

### 5.2 Change detection (CHG)
- **TIA-CHG-001 [E]** — When invoked with a base and head revision, the core **shall** derive the changed file set from the version-control diff between them. *(V: T)*
- **TIA-CHG-002 [O]** — Where the caller supplies an explicit changed-file list, the core **shall** use that list as the change source in place of a diff. *(V: T)*
- **TIA-CHG-003 [U]** — The core **shall** compute the change set Δ as the content units whose fingerprint differs between base and head. *(V: T)*
- **TIA-CHG-004 [O]** — Where the responsible adapter declares symbol granularity, the core **shall** fingerprint only the changed symbols/blocks so that whitespace-only or unrelated edits within a file yield no affected tests for unchanged symbols. *(V: T)*
- **TIA-CHG-005 [U]** — The core **shall** compute each test item's dependency fingerprint as a function of its dependency content-unit fingerprints and its environment fingerprint, and **shall** select a test item if and only if its dependency fingerprint changed or it is in the always-run set. *(V: T)*
- **TIA-CHG-006 [U]** — The core **shall** treat lockfiles, configuration, and adapter-declared external resources as content units subject to change detection. *(V: T)*
- **TIA-CHG-007 [UW]** — If a changed file has a kind the core cannot model and no known edges, then the core **shall** raise a fallback signal for the affected component. *(V: T)*
- **TIA-CHG-008 [S]** — While operating in CI mode, the core **shall not** read the local working tree as a change source. *(V: I, T)*

### 5.3 Selection engine (SEL)
- **TIA-SEL-001 [E]** — When a change set is supplied, the selector **shall** compute the affected set as the union of the transpose-closure of Δ and the always-run set. *(V: T)*
- **TIA-SEL-002 [U]** — The selector **shall** scope reverse traversal to components reachable from the change set before enumerating candidate tests. *(V: A, T)*
- **TIA-SEL-003 [O]** — Where a semiring is specified, the selector **shall** evaluate the same selection query in that semiring (Boolean → affected set, Viterbi → confidence, tropical → change-to-test distance, cost → expected duration). *(V: T)*
- **TIA-SEL-004 [U]** — The selector **shall** never remove a static edge from consideration on the grounds that runtime evidence did not confirm it. *(V: I, T)*
- **TIA-SEL-005 [O]** — Where deterministic ordering is requested, the selector **shall** emit the affected set in a stable, reproducible order. *(V: T)*
- **TIA-SEL-006 [O]** — Where a duration ordering is requested, the selector **shall** order selected tests by descending recorded mean duration. *(V: T)*
- **TIA-SEL-007 [O]** — Where predictive ranking is enabled, the selector **shall** apply ranking only as a re-ordering or cap over the already-computed recall-safe affected set, and **shall not** remove any always-run member. *(V: I, T)*

### 5.4 Provenance and explainability (PROV)
- **TIA-PROV-001 [U]** — The core **shall** compute, for each selection, the provenance expression that derived it. *(V: T)*
- **TIA-PROV-002 [E]** — When a test item is selected, the core **shall** be able to produce its reason chain as the set of edges and changed content units that caused its selection, each annotated with origin. *(V: T)*
- **TIA-PROV-003 [E]** — When a test item is excluded, the core **shall** be able to produce an explicit exclusion reason. *(V: T)*
- **TIA-PROV-004 [E]** — When queried about a specific test, the core **shall** report whether and why that test was or was not selected for the current change set. *(V: D, T)*
- **TIA-PROV-005 [U]** — The store **shall** persist provenance such that a past selection can be re-explained without re-running selection. *(V: T)*

### 5.5 Runtime feedback (RUN)
- **TIA-RUN-001 [E]** — When test results and coverage are ingested, the ingestor **shall** create or update runtime edges with origin `runtime`. *(V: T)*
- **TIA-RUN-002 [E]** — When coverage indicates a test exercised a content unit not linked by any static edge, the ingestor **shall** record that runtime edge so the dependency becomes selectable on future changes. *(V: T)*
- **TIA-RUN-003 [E]** — When an adapter reports external inputs read at runtime (config files, env vars, fixture files, reflectively loaded modules), the ingestor **shall** record them as runtime edges to the corresponding content units. *(V: T)*
- **TIA-RUN-004 [E]** — When results are ingested, the ingestor **shall** update per-test run history, timing, and failure-rate statistics. *(V: T)*
- **TIA-RUN-005 [U]** — The ingestor **shall** be idempotent with respect to re-ingestion of the same run. *(V: T)*
- **TIA-RUN-006 [S]** — While ingesting a run, the ingestor **shall** record the environment fingerprint under which the run executed. *(V: T)*

### 5.6 Soundness and safety (SAFE)
> TIA-SAFE-001 is the cardinal requirement; all other safety requirements exist to maintain it.

- **TIA-SAFE-001 [U]** — The core **shall** maintain the invariant that the modeled dependency relation over-approximates the true semantic dependency relation, such that any test that could be affected by a change is selected. *(V: A, T)*
- **TIA-SAFE-002 [UW]** — If selection confidence for a component falls below the configured threshold, then the core **shall** fall back to selecting all tests in that component. *(V: T)*
- **TIA-SAFE-003 [U]** — The core **shall** scope confidence-driven fallback to the affected component(s) and **shall not** escalate to a global full run unless every affected component is below threshold. *(V: T)*
- **TIA-SAFE-004 [UW]** — If an adapter reports unresolved files it cannot statically analyze, then the core **shall** treat the dependents of those files conservatively by force-including them or raising a component fallback. *(V: T)*
- **TIA-SAFE-005 [E]** — When the environment fingerprint or a lockfile changes, the core **shall** schedule a full run for the affected environment. *(V: T)*
- **TIA-SAFE-006 [U]** — The core **shall** support a configurable periodic full-run schedule independent of change-based selection. *(V: D)*
- **TIA-SAFE-007 [U]** — The core **shall** include in the always-run set every test that failed in its last recorded run, every newly added test, every test with no recorded history, and every quarantined test. *(V: T)*
- **TIA-SAFE-008 [E]** — When a full run fails a test that the most recent selection would have skipped, the core **shall** record a missed-selection incident and create a `manual` edge that forces the test on the implicated change in future. *(V: T)*
- **TIA-SAFE-009 [O]** — Where the user defines must-run rules (e.g. path globs mapped to tests), the core **shall** force-select the mapped tests when matching files change. *(V: T)*
- **TIA-SAFE-010 [U]** — The core **shall** treat a quarantined test as selected-and-run while excluding its outcome from pass/fail trust calculations; quarantine **shall not** mean skip. *(V: I, T)*
- **TIA-SAFE-011 [E]** — When a test produces inconsistent outcomes across retried attempts in one run, the core **shall** record the outcome as flaky and update its flakiness score. *(V: T)*
- **TIA-SAFE-012 [O]** — Where predictive ranking training is enabled, the core **shall** exclude flaky-labeled outcomes from the training labels. *(V: I, A)*

### 5.7 Confidence scoring (CONF)
- **TIA-CONF-001 [U]** — The core **shall** compute a selection confidence in the range `[0,1]`. *(V: T)*
- **TIA-CONF-002 [U]** — The core **shall** derive confidence from at least coverage freshness, adapter resolution ratio, history depth, and environment match. *(V: I, A)*
- **TIA-CONF-003 [U]** — The core **shall** report the confidence value to every consumer interface. *(V: T)*
- **TIA-CONF-004 [U]** — The core **shall** document, wherever confidence is reported, that confidence gates fallback and is not a probability that the suite is correct. *(V: I)*

### 5.8 Language adapter interface (ADAPT)
- **TIA-ADAPT-001 [U]** — The core **shall** communicate with adapters using JSON request/response over standard input and output, with diagnostics on standard error and status via exit code. *(V: I, T)*
- **TIA-ADAPT-002 [E]** — When the core starts an adapter, the adapter **shall** return a handshake declaring its name, version, supported protocol version, languages, granularity, and capability flags. *(V: T)*
- **TIA-ADAPT-003 [U]** — An adapter **shall** implement the commands `discover`, `static-deps`, `fingerprint`, `run-args`, and `ingest`. *(V: I, T)*
- **TIA-ADAPT-004 [E]** — When invoked with `discover`, an adapter **shall** enumerate test items in scope with their node id, suite kind, and file. *(V: T)*
- **TIA-ADAPT-005 [E]** — When invoked with `static-deps` and a changed-file set, an adapter **shall** return candidate test items, K-valued edges, and a list of files it could not resolve. *(V: T)*
- **TIA-ADAPT-006 [E]** — When invoked with `fingerprint`, an adapter **shall** return content fingerprints at its declared granularity. *(V: T)*
- **TIA-ADAPT-007 [E]** — When invoked with `run-args` and a selected set, an adapter **shall** return the native runner arguments and a collection path, and **shall not** execute the tests. *(V: I, T)*
- **TIA-ADAPT-008 [E]** — When invoked with `ingest` and a run's output, an adapter **shall** return runtime edges, per-test results, and observed external inputs. *(V: T)*
- **TIA-ADAPT-009 [U]** — An adapter **shall** emit dependency edges as semiring values, defaulting to the multiplicative identity where it has no finer weight. *(V: I)*
- **TIA-ADAPT-010 [UW]** — If an adapter does not declare a capability, then the core **shall** degrade gracefully for that capability rather than fail, applying conservative defaults. *(V: T)*
- **TIA-ADAPT-011 [UW]** — If an adapter's protocol version is incompatible with the core, then the core **shall** refuse to use it and report the mismatch. *(V: T)*
- **TIA-ADAPT-012 [UW]** — If an adapter fails, times out, or returns malformed output, then the core **shall** fall back to selecting all tests in the affected component and record the failure. *(V: T)*
- **TIA-ADAPT-013 [U]** — The core **shall** invoke adapters with least privilege and a configurable timeout. *(V: I, T)*

### 5.9 Composability (COMP)
- **TIA-COMP-001 [U]** — The store **shall** maintain a component graph distinct from the fine-grained test-to-content-unit graph. *(V: I)*
- **TIA-COMP-002 [U]** — The store **shall** record inter-component dependency edges with an origin. *(V: I)*
- **TIA-COMP-003 [E]** — When computing selection in a monorepo, the core **shall** first resolve affected components bottom-up from the change set, then select within them. *(V: T)*
- **TIA-COMP-004 [O]** — Where multi-repo operation is configured, each repository **shall** be able to export a manifest of its components, their public-interface fingerprints, and a version. *(V: T)*
- **TIA-COMP-005 [E]** — When a consumer repository records a dependency on another repository, the core **shall** record an edge to that repository's published interface fingerprint and version. *(V: T)*
- **TIA-COMP-006 [E]** — When a published interface fingerprint changes, the core **shall** mark dependent tests in consumer repositories as affected. *(V: T)*
- **TIA-COMP-007 [U]** — The core **shall** aggregate manifests across repositories without requiring a global lock or single shared database. *(V: A, D)*
- **TIA-COMP-008 [U]** — The core **shall** compute per-component selection in parallel. *(V: D, T)*
- **TIA-COMP-009 [U]** — The core **shall** produce identical affected sets regardless of the order in which components or repositories are composed. *(V: A, T)*
- **TIA-COMP-010 [U]** — The core **shall** key a component's cached selection decision on its dependency fingerprint, and **shall** reuse the cached decision when the fingerprint is unchanged. *(V: T)*
- **TIA-COMP-011 [O]** — Where a remote cache is configured, the core **shall** share and retrieve cached selection decisions across machines through a local-then-remote lookup. *(V: T)*
- **TIA-COMP-012 [O]** — Where sharding is requested, the core **shall** emit a balanced shard plan computed over recorded test durations. *(V: T)*

### 5.10 Persistence and store (STORE)
- **TIA-STORE-001 [U]** — The store **shall** persist the queryable index in an embedded transactional database. *(V: I)*
- **TIA-STORE-002 [U]** — The store **shall** persist large per-run payloads in a content-addressed blob store, deduplicated by content hash. *(V: T)*
- **TIA-STORE-003 [U]** — The store **shall** support export and import of the dependency graph and provenance in a documented interchange format. *(V: T)*
- **TIA-STORE-004 [E]** — When the store schema version differs from the running core, the core **shall** migrate or refuse with a clear diagnostic rather than corrupt data. *(V: T)*
- **TIA-STORE-005 [U]** — The store **shall** support concurrent reads during an in-progress write. *(V: T)*

### 5.11 CI consumer (CI)
- **TIA-CI-001 [E]** — When selection completes successfully, the CLI **shall** exit with code `0`. *(V: T)*
- **TIA-CI-002 [UW]** — If confidence requires a full run, then the CLI **shall** exit with code `10` to signal "run everything." *(V: T)*
- **TIA-CI-003 [E]** — When no tests are affected, the CLI **shall** exit with code `20` to signal "safe to skip." *(V: T)*
- **TIA-CI-004 [UW]** — If a non-recoverable error occurs, then the CLI **shall** exit with a non-zero code distinct from `10` and `20`. *(V: T)*
- **TIA-CI-005 [U]** — The CLI **shall** treat exit code `10` and any unknown condition as "run all tests," never as "run no tests." *(V: I, T)*
- **TIA-CI-006 [O]** — Where a machine-readable format is requested, the CLI **shall** emit the selection plan as JSON or a runner-native plan. *(V: T)*
- **TIA-CI-007 [O]** — Where shadow mode is enabled, the CLI **shall** compute and record the selection but report that all tests should run. *(V: D, T)*
- **TIA-CI-008 [E]** — When a CI run finishes, the CLI **shall** accept ingestion of its results to update the model. *(V: T)*

### 5.12 Local-developer consumer (LOCAL)
- **TIA-LOCAL-001 [E]** — When invoked locally against the working tree, the CLI **shall** compute selection from uncommitted changes. *(V: T)*
- **TIA-LOCAL-002 [O]** — Where a daemon is running, the CLI **shall** reuse a cached in-memory graph to return selection with low latency. *(V: D, T)*
- **TIA-LOCAL-003 [O]** — Where watch mode is enabled, the CLI **shall** recompute the affected set on each saved change. *(V: D)*
- **TIA-LOCAL-004 [U]** — The CLI **shall** always re-run tests that failed in the developer's most recent local run. *(V: T)*
- **TIA-LOCAL-005 [U]** — The CLI **shall** operate without network access in local mode. *(V: T)*

### 5.13 LLM-agent consumer (AGENT)
- **TIA-AGENT-001 [O]** — Where the agent output format is requested, the CLI **shall** emit a structured JSON object containing the selection, per-test reasons, confidence, changed units, and summary statistics. *(V: T)*
- **TIA-AGENT-002 [U]** — Given the same change set and store state, the CLI in agent mode **shall** produce byte-stable output. *(V: T)*
- **TIA-AGENT-003 [E]** — When the agent requests an explanation, the CLI **shall** include for each selected test its reason chain and for each skipped test its exclusion reason. *(V: T)*
- **TIA-AGENT-004 [E]** — When the agent queries a specific test, the CLI **shall** answer why that test was or was not selected. *(V: T)*
- **TIA-AGENT-005 [O]** — Where pre-edit mode is requested, the CLI **shall** report the blast radius of a proposed change without requiring the edit to be applied. *(V: D, T)*
- **TIA-AGENT-006 [E]** — When a changed symbol has no covering test, the CLI **shall** surface that coverage gap in agent output. *(V: T)*
- **TIA-AGENT-007 [S]** — While serving an agent or a merge gate, the CLI **shall** default to deterministic selection and **shall not** apply non-deterministic predictive ranking unless explicitly enabled. *(V: I, T)*

### 5.14 Observability (OBS)
- **TIA-OBS-001 [E]** — When requested, the core **shall** export the current dependency graph in a documented format. *(V: T)*
- **TIA-OBS-002 [U]** — The core **shall** be able to explain any selection it produced. *(V: D)*
- **TIA-OBS-003 [U]** — The core **shall** emit metrics including selection rate, estimated time saved, fallback rate, flakiness rate, and missed-selection count. *(V: T)*
- **TIA-OBS-004 [U]** — The core **shall** emit structured logs for each selection and ingestion. *(V: I)*

---

## 6. Non-Functional Requirements

### 6.1 Performance (PERF)
- **TIA-PERF-001 [U]** — The core **shall** scale incremental selection time with the size of the change set rather than the size of the full test suite. *(V: A, T)*
- **TIA-PERF-002 [S]** — While a warm local daemon is available, the CLI **shall** return selection for a single-file change within an interactive latency budget. *(V: T)*
- **TIA-PERF-003 [U]** — The core **shall** bound its own selection overhead to a small fraction of the time saved by running fewer tests. *(V: A)*

### 6.2 Reliability (REL)
- **TIA-REL-001 [S]** — While in deterministic mode, the core **shall** produce identical selections for identical inputs and store state. *(V: T)*
- **TIA-REL-002 [U]** — The core **shall** make ingestion idempotent and crash-safe, leaving the store consistent after an interrupted operation. *(V: T)*

### 6.3 Security (SEC)
- **TIA-SEC-001 [U]** — The core **shall not** execute arbitrary repository code as part of selection. *(V: I, A)*
- **TIA-SEC-002 [U]** — The core **shall** run adapters under least privilege with bounded resource limits. *(V: I, T)*
- **TIA-SEC-003 [U]** — The core **shall** include only an allowlisted, hashed representation of environment variables in cache and environment keys, and **shall not** store raw secret values. *(V: I, T)*

### 6.4 Portability and maintainability (PORT)
- **TIA-PORT-001 [U]** — The core **shall** be usable across languages and test frameworks solely through adapters, with no core changes required to add a language. *(V: I, D)*
- **TIA-PORT-002 [U]** — The adapter protocol **shall** permit adapters to be implemented in any language and versioned independently of the core. *(V: I)*
- **TIA-PORT-003 [U]** — The core **shall** declare a protocol-compatibility policy and reject adapters outside the supported version range. *(V: I, T)*

### 6.5 Scalability (SCALE)
- **TIA-SCALE-001 [U]** — The core **shall** support monorepos containing many components without recomputing the full graph per change. *(V: A, T)*
- **TIA-SCALE-002 [U]** — The core **shall** support federation across multiple repositories without a single shared write bottleneck. *(V: A, D)*

---

## 7. Verification and Rollout

- **TIA-VER-001 [U]** — The system **shall** be verifiable in shadow mode, computing selections without gating, before it is permitted to gate. *(V: D)*
- **TIA-VER-002 [U]** — The system **shall** record zero missed-selection incidents over a defined evaluation window as the precondition for enabling enforcing mode. *(V: A, T)*
- **TIA-VER-003 [U]** — The system **shall** use periodic full-run reconciliation as a continuous verification mechanism for the over-approximation invariant. *(V: A, D)*
- **TIA-VER-004 [U]** — The soundness invariant (TIA-SAFE-001) **shall** be verified by a seeded-fault recall test in which every seeded regression's fault-revealing test is selected. *(V: T)*
- **TIA-VER-005 [O]** — Where predictive ranking is enabled, it **shall** pass a calibration gate meeting defined test-failure-recall and change-recall targets on a held-out recent window before promotion. *(V: T)*

---

## 8. Engine and Reference Architecture (Design Constraints)

> §4 (ARCH) is binding and **engine-independent**: it constrains the abstraction (a provenance-semiring K-relation, selection as its transpose, soundness as over-approximation). This section pins the **reference implementation** that realises §4 with the least code. `TIA-ENG-*` requirements bind the reference build; they do not constrain the abstraction, which remains portable per TIA-ENG-012.

### 8.1 Decision
The reference implementation is a single Rust binary embedding **Ascent** (a Datalog-style logic language with lattice support) as the in-process selection engine. Cross-invocation incrementality is provided by change scoping plus the content-addressed component cache (TIA-COMP-010), **not** by a persistent streaming engine. **Soufflé** serves as an out-of-process provenance oracle for validation and full why-provenance. **DBSP/Feldera** is the documented scale-up path for long-running, fully-incremental, retraction-aware selection. **DDlog** is rejected (archived/unmaintained).

### 8.2 Concern → mechanism

| Spec concern | Reference mechanism |
|---|---|
| Transitive closure (ARCH-005) | Ascent recursive rule, union-find–backed relation (BYODS) |
| Semiring layer (ARCH-001/003, SEL-003) | Ascent `lattice` columns: Boolean (selection), Viterbi-max (confidence), tropical-min (distance) |
| Incremental selection (ARCH-006, PERF-001) | change scoping + component cache (COMP-010); batch over the scoped residual |
| Parallel per-component (COMP-008) | Ascent `ascent_par!` on rayon |
| Explanation (PROV-002, AGENT-003) | minimal-witness derivation as a lattice value |
| Full why-provenance / validation | out-of-process Soufflé oracle |
| Persistence (STORE) | SQLite + CAS (§5.10); engine holds working state only |
| Scale-up to streaming IVM | re-target rule set to DBSP |

### 8.3 Design-constraint requirements (ENG)
- **TIA-ENG-001 [U]** — The reference implementation **shall** be distributed as a single statically linked binary that embeds the selection engine in-process. *(V: I, D)*
- **TIA-ENG-002 [U]** — The reference implementation **shall not** require a separate engine process to compute a selection. *(V: I)*
- **TIA-ENG-003 [U]** — The reference implementation **shall** use Ascent as the embedded logic engine for the selection query. *(V: I)*
- **TIA-ENG-004 [U]** — The reference implementation **shall** represent each semiring as a lattice column so that selection, confidence, and distance are produced by the same rule set under different lattice types. *(V: I, T)*
- **TIA-ENG-005 [U]** — The reference implementation **shall** compute transitive dependency closure using a union-find–backed relation. *(V: T)*
- **TIA-ENG-006 [U]** — The reference implementation **shall** evaluate per-component selection using the engine's data-parallel mode. *(V: D, T)*
- **TIA-ENG-007 [U]** — The reference implementation **shall** provide cross-invocation incrementality through change scoping and the content-addressed component cache (TIA-COMP-010) rather than a persistent streaming engine. *(V: A, T)*
- **TIA-ENG-008 [U]** — The reference implementation **shall** evaluate the selection query over only the change-scoped residual subgraph. *(V: A, T)*
- **TIA-ENG-009 [U]** — For explanation, the reference implementation **shall** compute a minimal-witness derivation (a single shortest reason chain) as a lattice value sufficient to satisfy TIA-PROV-002 and TIA-AGENT-003, without requiring full why-provenance for routine selection. *(V: T)*
- **TIA-ENG-010 [O]** — Where full why-provenance or independent validation is required, the reference implementation **shall** be able to evaluate the same rule set through an out-of-process Soufflé oracle. *(V: D)*
- **TIA-ENG-011 [S]** — While operating in shadow mode (TIA-VER-001), the reference implementation **shall** be able to cross-check its selections against the Soufflé oracle and flag divergences. *(V: D, T)*
- **TIA-ENG-012 [U]** — The selection rule set **shall** be engine-independent, such that it can be re-targeted to an alternative engine without changing the dependency model or any `TIA-ARCH-*` behaviour. *(V: A)*
- **TIA-ENG-013 [O]** — Where long-running, fully-incremental selection at monorepo scale with retractions is required, the system **shall** be re-targetable to a streaming incremental engine (e.g. DBSP) as a documented scale-up path. *(V: A)*
- **TIA-ENG-014 [UW]** — If a selected engine becomes unmaintained, then the system **shall** be re-targetable to an alternative engine through the engine-independent rule set without loss of `TIA-ARCH-*` behaviour. *(V: A)*
- **TIA-ENG-015 [U]** — The reference implementation **shall** persist the dependency graph and statistics in the store of §5.10 independently of the engine's in-memory working state. *(V: I)*
- **TIA-ENG-016 [U]** — The reference implementation core **shall** be memory-safe and **shall** isolate untrusted adapter execution to subprocesses (cf. TIA-SEC-002), keeping the engine free of repository code execution. *(V: I, A)*

### 8.4 Rejected alternatives (informative)
**DDlog** — cleanest conceptual match (incremental Datalog on differential dataflow) but archived; rejected on maintenance risk (see TIA-ENG-014). **Pure DBSP/differential-dataflow for v1** — true IVM, but Z-sets are an abelian *group* (signed weights) whereas the master provenance semiring has no additive inverses, making why-provenance awkward; also heavier than the cache-based incrementality requires. Retained as scale-up only (TIA-ENG-013). **Soufflé as the primary engine** — gold-standard built-in provenance, but compiles to C++ and is not a Rust-embeddable library; retained as oracle (TIA-ENG-010). **CozoDB** — bundles engine + persistence, but semiring/provenance are not first-class; viable fast-prototype substitute, not the reference.

---

## Appendix A — Semiring instantiations

| Quantity | Semiring K | ⊕ | ⊗ | 0 | 1 | Yields |
|----------|-----------|---|---|---|---|--------|
| Affected set | Boolean | ∨ | ∧ | false | true | which tests to run |
| Confidence | Viterbi `[0,1]` | max | × | 0 | 1 | selection confidence |
| Distance | Tropical `(ℝ⁺∪∞)` | min | + | ∞ | 0 | change-to-test graph distance |
| Explanation | Provenance `ℕ[Edges]` | + | × | 0 | 1 | reason chains (master) |
| Scheduling | Cost / expected-time | min | + | ∞ | 0 | shard ordering |

All concrete columns are homomorphic images of the provenance column (TIA-ARCH-003).

## Appendix B — CLI exit codes

| Code | Meaning | CI action |
|------|---------|-----------|
| 0 | Selection computed | run selected set |
| 10 | Low confidence / fallback | run all tests |
| 20 | No tests affected | safe to skip stage |
| other ≠ 0 | Hard error | run all tests |

## Appendix C — Adapter command summary

| Command | Input | Output |
|---------|-------|--------|
| handshake (`adapter-info`) | — | name, version, protocol, languages, granularity, capabilities |
| `discover` | paths, component | test items |
| `static-deps` | changed files | candidate tests, K-edges, unresolved files |
| `fingerprint` | files/symbols | content fingerprints |
| `run-args` | selected set | native runner argv, collection path |
| `ingest` | run output, coverage | runtime edges, results, external inputs |

## Appendix D — Verification legend
**I** Inspection · **A** Analysis · **D** Demonstration · **T** Test.

## Appendix E — EARS pattern legend
**[U]** Ubiquitous · **[S]** State-driven (While) · **[E]** Event-driven (When) · **[O]** Optional-feature (Where) · **[UW]** Unwanted-behavior (If/Then) · **[C]** Complex.

## Appendix F — Engine evaluation (informative)

Evaluated against the abstraction in §4 and the "easy to implement" priority. The decisive reframing: **"incremental" is satisfiable by change-scoping + the component cache (COMP-010), not only by a streaming IVM engine** — which favours an embeddable batch engine with lattice support over a heavier streaming one.

| Engine | Embed / lang | "Incremental" | Semiring / lattice | Provenance | Persistence | Status | Role |
|---|---|---|---|---|---|---|---|
| **Ascent** | Rust macro, in-binary | batch + cache | **native `lattice`** | build (minimal-witness) | none (use §5.10) | active | **reference engine** |
| **Soufflé** | C++, compile/shell-out | batch; IVM in research forks | limited | **native proof trees** | none | mature | validation oracle |
| **CozoDB** | Rust, in-binary | batch | aggregations | not native | **built-in (MVCC)** | active | prototype substitute |
| **DBSP / Feldera** | Rust lib | **true IVM (Z-sets)** | group, not semiring | not native | via platform | active | scale-up path |
| **DDlog** | Rust (generated) | true IVM | — | — | — | **archived** | rejected |

**Why Ascent for the reference build:** its `lattice` feature *is* the semiring layer (its shortest-path `Dual<u32>` example is the tropical distance semiring), it embeds with no FFI or separate process, it parallelises via rayon, and BYODS union-find gives efficient transitive closure — so transitive closure, the semiring instantiations (SEL-003), and parallel selection (COMP-008) fall out of the tool. The one gap, provenance, is closed with a minimal-witness lattice column (TIA-ENG-009); full why-provenance is delegated to the Soufflé oracle (TIA-ENG-010). Because the rule set is engine-independent (TIA-ENG-012), this choice is reversible.

## Appendix G — Reference rule set (verified)

The entire core selection path is **40 lines** of Ascent declarations + rules (104 lines including a worked example and assertions). It compiles and runs on rustc/cargo 1.75 with ascent 0.8; the full runnable file is `tia_core_selection.rs`. The rule block:

```rust
ascent! {
    // EDB (scoped residual): facts loaded from the store
    relation changed(u32);                    relation unresolved(u32);
    relation cu_dep(u32, u32, Origin, u32);   relation test_dep(u32, u32, Origin, u32);
    relation always_run(u32);  relation comp_fallback(u32);  relation test_comp(u32, u32);

    // Boolean selection = reverse reachability (ARCH-004/005, SEL-001)
    relation impacted(u32);  relation affected(u32);
    impacted(c) <-- changed(c);
    impacted(c) <-- unresolved(c);                          // over-approximate (SAFE-004)
    impacted(a) <-- cu_dep(a, b, _, _), impacted(b);
    affected(t) <-- test_dep(t, c, _, _), impacted(c);
    affected(t) <-- always_run(t);                          // SAFE-007 union
    affected(t) <-- comp_fallback(k), test_comp(t, k);      // fallback

    // Confidence (Viterbi: lub = max, product along path) — ppm integers
    lattice impact_conf(u32, u32);  lattice test_conf(u32, u32);
    impact_conf(c, ONE) <-- changed(c);
    impact_conf(c, ONE) <-- unresolved(c);
    impact_conf(a, ((*w as u64 * *d as u64)/ONE as u64) as u32) <-- cu_dep(a,b,_,w), impact_conf(b,d);
    test_conf(t,  ((*w as u64 * *d as u64)/ONE as u64) as u32) <-- test_dep(t,c,_,w), impact_conf(c,d);
    test_conf(t, ONE) <-- always_run(t);

    // Distance (tropical min-plus: Dual flips lub to min)
    lattice impact_dist(u32, Dual<u32>);  lattice test_dist(u32, Dual<u32>);
    impact_dist(c, Dual(0)) <-- changed(c);
    impact_dist(c, Dual(0)) <-- unresolved(c);
    impact_dist(a, Dual(d+1)) <-- cu_dep(a,b,_,_), impact_dist(b, ?Dual(d));
    test_dist(t, Dual(d+1)) <-- test_dep(t,c,_,_), impact_dist(c, ?Dual(d));

    // Minimal-witness predecessors (TIA-ENG-009): edges on a shortest reason chain
    relation cu_pred(u32, u32, Origin);  relation test_pred(u32, u32, Origin);
    cu_pred(a,b,o) <-- cu_dep(a,b,o,_), impact_dist(a, ?Dual(da)), impact_dist(b, ?Dual(db)), if *da == *db+1;
    test_pred(t,c,o) <-- test_dep(t,c,o,_), test_dist(t, ?Dual(dt)), impact_dist(c, ?Dual(dc)), if *dt == *dc+1;
}
```

**Worked example output** (developer edits `session.py`; `test_totp` reaches it *only* through a runtime-observed edge that static import analysis cannot see — TIA-RUN-002):

```
affected = [100, 102, 900]
  test 100: conf=0.640 dist=Some(2) witness=[(11,Static),(10,Static)]   # static path
  test 102: conf=1.000 dist=Some(2) witness=[(20,Runtime),(10,Runtime)] # RUNTIME-only path
  test 900: conf=1.000 dist=None    witness=None                        # always-run
ALL ASSERTIONS PASSED
```

`test_invoice` (101) is correctly excluded (precision); `test_totp` (102) is caught solely by the runtime edge (the fusion thesis); confidence `0.640 = 0.8×0.8` is the Viterbi product; distance is the tropical hop count; the witness is the minimal reason chain. One EDB load + `p.run()` per invocation; swap `ascent!`→`ascent_par!` for the parallel evaluator (COMP-008). Cross-run incrementality is the cache (TIA-ENG-007), not the engine.
