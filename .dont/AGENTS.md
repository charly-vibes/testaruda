# dont managed instructions

> This file is managed by `dont init` and `dont doctor --fix`. Do not edit manually.

# dont

This project uses `dont` for epistemic claim tracking.

For full documentation see the [dont spec](https://github.com/charly-vibes/dont).

## Quick start

```
dont ground "claim text" --file README.md --lines 10-18 # fast path: claim + repo evidence
dont conclude "claim text"                            # introduce an unverified claim
dont trust <id> --reason ...                          # register doubt
dont dismiss <id> --evidence ...                      # verify with evidence
dont show <id>                                        # inspect a claim
dont trace <id>                                       # diagnose blocker paths
dont list                                             # list all claims
```

`dont ground` is the preferred fast path when you already have the claim and evidence in hand. The underlying model is still conclude → trust → dismiss → forget: `ground` composes `conclude` and `dismiss` rather than bypassing the core lifecycle.

## Grounding claims in repository evidence

Prefer repository-relative file locators over opaque `file://` URIs when the evidence lives inside the current project:

```
# Preferred: repository-relative locator
dont ground "documented project fact" --file README.md --lines 10-18
dont dismiss <id> --file src/lib.rs --lines 42-55 --anchor "MyTrait"
dont dismiss <id> --file docs/spec.md --excerpt "The system SHALL..."

# Supported for compatibility: plain URI
dont dismiss <id> --evidence https://external-source.example/ref
```

Repository-relative locators resolve against the project root regardless of the caller's working directory. Paths that escape the project root (via `..` traversal or symlink escape) are refused.

When `show` or `why` reports stale, unresolved, or otherwise confusing blockers, run `dont trace <id>` to see the blocker path that explains what dependency or support fallout needs attention.

## Modes

`dont` operates in two modes:

- **permissive** — allows claims to progress with weaker evidence; suitable for exploratory work.
- **strict** — enforces full evidence and lifecycle requirements before dismissal.

Run `dont prime --json` to see the current mode. To change mode, edit `mode = "permissive"` or `mode = "strict"` in `.dont/config.toml`.

## Error recovery

When a command returns `"ok": false`, read `data.remediation[0].command` from the JSON envelope and run it exactly as printed. Do not guess reformulations — the remediation field is authoritative.

## Defining terms

Before running `dont define`, run `dont suggest-term "<description>"` to check for an existing term that fits. When defining, always supply both `--label "<a noun phrase>"` and `--doc "<definition text>"` together.

## Spawn

When `dont` requests a sub-agent via a spawn envelope, the harness is responsible for fulfilling the spawn. Run the spawn command from the envelope's `data.command` field and feed its output back to the calling agent.

## Help

Run `dont help --tutorial` for the first-session walkthrough.
Run `dont help --topics` to list all how-to guides.
Run `dont help --howto <topic>` to read a specific guide.

