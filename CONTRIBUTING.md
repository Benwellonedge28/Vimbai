# Contributing to Vimbai

## Getting Started

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature-name`
3. Make your changes following the code style below
4. Run tests: `pytest` for Python, `go test` for Go, `yarn test` for frontend
5. Submit a pull request to the `develop` branch

## Code Style

### Python Services
- Use `black` for formatting (line length 120)
- Use `isort` for import ordering
- Use `flake8` for linting
- All new endpoints must have Pydantic models for request/response
- Use `structlog` for structured logging

### Go (API Gateway)
- Use `gofmt` for formatting
- Follow standard Go project layout

### Frontend
- Use TypeScript strict mode
- Use `eslint` for linting
- Follow React best practices (hooks, functional components)

## Pull Request Checklist

- [ ] Code passes all linters (black, flake8, isort, eslint)
- [ ] Tests written and passing
- [ ] No hardcoded secrets
- [ ] Dockerfile uses non-root user
- [ ] DESIGN.md updated if architecture changed
- [ ] CHANGELOG.md updated

## Branch Strategy

- `main` - Production releases (tagged)
- `develop` - Active development
- `feature/*` - Feature branches
- `fix/*` - Bug fix branches
