# Thin wrapper. Everything here is a command you could type; nothing is hidden in the Makefile.
CARGO := cargo
MANIFEST := rust/Cargo.toml
XTASK := $(CARGO) run -q -p xtask --manifest-path $(MANIFEST) --

.PHONY: help preflight test fmt clippy catalog gateway reachability boundaries rules build

help:
	@echo "make preflight     everything CI runs: fmt, clippy, workspace, on-device tests, gateway, flutter"
	@echo "make test          the Rust workspace only"
	@echo "make fmt           format the workspace"
	@echo "make clippy        lints, warnings as errors"
	@echo "make catalog       regenerate catalog/services from the route table"
	@echo "make gateway       audit every published address is answered"
	@echo "make reachability  report modules nothing reaches"
	@echo "make boundaries    check crate dependency rules"
	@echo "make rules         the repository gates"
	@echo "make build         release build of the engine and the local runtime"

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

build:
	$(CARGO) build --release --manifest-path $(MANIFEST) -p vimbai_api -p vimbai_local
