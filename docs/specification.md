# Specification

The full Software Requirements Specification (SRS) uses the EARS (Easy Approach
to Requirements Syntax) notation and is documented in `docs/tia-srs-ears.md`.

## Key Requirement Groups

| Group | Area |
|-------|------|
| ARCH | Architectural constraints (semiring K-relation, transpose selection) |
| CORE | Core data model (content units, test items, edges) |
| CHG | Change detection (diff computation, fingerprinting) |
| SEL | Selection engine (transpose closure, semiring evaluation) |
| PROV | Provenance and explainability |
| RUN | Runtime feedback ingestion |
| SAFE | Soundness and safety (over-approximation invariant) |
| CONF | Confidence scoring |
| ADAPT | Language adapter interface |
| COMP | Composability (multi-component, multi-repo) |
| STORE | Persistence and store (SQLite + CAS) |
| CI | CI consumer interface |
| LOCAL | Local developer interface |
| AGENT | LLM-agent consumer interface |
| OBS | Observability |
| PERF | Performance |
| REL | Reliability |
| SEC | Security |
| PORT | Portability and maintainability |
| SCALE | Scalability |
| VER | Verification and rollout |
| ENG | Engine design constraints (Ascent reference) |

## Design Constraints

The reference implementation uses **Ascent** as the embedded logic engine,
with Soufflé as a validation oracle. See TIA-ENG-001 through TIA-ENG-016
for full details.