# Production readiness and quality assurance
Status: proposed release gate; not a certification of current readiness.

## Evidence baseline
Existing ROADMAP.md requires safe Rust, preflight, no duplicate routes, exact money, parity and immutable ledger. Financial-integrity suite documents named invariant tests. Flutter structure documents missing device-to-engine integration tests. Runbook documents recovery and overload behavior. Re-run all evidence against the release commit; historical green results do not certify a new build.

## Gate matrix
| Gate | Evidence |
|---|---|
| Architecture | xtask boundaries, repository and route gates |
| Financial | independent financial-integrity and scale suites |
| Storage | real PostgreSQL/SQLite conformance, migration and restore |
| API | OpenAPI compatibility, idempotency, authorization |
| Client | format, analyze, unit/widget and real-device integration |
| Security | threat review, dependency scan, penetration tests |
| Operations | deployment, rollback, backup and incident drills |
| Commercial | verified merchant agreement, sandbox/live reconciliation |
| Release | signed packages, checksums, SBOM, release notes |

## Execution
Use existing scripts/preflight.sh and CI rather than parallel pipelines. Record commit, environment, test results, skipped tests, defects and accountable reviewer. Block release on unresolved financial integrity, tenant isolation, data loss or critical security failures. Stage pilot, monitor, expand gradually; rollback must respect schema compatibility.

## Acceptance
Every applicable gate passes with reproducible evidence; skipped gates remain explicitly unverified. No production-ready label based on document completion alone.
