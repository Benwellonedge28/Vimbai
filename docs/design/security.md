# Security and financial integrity
Status: proposed extension of ../compliance/financial-integrity.md, ADR-002 and access-control.md.

## Threat model and invariants
Protect against cross-tenant access, stolen credentials, malicious clients, replay, compromised integrations, insider tampering, ransomware and supply-chain compromise. Trust authenticated server-derived identity, not client scope headers. Ledger-core remains pure and owns balance/hash/reversal algorithms; domain and persistence layers enforce posted status, uniqueness and atomicity. Hash chains detect tampering but do not prevent a privileged actor rebuilding the entire chain; consider independently protected signed checkpoints.

## Controls and ownership
Identity owns MFA, session rotation and revocation; tenancy owns scoped authorization; gateway owns ingress limits and token verification; API server repeats resource authorization; database adapter owns transaction isolation and scoped persistence; platform audit owns append-only security evidence. Use encryption in transit and at rest, managed secrets, key rotation, least-privilege service identities and redacted logs. Separate billing and customer data. Offline keys require recovery and revocation design. No secrets in repositories or telemetry.

## Implementation sequence
Inventory existing identity, tenancy, gateway, audit and store modules; define threat/control matrix and security contracts; extend existing authorities; add independent security tests and operational response procedures. Do not create parallel authentication or ledger implementations. Existing financial-integrity suite is the baseline, not a substitute for penetration testing.

## Acceptance
Prove cross-tenant denial on every entry point, spoofed header refusal, expired/revoked sessions, replay resistance, exact balance, immutable postings, reversal uniqueness, chain tamper detection, backup restore, secret rotation and incident containment. Run dependency and unsafe-code gates. Document residual risks, owners and remediation before release.
