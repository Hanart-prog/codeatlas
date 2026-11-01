# rustup installs to ~/.cargo/bin, which may not be on PATH yet. Make 3.81 (macOS)
# ignores an exported PATH when it finds commands, so cargo is resolved explicitly.
export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO := $(shell command -v cargo 2>/dev/null || echo $(HOME)/.cargo/bin/cargo)

WEB := apps/web
WARMUP := 100
PNPM_VERSION := 10.34.9
PNPM := $(shell command -v pnpm 2>/dev/null || echo npx --yes pnpm@$(PNPM_VERSION))

.PHONY: setup check test build e2e verify dev types fixture clean

setup: ## Fetch Rust and frontend dependencies
	@test -x "$(CARGO)" || { echo "cargo not found; install Rust from https://rustup.rs"; exit 1; }
	$(CARGO) fetch --locked
	cd $(WEB) && $(PNPM) install --frozen-lockfile

check: ## Formatting, lints and type checks
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	cd $(WEB) && $(PNPM) lint && $(PNPM) typecheck

test: ## All Rust and frontend tests
	$(CARGO) test --workspace --locked
	cd $(WEB) && $(PNPM) test

build: ## Release binary and production frontend
	$(CARGO) build --release --locked -p atlas-cli
	cd $(WEB) && $(PNPM) build

e2e: ## Browser tests against a generated fixture; set PW_CHANNEL=chrome to use local Chrome
	$(CARGO) build --locked -p atlas-cli -p atlas-fixtures
	cd $(WEB) && $(PNPM) build && $(PNPM) e2e

verify: ## Recheck a real repository independently: make verify REPO=/path/to/clone [WARMUP=100]
	@test -n "$(REPO)" || { echo "usage: make verify REPO=/path/to/clone"; exit 1; }
	$(CARGO) build --locked -p atlas-cli
	python3 scripts/verify_compare.py target/debug/codeatlas "$(REPO)"
	python3 scripts/verify_backtest.py target/debug/codeatlas "$(REPO)" $(WARMUP)
	@data=$$(mktemp -d); graph=$$data/graph.json; \
	target/debug/codeatlas analyze "$(REPO)" --data-dir "$$data" >/dev/null && \
	target/debug/codeatlas graph "$(REPO)" --limit 50000 --format json --data-dir "$$data" > "$$graph" && \
	node scripts/verify_imports.cjs "$(CURDIR)/$(WEB)/node_modules/typescript" "$(REPO)" "$$graph"

dev: ## API on :4317 and Vite on :5173; Ctrl-C stops both
	@trap 'kill 0' EXIT; \
	$(CARGO) run -p atlas-cli -- serve & \
	(cd $(WEB) && $(PNPM) dev) & \
	wait

types: ## Regenerate frontend types from the Rust DTOs
	UPDATE_BINDINGS=1 $(CARGO) test -p atlas-model --test ts_bindings

fixture: ## Generate the fixture repository in DIR, an empty directory
	$(CARGO) run -q -p atlas-fixtures --bin generate-fixture -- $(DIR)

clean: ## Remove build output
	$(CARGO) clean
	rm -rf $(WEB)/dist
