<!-- SPECODELIC:START -->
## Specodelic — spec format rules (managed block)

This repo's `specs/`-style markdown spec files (YAML frontmatter +
fixed-schema tables) are linted by `spk` (crates.io: specodelic).
Write specs so `spk lint` passes; embedded format revision: specodelic.md Revision 18

### Lint rules (every violation names its `rule_id`)

- `linter.frontmatter_valid` — frontmatter `kind` must be `intent` — the only top-level intent kind
- `linter.id_matches_file` — frontmatter `id` must equal the filename stem with `-` mapped to `.` (`_` is literal); a `spec.md` file derives its expected id from its parent directory (Revision 18)
- `linter.unique_id` — every row id in a file must be unique across all of the file's tables
- `linter.guard_required` — every transition must carry a non-null guard (a guard may be prose, or cite an invariant Constraint or a State — target typing is `ref_kind_compatible`'s beat)
- `linter.model_present` — the Model section must contain both a States list and a Transitions table (empty-but-present beats absent)
- `linter.ears_syntax` — the intent statement must contain an imperative `SHALL` and match one of the five EARS patterns
- `linter.no_conjoined_id` — an id must not encode two capabilities joined by `and`/`or`
- `linter.no_universal_in_id` — an id must not contain a universal token (all/every/any/always/never)
- `linter.total_refs` — every structured-field [[link]] must resolve to a definition somewhere in the corpus (Revision 18: all files resolve corpus-wide — the former `id: spec` self-containment retired)
- `linter.coverage` — every constraint must have a deriving property (`∃ property.derives_from == <constraint>`)
- `linter.no_orphan_property` — every property must derive from at least one constraint
- `linter.law_cases` — every law-kind property must enumerate its required cases as **name:** labels in its own predicate — the identity and associativity floor is mandatory, extra named cases are checkable declarations
- `linter.requirement_drift` — a dual-format file's ## Requirements mirror must hold every delta requirement (ADDED and MODIFIED sections alike) with identical requirement text, compared per requirement so mixed-delta files are satisfiable (blank lines and trailing space ignored)
- `linter.dual_format_valid` — a file carrying `## ADDED Requirements` must be a dual-format file — pair it with a sibling `## Requirements` section (Revision 18: the id is the naming law's business, not this rule's)
- `linter.terminal_states_emit` — every failure terminal state must emit exactly one file-owned effect Constraint — a mute failure terminal is a finding (specs/linter-failure_shape.md; timed_out/exploration_only are the stated v1 non-goal)
- `linter.error_labels_unique` — within one file, no two error Constraints may share a variant head — the label is file-id-namespaced (errors.md error_expr_shape), so collisions are a per-file property
- `linter.guard_negation_total` — every failure transition must cite exactly the union of its success siblings' citation sets, or be on the recorded carve-out list (orchestrate.md's stage-fail transitions) — a zero-citation failure guard off the list is a finding
- `linter.every_state_used` — every declared state must appear as from or to in at least one transition — a state no transition reaches is machinery the model can never enter or leave
- `linter.every_transition_valid` — every transition's from and to must name states declared in the same file's States section
- `linter.no_self_ref` — a row must not reference itself via traces_to or derives_from — a self-tracing row has no owning purpose
- `linter.acyclic` — the directed graph formed by constraint-traces_to ∪ property-derives_from ∪ guard-as-edge must contain no cycle (derives_from edges are property-sourced — the Reference Typing Appears-on column is normative, so a Constraint-row derives_from is typing's beat, never an edge)
- `linter.single_root_reachable` — every constraint/property/state/transition row must reach its file's OWN intent row through own-file primary linkage (traces_to/derives_from chains resolved within the file, plus the model's own from/to/guard/emits edges) — cross-file typed edges (guard citations of foreign constraints, satisfies, observes) are outbound leaves, never reachability paths; tiered: cross-file-only rows warn (advisory, exit 0), rows with no path to ANY intent hard-fail
- `linter.observability` — every effect Constraint must be the target of ≥1 `observes` reference from a different row — advisory: warned on the warnings channel (exit 0), never a failure
- `linter.checklist_well_formed` — a declared checklist manifest (`*.checklist.md`) must be a flat item list with stable ids plus a mapping table with exactly item/status/mapped_ids/rationale columns — a manifest the linter cannot read is a checklist going silently unconsulted
- `linter.every_item_accounted` — every checklist item must have exactly one mapping row with status `covered` or `waived` — an unconsulted item is the failure this checker exists to prevent
- `linter.covered_maps_resolve` — a `covered` mapping row must name a non-empty mapped_ids list whose ids resolve to real constraint or property rows — a claim resting on nothing is not a claim
- `linter.waiver_has_rationale` — a `waived` mapping row must carry non-empty rationale prose — an unexplained waiver is an unconsulted item with extra steps
- `linter.no_duplicate_claim` — no two mapping rows may target the same checklist item — one claim per item, on the record
- `linter.constraint_kind_closed` — every Constraint row's kind must be in {invariant, advisory, effect, extension_point} — an unreadable kind cell is outside the closed set (specs/linter-schema_shape.md)
- `linter.property_kind_closed` — every Property row's kind must be in {unit, law} — an unreadable kind cell is outside the closed set (specs/linter-schema_shape.md)
- `linter.pack_shape` — a kind: profile pack file's manifest must carry all six facet tables (Sections/Kinds/References/Checkers/Floors/Requires) with well-formed two-column rows, its Kinds rows must be pack-qualified (never a base closed-set name — the narrowing rejection), and manifest tables may not appear on non-profile files (specs/packs.md, Revision 14)
- `linter.orphan_vocabulary` — orphan vocabulary is a labeled failure naming the candidate pack and both remediations (enable/declare the pack, or fix the vocabulary) — a declared uses edge targeting an id no discovered kind: profile pack carries, or a pack-qualified token used in a kind/field position with no discovered pack in its namespace (candidate prefix-derived when only the namespace is known) (specs/packs.md, Revision 14)
- `linter.skew_advisory` — a declared pack's Requires base pin older than the workspace corpus revision is a warnings-channel advisory naming the pack's base pin and the corpus revision — never silent, never failing (specs/packs.md, Revision 14)
- `linter.schema_matches_typing_table` — when the lint target carries the format doc, its Reference Typing table must equal the Schema value row for row — the document and the code cannot drift; a corpus without the format doc is out of the gate's scope (no-op, never fabricated expected rows) (add-acset-core, linter-schema_shape family)

### Commands

- `spk lint <dir>` — check the invariants (fails with a hint on zero files)
- `spk graph <dir>` — typed reference graph (state edges, typing
  violations, supersedes cycles; blast-radius lands later)
- `spk compile <files>` — emit TOML / proptest / TLA+ artifacts
- `spk model-check <files>` — run the model checker against compiled
  output (stateright; reports land as `*.check.json`)
- `spk explain [topic]` — the embedded format primer (works offline)
- `spk doctor` — diagnose workspace + block currency
- `spk feedback bug --dry-run` — file an issue against upstream

Refresh this block after upgrading: `spk init --force`.
<!-- SPECODELIC:END -->

<!-- BEADS:START -->
<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY.
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session
<!-- END BEADS INTEGRATION -->
<!-- BEADS:END -->

<!-- OPENSPEC:START -->
<!-- openspec: managed block -->
See openspec/ for spec-driven development.
<!-- OPENSPEC:END -->

<!-- DONT:START -->
<!-- dont: managed block -->
See .dont/ for grounded-claim workflow.
<!-- DONT:END -->

<!-- WAI:START -->
## PRIMARY OBJECTIVE

Build and maintain **testaruda** — the language-agnostic test selection engine
that computes the minimal set of tests to run from a code change, under a
recall-first soundness invariant. Every action should trace back to: does
this make testaruda more accurate at selecting the right tests, faster in
incremental evaluation, or easier to integrate into new language ecosystems?

# Workflow Tools

This project uses **wai** to track the *why* behind decisions — research,
reasoning, and design choices that shaped the code. Run `wai status` first
to orient yourself.

Detected workflow tools:
- **wai** — research, reasoning, and design decisions
- **beads** — issue tracking (tasks, bugs, dependencies). CLI command: **`bd`** (not `beads`)
- **openspec** — specifications and change proposals (see `openspec/AGENTS.md`)

> **CRITICAL**: Apply TDD and Tidy First throughout — not just when writing code:
> - **Planning/task creation**: each ticket should map to a red→green→refactor cycle; refactoring tasks must be separate tickets from feature tasks.
> - **Design**: define the test shape (inputs/outputs) before designing the implementation.
> - **Implementation**: write the failing test first, then make it pass, then tidy in a separate commit.

> **When beginning research or creating a ticket**: run `wai search "<topic>"` to check for existing patterns before writing new content.

## Quick Start

1. `wai sync` — ensure agent tools are projected
2. `wai status` — see active projects, phase, and suggestions
3. `bd ready` — find available work items

When context reaches ~40%: stop and tell the user — responses degrade past
this point. Recommend `wai close` then `/clear` to resume cleanly.
Do NOT skip `wai close` — it enables resume detection.



## Detailed Instructions

Full workflow reference — session lifecycle, capturing work, command cheat
sheets, cross-tool sync, and PARA structure — lives in **`.wai/AGENTS.md`**.
Read it at the start of your first session or when you need detailed guidance.

## PRIMARY OBJECTIVE (echo)

Build and maintain **testaruda** — the language-agnostic test selection engine
that computes the minimal set of tests to run from a code change, under a
recall-first soundness invariant. Every action should trace back to: does
this make testaruda more accurate at selecting the right tests, faster in
incremental evaluation, or easier to integrate into new language ecosystems?

Keep this managed block so `wai init` can refresh the instructions.

<!-- WAI:END -->

## Behavioral Constraints

These constraints are **persistent** — they live outside the WAI managed
block so they survive `wai init`. Do not remove or edit them without
deliberate intent.

### Prohibited (DON'T)

- **DON'T** change the selection algorithm's recall/soundness invariant without an openspec proposal
- **DON'T** push directly to main — all changes go through feature branches with PR review
- **DON'T** skip `testaruda select --safe` pre-push — the lefthook is there for a reason
- **DON'T** add new language adapters without adding Datalog provenance rules for the dependency model
- **DON'T** modify managed blocks (`<!-- WAI: -->`, `<!-- OPENSPEC: -->`, `<!-- DONT: -->`)
- **DON'T** commit the SQLite store (`.testaruda/store.db`) — it's a build artifact

### Stop and Ask

Pause and request human input when any of these triggers fire:
1. **Ambiguity** — the ticket text itself is contradictory or underspecified
2. **Scope uncertainty** — the ticket is clear but the change naturally touches code or features not mentioned in it
3. **Irreversibility** — breaking changes to the adapter protocol, store schema, or Ascent Datalog program
4. **Secrets/credentials** — any external service, API key, or credential not yet authorized
5. **Test failure persistence** — unresolved test failure after two repair attempts, or the same failure across 3 different approaches
6. **Push/release** — pushing to remote, creating a release, or deploying
7. **Context saturation** — context approaching ~40%; recommend `wai close` then `/clear`

### Minimal Footprint

- Prefer small, focused changes over large refactors — one ticket, one concern
- Delete unused code, don't leave commented-out code behind
- Keep PRs under 400 lines changed. If you cannot, split the work into multiple PRs before proceeding.
- Use existing abstractions (genesis, wai patterns) before introducing new ones
- testaruda is an engine — prefer correctness proofs over runtime assertions for the core algorithm

### Drift Detection

Proceed without routine confirmation when the next step is clear.
Do not ask to continue, fix, or commit — just do it. After each major
action (edit, test run, commit), pause and self-check:
1. **ALIGNMENT** — does this still serve selecting the right set of tests?
2. **SCOPE** — did I stay within the ticket scope or did I expand into unticketed work?
3. **FOOTPRINT** — did I leave dead code, debug prints, or unnecessary changes?
4. **GOVERNANCE** — did I follow openspec workflow for spec changes?

If any check fails: undo the last change (`git checkout -- <files>` for
uncommitted edits, `git revert HEAD` for committed) before proceeding,
or open a follow-up ticket.

<!-- WAI:REFLECT:REF:START -->
## Accumulated Project Patterns

Project-specific conventions, gotchas, and architecture notes live in
`.wai/resources/reflections/`. Run `wai search "<topic>"` to retrieve relevant
context before starting research or creating tickets.

> **Before research or ticket creation**: always run `wai search "<topic>"` to
> check for known patterns. Do not rediscover what is already documented.

<!-- WAI:REFLECT:REF:END -->

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

<!-- ah:managed:start -->
## espectacular

Run `ah check` to verify spec-test correspondence before committing.

- `ah check` — validate all deployed specs
- `ah check --changes <name>` — validate with a change overlay
- `ah init` — set up or refresh espectacular project files
- `ah doctor` — diagnose setup issues
- `ah explain <topic>` — playbook guidance for finding kinds and suggested actions
- `ah doctor --enable <adapter>` — write adapter config into .espectacular/config.toml
- `ah signals` — emit dont drift signals
<!-- ah:managed:end -->

## Git & Workflow Discipline

- **Never use `git add -A`** — always stage specific files with explicit paths
- **Per-ticket pipeline**: always follow `TDD → ro5u → fix → commit → next ticket`
