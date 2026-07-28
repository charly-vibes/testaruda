# doctor capability: ADDED

## ADDED Requirements

### Requirement: Doctor command

testaruda SHALL provide a `doctor` command (the true-suite-minimum verb it currently lacks — tool-craft §2.1) backed by `genesis::suite_linter`, running health checks and printing a per-check pass/fail verdict, with `--fix` applying safe fixes.

#### Scenario: doctor runs suite linter checks

- **WHEN** `testaruda doctor` is run
- **THEN** it SHALL run `genesis::suite_linter` checks: testaruda.toml schema conformance, pretender.toml presence when pretender is wired, `ah check` in pre-commit when `.espectacular/` exists, `dont check` in pre-push when `.dont/` exists, and managed-block/badge match
- **AND** SHALL exit 0 only when all non-skipped checks pass, 1 otherwise.

#### Scenario: doctor --fix applies safe fixes

- **WHEN** `testaruda doctor --fix` is run and a check carries a fix function
- **THEN** the fix SHALL be applied via `genesis::suite_linter` fix fns
- **AND** checks without a fix SHALL remain as `✗` with a `ContextHint`.

### Requirement: feedback subcommand

testaruda SHALL provide a `feedback` subcommand that files a structured issue against testaruda's upstream repo via `gh`, wrapping `genesis::feedback` for the redactor, context-bundle, error-scratch, and `gh`-invocation machinery.

#### Scenario: agent files a bug with last error

- **WHEN** `testaruda feedback bug --from-last-error --yes` is run after a non-zero exit
- **THEN** testaruda SHALL read its own error scratch
- **AND** SHALL assemble and redact the body via `genesis::feedback`
- **AND** SHALL invoke `gh issue create` against testaruda's `Cargo.toml` `repository` with labels `agent-reported`, `bug`, `has-repro`.

#### Scenario: error with no self-healing fix

- **WHEN** testaruda exits non-zero and no `genesis::suggestions::Fix` is available
- **THEN** the error footer SHALL print `Feedback: testaruda feedback bug --from-last-error`.