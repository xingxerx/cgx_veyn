##
## VEYN — project tasks
## Usage: make <target>
##

.PHONY: build release run-dev test lint fmt clean package help

CARGO     ?= cargo
BINARY    ?= target/release/veyn-core
TOKEN_FILE = $(HOME)/.local/share/veyn/token

## build: compile debug binary
build:
	$(CARGO) build -p veyn-core

## release: compile optimised release binary
release:
	$(CARGO) build -p veyn-core --release

## run-dev: start daemon with mock adapter and auth disabled
run-dev:
	VEYN_MOCK=true $(CARGO) run -p veyn-core -- --no-auth

## run: start daemon with a release binary (requires `make release` first)
run:
	VEYN_MOCK=true $(BINARY) --config veyn.toml

## test: run all workspace tests
test:
	$(CARGO) test --workspace

## lint: run clippy on all workspace crates
lint:
	$(CARGO) clippy --workspace -- -D warnings

## fmt: format all Rust source files
fmt:
	$(CARGO) fmt --all

## check: dry-run fmt + clippy (useful in CI)
check:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace -- -D warnings

## clean: remove build artefacts
clean:
	$(CARGO) clean

## package: build release binary and create a tarball
package: release
	mkdir -p dist
	cp $(BINARY) dist/veyn-core
	cp veyn.toml.example dist/veyn.toml.example
	cp rules.toml dist/rules.toml
	cp INSTALL.md dist/INSTALL.md
	tar -czf dist/veyn-$(shell $(CARGO) metadata --no-deps --format-version 1 | \
		python3 -c "import sys,json; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='veyn-core'))").tar.gz \
		-C dist veyn-core veyn.toml.example rules.toml INSTALL.md
	@echo "Package created in dist/"

## health: quick API smoke-test (daemon must be running)
health:
	@TOKEN=$$(cat $(TOKEN_FILE) 2>/dev/null); \
	if [ -z "$$TOKEN" ]; then \
		curl -s http://localhost:7700/v1/health | python3 -m json.tool; \
	else \
		curl -s -H "Authorization: Bearer $$TOKEN" http://localhost:7700/v1/health | python3 -m json.tool; \
	fi

## context: fetch current AI context snapshot
context:
	@TOKEN=$$(cat $(TOKEN_FILE) 2>/dev/null); \
	curl -s -H "Authorization: Bearer $$TOKEN" http://localhost:7700/v1/context/current | python3 -m json.tool

## help: show this message
help:
	@grep -E '^## ' Makefile | sed 's/^## //'
