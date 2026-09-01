# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Vimbai, please report it responsibly:

1. **Do not** open a public GitHub issue
2. Email security concerns to the repository owner
3. Include a detailed description and reproduction steps
4. You will receive a response within 48 hours

## Security Measures

- JWT-based authentication with environment-variable secrets (no hardcoded defaults)
- RBAC with capability-based security
- MFA support (TOTP, SMS, Email)
- Non-root container execution
- Neo4j and RabbitMQ credentials via environment variables
- Regular security scanning: CodeQL, OWASP ZAP, Trivy, Bandit, TruffleHog

## Secret Management

All secrets must be provided via environment variables or a secrets manager (e.g., Kubernetes Secrets, HashiCorp Vault). No secrets are hardcoded in the codebase.
