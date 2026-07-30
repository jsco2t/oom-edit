# oom-edit — build system of record.
#
# Run `make help` to list every target.  All cargo invocations use
# --offline --locked except vendor and toolchain.

SHELL := /bin/bash

.PHONY: help
help: ## Show this help (default)
	@grep -E '^[a-zA-Z_-]+:.*##' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# ---------------------------------------------------------------------------
# Toolchain
# ---------------------------------------------------------------------------
.PHONY: toolchain
toolchain: ## Install dev toolchain (rustup, cargo-deny, cargo-audit)
	rustup component add rustfmt clippy 2>/dev/null || true
	cargo install cargo-deny --locked --force 2>/dev/null || true
	cargo install cargo-audit --locked --force 2>/dev/null || true

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
.PHONY: build
build: ## Build the workspace
	cargo build --workspace --offline --locked

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------
.PHONY: test
test: ## Run the full test suite
	cargo test --workspace --offline --locked

.PHONY: test-update-snapshots
test-update-snapshots: ## Re-run tests with OOM_UPDATE_SNAPSHOTS=1 to (re)write golden files
	OOM_UPDATE_SNAPSHOTS=1 cargo test --workspace --offline --locked

.PHONY: test-all
test-all: ## Tests + example builds
	cargo test --workspace --offline --locked
	cargo build --examples --offline --locked

# ---------------------------------------------------------------------------
# Format
# ---------------------------------------------------------------------------
.PHONY: fmt
fmt: ## Auto-format the workspace
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Format check (CI gate)
	cargo fmt --all -- --check

# ---------------------------------------------------------------------------
# Lint
# ---------------------------------------------------------------------------
.PHONY: lint
lint: ## Clippy with -D warnings (CI gate)
	cargo clippy --workspace --all-targets --offline --locked -- -D warnings

.PHONY: lint-fix
lint-fix: ## Apply safe clippy suggestions
	cargo clippy --workspace --all-targets --offline --locked --fix --allow-dirty

# ---------------------------------------------------------------------------
# Check — the local CI gate
# ---------------------------------------------------------------------------
.PHONY: check
check: fmt-check lint build test ## Run fmt-check + lint + build + test

# ---------------------------------------------------------------------------
# Supply-chain audit
# ---------------------------------------------------------------------------
.PHONY: deny
deny: ## License/ban/advisory checks (CI gate)
	cargo deny check

.PHONY: audit
audit: ## RustSec advisory checks (CI gate)
	cargo audit

# ---------------------------------------------------------------------------
# Documentation
# ---------------------------------------------------------------------------
.PHONY: doc
doc: ## Generate documentation (no deps)
	cargo doc --workspace --no-deps --offline --locked

# ---------------------------------------------------------------------------
# Vendor
# ---------------------------------------------------------------------------
.PHONY: vendor
vendor: ## Re-vendor dependencies (requires network)
	cargo vendor

# ---------------------------------------------------------------------------
# Bench
# ---------------------------------------------------------------------------
.PHONY: bench
bench: ## Run Criterion benchmarks
	cargo bench --workspace --offline --locked

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------
.PHONY: run
run: ## Run the editor (pass ARGS=...)
	cargo run -p oom-edit --offline --locked -- $(ARGS)

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------
.PHONY: clean
clean: ## Remove build artifacts
	cargo clean
