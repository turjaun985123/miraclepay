# Performance baseline

MiraclePay v0.0.1 — load/perf pass (Wave 28), measured 2026-11-26.

## Tool

`scripts/load-test.mjs` — zero-dependency Node load generator
(concurrency + duration, status counts, p50/p95/p99). It refuses
non-localhost targets unless `--allow-remote` is given.

```bash
node scripts/load-test.mjs --url http://localhost:8080 --path /health \
  --concurrency 20 --seconds 20
```

## Environment (read the caveats before quoting numbers)

- **Debug build** (`cargo build`, unoptimised) of `miraclepay-api`,
  in-memory mode, on a **shared, throttled VM** (1.5 shared vCPU).
  The load generator ran on the same VM, competing for the same CPU.
- Run-to-run throughput varied by >3× with neighbour load
  (`/health`: 3,693 req/s in one run, 1,018 req/s in another).
  **These figures are regression floors for this environment, not
  production capacity claims.** Re-measure with a release build on
  the target host before quoting capacity.

## Measured

| Scenario | Throughput | p50 | p95 | Errors |
|---|---|---|---|---|
| `GET /health`, c=20, 15 s | 3,693 req/s | 2.2 ms | 34.3 ms | 0 / 55,513 |
| `GET /openapi.json` (31.6 KB), c=20, 10 s, before cache | 156 req/s | 112.8 ms | 216.9 ms | 0 / 1,591 |
| `GET /openapi.json`, c=20, 10 s, after cache | 164 req/s | 106.4 ms | 208.6 ms | 0 / 1,650 |
| Single request, idle | — | /health ≈ 1.5–4 ms, /openapi.json ≈ 5–8 ms | — | — |

## Finding & fix

The spec document was **rebuilt from the route table on every
request**. It is now built once and cached (`openapi::spec_cached`,
Wave 28) — strictly less CPU and allocation per request; the
concurrent numbers above are too environment-noisy to show the
difference, but single-request cost is bounded by serialising a
31.6 KB document, and construction cost no longer scales with load.

## Notes for production measurement

- Build `--release`; run the generator on a *different* host.
- Authenticated list endpoints are the realistic hot path; measure
  those with a seeded dataset (Wave 46 adds a seed/demo mode).
- Rate limits (Wave 12) cap `/api/*` at 600 req/min per client IP —
  by design, load tests against `/api/*` from one IP will see 429s;
  that is the limiter working, not an error budget breach.
