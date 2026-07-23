# Adapter Protocol

Language adapters are long-lived subprocesses that communicate with testaruda
using one JSON object per line over stdin/stdout. Adapters must reserve stdout
for protocol responses and write diagnostics to stderr. The core waits up to 30
seconds for each response unless the caller supplies another timeout.

## Handshake

```text
testaruda → adapter: {"command": "handshake"}
adapter → testaruda: {"ok":true,"result":{"name":"python-adapter","version":"0.1.0","protocol":1,"languages":["python"],"granularity":"file","capabilities":{"symbol_model_complete":false,"fingerprinting":true,"runtime_edges":false}}}
```

The core rejects adapters whose `protocol` is not `1`. Capability fields are:

| Capability | Meaning | Default |
|---|---|---|
| `symbol_model_complete` | Symbol edges completely model the language dependency graph | `false` |
| `fingerprinting` | Adapter implements content fingerprinting | `true` |
| `runtime_edges` | Adapter can extract runtime dependency edges | `false` |

## Commands

| Command | Request | Success payload |
|---|---|---|
| `handshake` | `{"command":"handshake"}` | `result`: name, version, protocol, languages, granularity, capabilities |
| `discover` | `{"command":"discover"}` | `result`: array of `{node_id, suite_kind, file}` |
| `static-deps` | `{"command":"static-deps","params":{"changed_files":[...]}}` | Top-level `candidates`, `edges`, `unresolved`, and `symbol_edges` arrays |
| `fingerprint` | `{"command":"fingerprint","params":{"files":[...]}}` | Top-level `fingerprints` array of `{file, fingerprint, symbol}` |
| `run-args` | `{"command":"run-args","params":{"selected":[...]}}` | `result`: `{runner_args, collection_path}` |
| `ingest` | `{"command":"ingest","params":{"run_output":"..."}}` | `result`: `{runtime_edges, per_test_results, external_inputs}` |

Dependency edges have this shape:

```json
{"from":"test node ID","to":"content path","weight":1000000,"origin":"static"}
```

`weight` is an integer confidence in parts per million. `origin` is `static`,
`runtime`, or `manual`. Per-test ingestion results contain `test_id`, `outcome`,
and optional `duration_ms` and `error_text` fields.

## Errors and lifecycle

Adapters report command errors as a single line:

```json
{"ok":false,"error":"human-readable message"}
```

Malformed JSON, an explicit error response, timeout, and protocol version
mismatch are fatal to the current adapter connection. Early EOF is observed as
a timeout. A missing `result` is fatal for `handshake`, `run-args`, and `ingest`;
missing arrays from `discover`, `static-deps`, or `fingerprint` are treated as
empty. The core starts with `handshake`, may issue multiple commands, and
terminates the subprocess when the connection is dropped.

See TIA-ADAPT-001 through TIA-ADAPT-013 in the specification.
