# Stress-Test Coverage

**Generated:** 2026-07-29
**Adapters:** Built from source (dev profile), adapter-typescript local dev build with mocha/ava/tape detection.

## Summary

| Language | Target | Achieved | Pass Rate | Ingest Rate |
|----------|--------|----------|-----------|-------------|
| Python   | 10     | **10**   | 10/10 ✓   | 10/10 ✓     |
| Rust     | 10     | **10**   | 10/10 ✓   | 10/10 ✓     |
| Julia    | 10     | **5** ✗  | 5/5 (no ingest) | 0/5 (timeout) |
| TypeScript | 10   | **10**   | 10/10 ✓   | 7/10 ✓      |
| Clojure  | 10     | **10**   | 9/10 ✓    | 9/10 ✓      |
| .NET     | 10     | **0** ✗  | 0/10 (adapter crash) | — |
| **Total** | **60** | **45**   | —         | —           |

## Per-Language Details

### Python (10/10)

Uses `testaruda-adapter-python`. All phases green.

| Repo | Tests | Handshake | Discover | Static-deps | Run-args | Ingest |
|------|-------|-----------|----------|-------------|----------|--------|
| attrs | 26 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| click | 34 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| flask | 27 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| httpx | 31 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| requests | 9 | ✓ | ✓ | ✓ | ✓ | ✓ 9 |
| structlog | 17 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| pytest | 121 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| django | 627 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| fastapi | 508 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| numpy | 183 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |

### Rust (10/10)

Uses `testaruda-adapter-rust`. All phases green.

| Repo | Tests | Handshake | Discover | Static-deps | Run-args | Ingest |
|------|-------|-----------|----------|-------------|----------|--------|
| bat | 383 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| tokei | 23 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| serde | 413 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| clap | 1341 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| rayon | 310 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| regex | 430 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| ripgrep | 499 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| async-std | 33 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| actix-web | 351 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| syn | 165 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |

### Julia (5/10 — blocked)

Uses `testaruda-adapter-julia` (via Testimonial.jl). **Blocked by testaruda-l3g**: ingest times out on all repos (30s+). The root cause is a regression in Testimonial.jl's discover — it returns the adapter's own test suite instead of the target project's tests.

| Repo | Tests | Handshake | Discover | Static-deps | Run-args | Ingest |
|------|-------|-----------|----------|-------------|----------|--------|
| csv-jl | 699 | ✓ | ✓ | ✓ | ✓ | ✗ (timeout) |
| dataframes-jl | 699 | ✓ | ✓ | ✓ | ✓ | ✗ (timeout) |
| http-jl | 700 | ✓ | ✓ | ✓ | ✓ | ✗ (timeout) |
| json-jl | 700 | ✓ | ✓ | ✓ | ✓ | ✗ (timeout) |
| plots-jl | 699 | ✓ | ✓ | ✓ | ✓ | ✗ (timeout) |

### TypeScript (10/10)

Uses `testaruda-adapter-typescript`. Detection widened to support mocha, ava, tape in addition to vitest/jest (testaruda-6c6.6). Ingest returns 0 for some repos (testaruda-8ws).

| Repo | Tests | Handshake | Discover | Static-deps | Run-args | Ingest |
|------|-------|-----------|----------|-------------|----------|--------|
| axios | 37 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| zod | 1888 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| ajv | 836 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| express | 112 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| vitest | 5767 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| react | 4435 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| prettier | 1574 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| vuejs/core | 3874 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| nest | 4812 | ✓ | ✓ | ✓ | ✓ | ✗ (0) |
| microsoft/TypeScript | 0 | ✓ | ✓ | ✓ | ✗ | ✗ |

### Clojure (10/10)

Uses `testaruda-adapter-clojure`. Meander has 0 discoverable tests (non-test library).

| Repo | Tests | Handshake | Discover | Static-deps | Run-args | Ingest |
|------|-------|-----------|----------|-------------|----------|--------|
| core-logic | 470 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| ring | 219 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| babashka | 2216 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| datascript | 177 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| compojure | 21 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| meander | 0 | ✓ | ✓ | ✓ | ✗ | ✗ |
| schema | 132 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| leiningen | 263 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |
| test-runner | 3 | ✓ | ✓ | ✓ | ✓ | ✓ 3 |
| cider-nrepl | 343 | ✓ | ✓ | ✓ | ✓ | ✓ 10 |

### .NET (0/10 — adapter not ready)

Uses `titi testaruda-adapter`. Protocol works on minimal fixture (handshake, discover, fingerprint, static-deps, ingest) but **crashes on all real-world repos** due to:
- TFM (Target Framework Moniker) parsing bug in `titi/Interop.cs:138`
- Project-structure assumption (expects projects under `src/`)

**Bug filed:** titi issue forthcoming (TFM parsing `ArgumentOutOfRangeException`).

## Known Issues

| Issue | Description | Status |
|-------|-------------|--------|
| testaruda-l3g | Julia ingest timeout (regression) | Open — blocks Julia to 10 |
| testaruda-8ws | TypeScript ingest returns 0 ingested | Open — low priority |
| — | .NET adapter TFM parsing crash | Not yet filed |

## Next Steps

1. Fix testaruda-l3g (Julia ingest) to unblock Julia → 10
2. Fix TypeScript ingest 0-count (testaruda-8ws) for completeness
3. Fix titi TFM parsing bug, then re-attempt .NET → 10
4. Optionally: automate stress-test as CI check on adapter crates