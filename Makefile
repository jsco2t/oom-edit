# oom-edit — build system of record.
#
# Run `make help` to list every target.  All cargo invocations use
# --offline --locked except vendor and toolchain.

SHELL := /bin/bash
DENY_FLAGS := check -D warnings
AUDIT_FLAGS := -D warnings
DATA_LICENSE_ROOT ?= $(CURDIR)

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

.PHONY: build-release
build-release: ## Build the release binary
	cargo build --release --package oom-edit --bin oom-edit --offline --locked

.PHONY: build-examples
build-examples: ## Build all examples with locked offline dependencies
	cargo build --workspace --examples --offline --locked

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------
.PHONY: test
test: ## Run the full test suite
	bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked

.PHONY: test-update-snapshots
test-update-snapshots: ## Re-run tests with OOM_UPDATE_SNAPSHOTS=1 to (re)write golden files
	OOM_UPDATE_SNAPSHOTS=1 bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked

.PHONY: test-all
test-all: ## Tests + example builds
	bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked
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
check: ## Run fmt-check + lint + build + test + deny + audit + data-license-check (with summary)
	@PASS=0; FAIL=0; \
	fmt_ok=true; \
	lint_ok=true; \
	build_ok=true; \
	test_ok=true; \
	deny_ok=true; \
	audit_ok=true; \
	data_license_ok=true; \
	echo "=== oom-edit CI gate ==="; \
	echo ""; \
	echo "fmt-check"; \
	if cargo fmt --all -- --check 2>&1; then \
		echo "[PASS] fmt-check"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] fmt-check"; FAIL=$$((FAIL + 1)); fmt_ok=false; \
	fi; \
	echo ""; \
	echo "lint"; \
	if cargo clippy --workspace --all-targets --offline --locked -- -D warnings 2>&1; then \
		echo "[PASS] lint"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] lint"; FAIL=$$((FAIL + 1)); lint_ok=false; \
	fi; \
	echo ""; \
	echo "build"; \
	if cargo build --workspace --offline --locked 2>&1; then \
		echo "[PASS] build"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] build"; FAIL=$$((FAIL + 1)); build_ok=false; \
	fi; \
	echo ""; \
	echo "test"; \
	if bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked 2>&1; then \
		echo "[PASS] test"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] test"; FAIL=$$((FAIL + 1)); test_ok=false; \
	fi; \
	echo ""; \
	echo "deny"; \
	if cargo deny $(DENY_FLAGS) 2>&1; then \
		echo "[PASS] deny"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] deny"; FAIL=$$((FAIL + 1)); deny_ok=false; \
	fi; \
	echo ""; \
	echo "audit"; \
	if cargo audit $(AUDIT_FLAGS) 2>&1; then \
		echo "[PASS] audit"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] audit"; FAIL=$$((FAIL + 1)); audit_ok=false; \
	fi; \
	echo ""; \
	echo "data-license-check"; \
	if bash scripts/check-data-licenses.sh "$(DATA_LICENSE_ROOT)" 2>&1; then \
		echo "[PASS] data-license-check"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] data-license-check"; FAIL=$$((FAIL + 1)); data_license_ok=false; \
	fi; \
	echo ""; \
	echo "=== Summary ==="; \
	echo "  Passed: $$PASS"; \
	echo "  Failed: $$FAIL"; \
	if [ $$FAIL -gt 0 ]; then \
		echo ""; \
		echo "Failed checks:"; \
		[ "$$fmt_ok" = false ]    && echo "  - fmt-check"; \
		[ "$$lint_ok" = false ]   && echo "  - lint"; \
		[ "$$build_ok" = false ]  && echo "  - build"; \
		[ "$$test_ok" = false ]   && echo "  - test"; \
		[ "$$deny_ok" = false ]   && echo "  - deny"; \
		[ "$$audit_ok" = false ]  && echo "  - audit"; \
		[ "$$data_license_ok" = false ] && echo "  - data-license-check"; \
		echo ""; \
		exit 1; \
	fi; \
	echo ""; \
	echo "All checks passed."; \
	exit 0

# ---------------------------------------------------------------------------
# Supply-chain audit
# ---------------------------------------------------------------------------
.PHONY: deny
deny: ## License/ban/advisory checks (CI gate)
	cargo deny $(DENY_FLAGS)

.PHONY: audit
audit: ## RustSec advisory checks (CI gate)
	cargo audit $(AUDIT_FLAGS)

.PHONY: dictionaries
dictionaries: ## Regenerate pinned en_US/en_CA/en_AU dictionaries (requires network)
	bash scripts/fetch-dictionaries.sh

.PHONY: data-license-check
data-license-check: ## Verify bundled-data hashes, headers, notices, and provenance
	bash scripts/check-data-licenses.sh "$(DATA_LICENSE_ROOT)"

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
bench: ## Run performance benchmarks
	cargo bench --workspace --offline --locked

.PHONY: bench-check
bench-check: ## Run perf-smoke asserts with relaxed regression thresholds
	cargo test -p oom-spell --offline --locked --test perf_smoke
	cargo test -p oom-edit-core --offline --locked --test perf_smoke

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------
.PHONY: run
run: ## Run the editor (pass ARGS=...)
	cargo run -p oom-edit --offline --locked -- $(ARGS)

.PHONY: run-isolated
run-isolated: ## Run with temporary XDG config for manual verification (pass ARGS=...)
	bash scripts/with-isolated-config.sh cargo run -p oom-edit --offline --locked -- $(ARGS)

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------
.PHONY: clean
clean: ## Remove build artifacts
	cargo clean
