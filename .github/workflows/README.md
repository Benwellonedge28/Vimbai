# Vimbai CI/CD Pipelines

This directory contains the GitHub Actions workflows that form the continuous integration and continuous deployment (CI/CD) backbone of the Vimbai platform.

## Architecture

The pipelines are designed to handle a large-scale monorepo containing over 300 microservices.

### 1. Continuous Integration (`ci-main.yml`)
- **Change Detection:** Automatically detects which specific Python or Go services have been modified in a PR or push.
- **Matrix Testing:** Spawns parallel test jobs *only* for the services that changed, rather than running all 300+ test suites.
- **Linting & Security:** Runs `ruff` (linting) and `bandit` (security scanning) for Python, and `gosec` for Go.
- **Coverage:** Uploads coverage reports to Codecov.

### 2. Docker Publishing (`docker-publish.yml`)
- Builds and pushes Docker images to the GitHub Container Registry (`ghcr.io`).
- Runs on merges to `main`.
- Can be triggered manually via `workflow_dispatch` to rebuild specific services.

### 3. Continuous Deployment (`cd-staging.yml` & `cd-production.yml`)
- **Staging:** Deploys automatically to the staging Kubernetes cluster when code is merged into `develop`.
- **Production:** Deploys to the production Kubernetes cluster only when a semantic version tag (e.g., `v1.2.0`) is pushed.
- Uses Helm for deployment orchestration.

### 4. Infrastructure Validation (`iac-validate.yml`)
- Lints and validates Terraform configurations and Helm charts whenever changes are made to the `infrastructure/` or `charts/` directories.

### 5. Rollback (`cd-rollback.yml`)
- A manual `workflow_dispatch` pipeline that allows administrators to quickly rollback the production environment to a previous Helm revision in case of a critical failure.

## Secrets Required
To operate fully, the following repository secrets must be configured:
- `KUBE_CONFIG_STAGING`: Kubeconfig file for the staging cluster.
- `KUBE_CONFIG_PRODUCTION`: Kubeconfig file for the production cluster.
