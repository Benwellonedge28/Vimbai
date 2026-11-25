# Scalability and performance
Status: proposed extension of architecture overview and operations runbook.

## Workload model
Scale from a single offline Book to multi-tenant deployments with many Books and devices. Preserve per-Book posting order and exact arithmetic. Scale stateless gateway/API replicas and sync replicas independently; durable worker claims prevent duplicate jobs. PostgreSQL remains deployment authority; do not silently fall back to another store.

## Design
Use bounded cursor queries, scoped indexes, streaming exports, connection pools, backpressure, configurable deadlines and per-tenant fairness. Existing runbook settings include VIMBAI_API_MAX_IN_FLIGHT, VIMBAI_API_TIMEOUT_MS, VIMBAI_API_WORKERS and VIMBAI_POSTGRES_POOL. Treat these as operational budgets, not semantic limits. Keep canonical aggregation in Rust per ADR-007; optimize by measured batching and verified projections, never approximate financial totals. Partition only after workload evidence; avoid cross-Book distributed transactions where possible.

## Observability
Measure throughput, latency percentiles, queue age, pool saturation, error rates, sync lag, memory, storage growth and reconciliation time without logging financial payloads. Define SLOs per deployment profile from benchmarks and user requirements. Load shedding returns explicit retryable errors; idempotency protects retries.

## Acceptance
Run existing scale suite and real-provider tests; add mixed read/write, hot Book contention, many-Book fairness, slow provider, replica loss and long-running export cases. Verify no lost postings, exact balances and bounded resource use. Publish hardware/configuration and workload with results; no unmeasured capacity claims.
