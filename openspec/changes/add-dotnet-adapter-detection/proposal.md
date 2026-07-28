# Change: Add .NET adapter detection (external `titi` binary)

## Why

testaruda has no C#/.NET adapter. The titi project (a separate ClojureCLR-based
.NET monorepo orchestrator at `github.com/sashakile/titi`) already solves the
hard, .NET-specific half of test-impact analysis: it builds a full
`MonorepoGraph` from `.csproj` files via MSBuild `ProjectGraph`, computes an
`AffectedSet` from git diffs, and emits test-scoped MSBuild Traversal projects.
titi exposes this as a `titi testaruda-adapter` subcommand that speaks
testaruda's JSON-over-stdio adapter protocol (see titi change
`add-testaruda-adapter`, spec CLI-19).

Building a `testaruda-adapter-dotnet` crate *inside* the testaruda workspace
would mean re-implementing MSBuild project-graph evaluation, `.csproj` parsing,
reference resolution, and graph caching — machinery titi already has and that
depends on `Microsoft.Build.Locator` (process-global, .NET-runtime-only). That
machinery cannot live in a Rust crate. This change therefore registers titi as
an **external adapter binary** — invoked as `titi testaruda-adapter` via a
`testaruda.toml` extension mapping — mirroring the pattern already established
by the Julia adapter (`add-julia-adapter-via-testimonial`, TIA-ADAPT-014), which
is hosted inside Testimonial.jl and invoked as `julia --project=. -e '...'`.

## What Changes

1. **Adapter-protocol spec (ADDED TIA-ADAPT-023 + TIA-ADAPT-024):** TIA-ADAPT-023
   defines the .NET adapter as an external binary invoked via a
   `testaruda.toml` extension-mapping command string `"titi testaruda-adapter"`
   (analogous to the Julia adapter's `"julia --project=. -e '...'"` form in
   TIA-ADAPT-014). TIA-ADAPT-024 is a separate, cross-cutting requirement
   defining the **command-string shell-split** contract that all adapters
   depend on (split into `(binary, args)` before `AdapterIO::spawn`); it is
   separated from TIA-ADAPT-023 so the .NET-adapter definition is not entangled
   with core parsing logic, and so it explicitly unblocks the Julia adapter
   (TIA-ADAPT-014) as well. The .NET adapter handshake declares
   `name="titi"`, `languages=["csharp"]`, `granularity="project"`,
   `symbol_model_complete=false`, and `runtime_edges=false` (the
   `runtime_edges` value is pinned in the titi adapter spec CLI-19 and
   referenced here). The .NET adapter is NOT a workspace crate and MUST be
   installed separately (see `github.com/sashakile/titi`), exactly as the Julia
   adapter requires Testimonial.jl to be installed separately.

2. **Core change — shell-split adapter command strings (TIA-ADAPT-024).** The
   current core passes the configured adapter string verbatim as the `binary`
   argument to `AdapterIO::spawn(binary, &[], None)` at **five** call sites in
   `src/main.rs` (lines 448, 579, 803, 858, 904 — the discover/full-walk and
   edge-case paths included), with an empty args slice. This works for
   bare-binary adapters (`testaruda-adapter-rust`, `-python`, `-typescript`,
   `-clojure`) but NOT for command-string adapters like Julia's
   `"julia --project=. -e '...'"` or titi's `"titi testaruda-adapter"`. This
   change adds a shell-split step (via the `shell-words` crate — POSIX-ish
   quoting, sufficient for both known command-string adapters, no surprising
   shell expansions in a config file) that parses the configured string into
   `(binary, args)` before calling `AdapterIO::spawn`. To avoid partial
   coverage, all five spawn sites SHALL be routed through a single
   `spawn_adapter(command_string, timeout)` helper so the split is applied by
   construction. This unblocks BOTH the Julia adapter (TIA-ADAPT-014, which
   assumes this works) and the .NET adapter introduced here.

3. **Extension mappings include .NET source files.** The `testaruda.toml`
   mappings for .NET SHALL include `.cs`, `.fs`, and `.vb` (the .NET source
   extensions — the units of change titi's `AffectedSet` consumes, see titi
   DG-04) in addition to `.csproj`, `.sln`, `.slnx`. Routing source extensions
   is mandatory: testaruda routes changed files to adapters strictly by
   extension (`registry.resolve` in `src/config.rs`), and titi receives the
   changed-file set from testaruda (titi CLI-19). Without `.cs`/`.fs`/`.vb`
   mapped, typical .NET source changes would never reach titi and the adapter
   would compute an empty affected set. Polyglot repos map only .NET extensions
   to titi; files routed to titi that are not in titi's `MonorepoGraph` are
   returned as `unresolved` and handled by testaruda's over-approximation
   (TIA-SAFE-004).

4. **Opt-in, not auto-detected — following the Julia precedent.** The Julia
   adapter is explicitly opt-in via manual `testaruda.toml` configuration rather
   than auto-detected, because the adapter (Testimonial.jl) is a separate
   install — "requiring it as a hard dependency for auto-detection would be
   misleading" (see `add-julia-adapter-via-testimonial` proposal, Impact). titi
   is likewise a separate install, so this change applies the same reasoning:
   .NET is NOT added to the TIA-CHG-009 init-time probe. `testaruda init` in a
   .NET repo continues to fall back to `testaruda-adapter-rust` (or whatever
   other marker wins); the user adds the titi mapping manually.

## Impact

- **Affected specs:** `adapter-protocol` (two new requirements: TIA-ADAPT-023
  .NET adapter definition, TIA-ADAPT-024 command-string shell-split contract).
  **No change to `change-detection` (TIA-CHG-009)** — .NET is opt-in, following
  the Julia precedent.
- **Affected code (testaruda core):** `src/main.rs` (shell-split the configured
  adapter string into `(binary, args)` at all five spawn sites, routed through a
  single `spawn_adapter` helper); `src/config.rs` (no schema change — the
  extension-mapping value remains a single string, as the Julia adapter already
  assumes). New core dependency: the `shell-words` crate.
- **Affected code (external, titi repo):** titi change `add-testaruda-adapter`
  (CLI-19) implements the adapter side, including the `runtime_edges: false`
  handshake declaration this change references. This change depends on that
  adapter existing and being installed; it does not modify titi.
- **Unblocks:** the Julia adapter (`add-julia-adapter-via-testimonial`), whose
  config form `"julia --project=. -e '...'"` requires the same shell-split
  step. That change assumes the split works; TIA-ADAPT-024 implements it.
- **No change to core engine:** The adapter protocol is unchanged; the core
  already handles all required commands generically via `AdapterIO`.
- **New external dependency at runtime:** the `titi` binary MUST be on `PATH`
  (or configured explicitly). When `titi` is not found, the core SHALL fall back
  per TIA-ADAPT-012 (adapter binary not found → full-suite fallback), exactly as
  the Julia adapter falls back when `julia` or `Testimonial` is missing.

## Relationship to the Julia adapter (TIA-ADAPT-014)

This change follows the pattern the Julia adapter established for
external/hosted adapters:

| Aspect | Julia (TIA-ADAPT-014) | .NET (this change, TIA-ADAPT-023) |
|---|---|---|
| Hosted in | Testimonial.jl (external repo) | titi (external repo, `github.com/sashakile/titi`) |
| Invoked as | `julia --project=. -e '...'` | `titi testaruda-adapter` |
| Config form | command string in `testaruda.toml` | command string in `testaruda.toml` |
| Auto-detected? | No — opt-in (separate install) | No — opt-in (separate install) |
| `granularity` | `"file"` | `"project"` (Phase 1) |
| `symbol_model_complete` | `false` | `false` |
| `runtime_edges` | `true` (coverage-based) | `false` (Phase 1; titi uses MSBuild graph — runtime edges deferred to Phase 2 per titi `add-test-item-detection` DM-08) |

The .NET adapter is the **second** external subcommand-style adapter, not the
first. The core shell-split change (TIA-ADAPT-024) is shared infrastructure that
both external adapters require. Both are opt-in because the adapter is a
separate install the user must provide; neither is bundled with testaruda.

## Dependencies

- **titi adapter:** requires titi change `add-testaruda-adapter` (CLI-19) to be
  implemented and the `titi` binary installed. Tracked in the titi repo as
  `bd` issue `titi-co9` (P0 blocker for the titi adapter epic `titi-dik`).
- **testaruda core:** `AdapterIO::spawn(binary, args, timeout)` already accepts
  a binary path and an args slice (src/adapter.rs, v0.2.3) — no change to
  `AdapterIO` itself. The core change is only in `src/main.rs` (shell-splitting
  the configured string before calling spawn).
- **Shared with Julia:** the shell-split step (TIA-ADAPT-024) is also required
  by `add-julia-adapter-via-testimonial`. If the Julia change lands first and
  implements the split, this change's task 1.1 is already done.

## Success criteria

This change is complete when:

1. `testaruda.toml` mapping `.cs`/`.fs`/`.vb`/`.csproj`/`.sln`/`.slnx` to
   `titi testaruda-adapter` causes the core to spawn `titi` with arg
   `testaruda-adapter` (verified: the configured string is shell-split per
   TIA-ADAPT-024, not passed verbatim as a binary path) at every spawn site.
2. With `titi` installed and the titi adapter implemented, `testaruda select`
   against a .NET monorepo with a changed `.cs` file returns a non-empty
   selection with edges of origin `static`, sourced from titi's
   `MonorepoGraph`.
3. With `titi` NOT on `PATH`, `testaruda select` falls back to full-suite
   selection per TIA-ADAPT-012 and records the missing-binary failure.
4. The shell-split step also makes the Julia adapter's
   `"julia --project=. -e '...'"` config form work (verified with a Julia
   adapter smoke test or unit test).
5. `testaruda init` in a .NET repo does NOT set titi as a default (opt-in
   preserved), per the Julia precedent.
