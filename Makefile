# Thin wrapper. Everything here is a command you could type; nothing is hidden in the Makefile.
CARGO := cargo
MANIFEST := rust/Cargo.toml
XTASK := $(CARGO) run -q -p xtask --manifest-path $(MANIFEST) --

.PHONY: help preflight test fmt clippy flutter-test catalog gateway reachability boundaries rules docs readme makefile routes openapi owners domains manifests safety preflight-check build

help:
	@echo "make help          this list"
	@echo "make preflight     everything CI runs: fmt, clippy, workspace, on-device tests, gateway, flutter"
	@echo "make test          the Rust workspace only"
	@echo "make fmt           format the workspace"
	@echo "make clippy        lints, warnings as errors"
	@echo "make flutter-test  the app's own tests"
	@echo "make catalog       regenerate catalog/services from the route table"
	@echo "make gateway       audit every published address is answered"
	@echo "make reachability  report modules nothing reaches"
	@echo "make boundaries    check crate dependency rules"
	@echo "make rules         the repository gates"
	@echo "make docs          check the documentation index against the documents"
	@echo "make safety        check the safe-Rust rule and that no Python is left"
	@echo "make preflight-check  check that scripts/preflight.sh runs everything the pipelines run"
	@echo "make readme        check the front page against the catalogue and the decisions"
	@echo "make makefile      check this file against its own targets and help"
	@echo "make routes        regenerate the published address snapshot"
	@echo "make openapi       regenerate the domain contracts"
	@echo "make owners        report which crate answers each unrouted prefix"
	@echo "make domains       check every routed service has a domain page"
	@echo "make manifests     check cluster, compose and image declarations"
	@echo "make build         release build of the engine and the local runtime"

flutter-test:
	cd apps/flutter && flutter pub get && flutter analyze --no-fatal-infos && flutter test

preflight:
	./scripts/preflight.sh

test:
	$(CARGO) test --workspace --manifest-path $(MANIFEST)

fmt:
	$(CARGO) fmt --manifest-path $(MANIFEST) --all

clippy:
	$(CARGO) clippy --workspace --all-targets --manifest-path $(MANIFEST) -- -D warnings

catalog:
	$(XTASK) catalog

gateway:
	$(XTASK) gateway

reachability:
	$(XTASK) reachability

boundaries:
	$(XTASK) boundaries

rules:
	$(CARGO) test -p xtask --manifest-path $(MANIFEST)

docs:
	$(XTASK) docs

readme:
	$(XTASK) readme

safety:
	$(XTASK) safety

preflight-check:
	$(XTASK) preflight

makefile:
	$(XTASK) makefile

routes:
	$(XTASK) routes

openapi:
	$(XTASK) openapi

owners:
	$(XTASK) owners

domains:
	$(XTASK) domains

manifests:
	$(XTASK) manifests

build:
	$(CARGO) build --release --manifest-path $(MANIFEST) -p vimbai_api -p vimbai_local
