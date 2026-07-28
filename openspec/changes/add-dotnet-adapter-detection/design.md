# Design: Add .NET adapter detection (external `titi` binary)

## Context

testaruda supports Rust, Python, TypeScript, and Clojure adapters as in-workspace
Rust crate binaries, and a Julia adapter hosted in Testimonial.jl (invoked as
`julia --project=. -e '...'`). There is no C#/.NET adapter. The titi project
(`github.com/sashakile/titi`) is a separate ClojureCLR-based .NET monorepo
orchestrator that already builds a `MonorepoGraph` via MSBuild `ProjectGraph`,
computes an `AffectedSet` from git diffs, and exposes this as a
`titi testaruda-adapter` subcommand speaking testaruda's JSON-over-stdio
adapter protocol (titi change `add-testaruda-adapter`, CLI-19).

This change registers titi as an external adapter, mirroring the Julia
adapter's pattern, and adds the core shell-split infrastructure both external
adapters require.

## Goals / Non-Goals

**Goals:**
- Register the .NET adapter via a `testaruda.toml` command-string mapping,
  following the Julia adapter's opt-in precedent.
- Route .NET source files (`.cs`/`.fs`/`.vb`) plus project/solution files to
  titi so changed source reaches titi's `AffectedSet`.
- Add the shell-split step that all command-string adapters require,
  unblocking the Julia adapter (TIA-ADAPT-014) as well.

**Non-Goals:**
- Auto-detecting .NET at `testaruda init` (opt-in, following Julia).
- Implementing the titi adapter itself (lives in the titi repo, CLI-19).
- A `testaruda-adapter-dotnet` workspace crate (rejected — MSBuild machinery
  cannot live in a Rust crate).

## Decisions

### Decision 1: External binary, not a workspace crate

The .NET adapter is an external binary (`titi testaruda-adapter`), not a
workspace crate. MSBuild `ProjectGraph` evaluation, `.csproj` parsing, reference
resolution, and graph caching all depend on `Microsoft.Build.Locator`
(process-global, .NET-runtime-only) and cannot live in a Rust crate. titi
already implements this machinery in ClojureCLR.

- **Alternatives considered:** (1) A `testaruda-adapter-dotnet` Rust crate
  that re-implements MSBuild evaluation — rejected as pure duplication of
  titi's capability plus an impossible .NET-runtime dependency. (2) Embedding
  the .NET adapter in testaruda's core — violates TIA-ARCH-008.
- **Precedent:** The Julia adapter (TIA-ADAPT-014) established this pattern —
  hosted in Testimonial.jl, invoked as `julia --project=. -e '...'`.

### Decision 2: Command-string config form, shell-split by the core

The `testaruda.toml` extension-mapping value is a command string (e.g.
`"titi testaruda-adapter"`), which the core shell-splits into `(binary, args)`
before `AdapterIO::spawn`. This follows the Julia adapter's config form
(`"julia --project=. -e '...'"`) and avoids introducing a structured `args`
field — the extension-mapping value remains a single string, as the Julia
adapter already assumes.

- **Crate choice:** `shell-words` (not `shlex`). POSIX-ish quoting, sufficient
  for both known command-string adapters (`titi testaruda-adapter` and
  `julia --project=. -e '...'`), no surprising shell expansions (no env-var
  expansion, no command substitution) in a config file. `shlex` is closer to
  POSIX shell but offers more than needed and could introduce surprising
  semantics for config-file values.
- **Spawn-site coverage:** There are five `AdapterIO::spawn` call sites in
  `src/main.rs` (lines 448, 579, 803, 858, 904) that consume configured adapter
  strings. All five pass `&[]` args today, which only works for bare-binary
  adapters. A shell-split applied to only two sites (the initial proposal's
  error) would leave the discover/full-walk paths (803, 858, 904) still passing
  the command string verbatim as a binary path, causing NotFound for
  command-string adapters. All five sites SHALL be routed through a single
  `spawn_adapter(command_string, timeout)` helper so the split is applied by
  construction.

### Decision 3: Route .NET source extensions (`.cs`/`.fs`/`.vb`), not just project files

testaruda routes changed files to adapters strictly by extension
(`registry.resolve` in `src/config.rs`), and titi receives the changed-file set
from testaruda (titi CLI-19 "Static-deps uses titi's AffectedSet"). Without
`.cs`/`.fs`/`.vb` mapped, typical .NET source changes would never reach titi and
the adapter would compute an empty affected set — functionally broken.

- **Extensions to map:** `.cs`, `.fs`, `.vb` (source), `.csproj` (project),
  `.sln`, `.slnx` (solution). titi's MSBuild graph is language-neutral per the
  titi project.md, so F# and VB source files are routed too.
- **Polyglot routing:** In a polyglot repo, a `.cs` file outside any `.csproj`
  in titi's `MonorepoGraph` is routed to titi, returned by titi as `unresolved`
  in its `static-deps` response, and handled by testaruda's over-approximation
  (TIA-SAFE-004). Polyglot repos SHOULD map only .NET extensions to titi,
  leaving other languages' extensions mapped to their own adapters.
- **Alternatives considered:** Setting `default = "titi testaruda-adapter"` to
  catch all unmapped files — rejected; in a polyglot repo this routes non-.NET
  files to titi too, polluting titi's change set.

### Decision 4: Opt-in, not auto-detected (following the Julia precedent)

The Julia adapter is explicitly opt-in because Testimonial.jl is a separate
install ("requiring it as a hard dependency for auto-detection would be
misleading"). titi is likewise a separate install, so .NET is NOT added to the
TIA-CHG-009 init-time probe. `testaruda init` in a .NET repo continues to fall
back to `testaruda-adapter-rust` (or whatever other marker wins); the user adds
the titi mapping manually.

- **Alternatives considered:** Adding `.slnx`/`Directory.Packages.props` to the
  TIA-CHG-009 probe — rejected because it contradicts the Julia precedent and
  would set titi as a default in repos where titi is not installed, producing
  confusing not-found errors at `select` time.

### Decision 5: `runtime_edges = false` for Phase 1 (pinned in titi CLI-19)

titi's Phase 1 adapter uses its MSBuild `MonorepoGraph` for static edges; the
`ingest` command parses TRX for per-test outcomes but does not construct runtime
dependency edges. Runtime-edge construction is deferred to Phase 2, where
titi's `add-test-item-detection` capability provides coverage-based
`TestToSourceEdge` instances (DM-08). The `runtime_edges = false` value is
pinned in the titi adapter spec (CLI-19) and referenced from TIA-ADAPT-023, so
the two repos agree on the handshake without either side guessing.

## Risks / Trade-offs

| # | Risk | Impact | Mitigation |
|---|------|--------|------------|
| 1 | `shell-words` Windows behavior with backslash paths | Adapter fails to spawn on Windows | Task 1.7: verify on Windows (CI job or documented behavior) |
| 2 | Polyglot repo routes non-.NET files to titi if `default` is set | titi receives irrelevant files, returns `unresolved`, over-approximation inflates selection | Decision 3: map only .NET extensions, do NOT set `default = titi` |
| 3 | Five spawn sites partially covered by shell-split | Some code paths work, others NotFound | Decision 2: single `spawn_adapter` helper covers all five sites by construction |
| 4 | titi not installed at `select` time | Adapter failure | TIA-ADAPT-012 fallback (full-suite); already handled by existing `AdapterIO::spawn` NotFound path once shell-split is in place |
| 5 | `runtime_edges = false` may contradict a future titi Phase 2 | Handshake mismatch when titi upgrades to runtime edges | Coordinate with titi: Phase 2 will update CLI-19 to `runtime_edges = true`; this spec references CLI-19 rather than pinning independently |

## Migration Plan

1. Implement the shell-split core change (TIA-ADAPT-024, tasks 1.1–1.8) —
   unblocks both Julia and .NET.
2. Document the .NET extension mappings (task 2.1).
3. Verify error paths (tasks 3.1–3.2).
4. Run integration tests (tasks 4.1–4.5), gated on titi being installed.
5. Update docs (tasks 5.1–5.2).
6. Coordinate with titi (`add-testaruda-adapter` CLI-19) and the Julia adapter
   change owner (tasks 6.1–6.3).
7. Rollback: removing the shell-split helper and the .NET mapping is a clean
   revert; no existing adapter is affected (bare-binary adapters produce
   `binary = <string>, args = []` from the split, equivalent to current
   behavior).

## Open Questions

1. Should the `spawn_adapter` helper live in `src/main.rs` or `src/adapter.rs`?
   (Leaning `src/adapter.rs` — it's adapter-invocation logic, not CLI logic.)
2. Should the empty-command-string case (TIA-ADAPT-024) be a config-load-time
   rejection or a runtime adapter-failure? (Spec currently says fall back per
   TIA-ADAPT-012; a config-load rejection would surface the error earlier.)
3. Will `shell-words` correctly handle the Julia `-e` body containing `;` and
   `.` on all platforms? (Task 1.4's unit test will confirm.)
