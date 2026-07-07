# Adapter Protocol

Language adapters communicate with testaruda via JSON over stdin/stdout.

## Handshake

```
testaruda → adapter: {"command": "handshake"}
adapter → testaruda: {"name": "pytest-adapter", "version": "0.1", ...}
```

## Commands

| Command | Input | Output |
|---------|-------|--------|
| `discover` | paths, component | test items |
| `static-deps` | changed files | candidate tests, K-edges, unresolved |
| `fingerprint` | files/symbols | content fingerprints |
| `run-args` | selected set | native runner argv |
| `ingest` | run output, coverage | runtime edges, results |

See TIA-ADAPT-001 through TIA-ADAPT-013 in the specification.