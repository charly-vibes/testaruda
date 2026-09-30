# Status

**beta** — core works and is dogfooded in anger; surface may shift before stable.

## Implemented

| Command | Status | Notes |
|---|---|---|
| `testaruda init` | stable | project/adapter setup |
| `testaruda select` | stable | language-agnostic test selection engine |
| `testaruda doctor` | stable | environment/adapter diagnostics |
| `testaruda explain` | stable | why a test was/wasn't selected |
| `testaruda graph` | beta | dependency graph inspection |
| `testaruda validate` | beta | config/adapter validation |

## Adapters

| Adapter | Status | Notes |
|---|---|---|
| rust / python / julia / clojure / typescript | stable | per-language test discovery |

## In progress

- Adapter coverage round; dogfood-matrix installs (DDL-1a0).

## Mapped to specs

- `openspec/` proposals in-repo; validated strict in CI.

## Dogfooding

- Runs pretender (pinned), dont, espectacular, plus planned wai/openspec/ddl installs per `versions.ddl.toml`
- Its own CI runs `just ci`; docs book built from root `book.toml`
