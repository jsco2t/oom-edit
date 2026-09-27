# oom-edit — build system of record.
#
# Run `make help` to list every target. Builds and tests use --offline
# --locked; toolchain, vendor, and supply-chain checks require network access.

SHELL := /bin/bash
DENY_FLAGS := check -D warnings
DENY_COMMAND := bash scripts/cargo-deny.sh
AUDIT_FLAGS := -D warnings
DATA_LICENSE_ROOT ?= $(CURDIR)
DOWNSTREAM_DIR ?=
DOWNSTREAM_REV ?=
DOWNSTREAM_TAG ?=
DOWNSTREAM_SOURCE ?=
DOWNSTREAM_SNAPSHOT ?=
TREE_SITTER ?= tree-sitter
GRAMMAR_OUTPUT ?= patches/tree-sitter-md/tree-sitter-markdown/src
PERF_ROOT ?= $(CURDIR)

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

.PHONY: run-embedded
run-embedded: ## Run the split-pane example (ARGS=paths or --legacy-keys)
	cargo run --package oom-edit --example embedded --offline --locked -- $(ARGS)

.PHONY: sync-notices
sync-notices: ## Refresh the byte-identical crate-local copy of canonical third-party notices
	cp THIRD-PARTY-NOTICES.md crates/oom-edit/assets/THIRD-PARTY-NOTICES.md

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------
.PHONY: test
test: feature-workflow-test tui-perf-test ci-workflow-test drd-coverage-test grammar-generation-test downstream-tool-test ## Run the full test suite
	bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked

.PHONY: downstream-tool-test
downstream-tool-test: ## Test independent consumer provenance and isolation guards
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_downstream.py'

.PHONY: downstream-snapshot
downstream-snapshot: ## Create a temporary immutable source snapshot (not a release commit)
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/downstream.py snapshot --directory "$(DOWNSTREAM_SNAPSHOT)"

.PHONY: downstream-prepare
downstream-prepare: ## Seed an independent consumer at DOWNSTREAM_REV (Git network allowed here only)
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/downstream.py prepare --directory "$(DOWNSTREAM_DIR)" --revision "$(DOWNSTREAM_REV)" --tag "$(DOWNSTREAM_TAG)" --source "$(DOWNSTREAM_SOURCE)" --registry-vendor "$(CURDIR)/vendor"

.PHONY: downstream-candidate-check
downstream-candidate-check: ## Verify all candidate sources and run an independently locked/vendored consumer offline
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/downstream.py check --directory "$(DOWNSTREAM_DIR)" --revision "$(DOWNSTREAM_REV)"

.PHONY: downstream-tag-check
downstream-tag-check: ## Verify a separately prepared exact-tag consumer against its gated SHA (after authorized publication)
	@test -n "$(DOWNSTREAM_TAG)" || { echo 'DOWNSTREAM_TAG must name the published version tag' >&2; exit 1; }
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/downstream.py check --directory "$(DOWNSTREAM_DIR)" --revision "$(DOWNSTREAM_REV)" --tag "$(DOWNSTREAM_TAG)"

.PHONY: downstream-negative-check
downstream-negative-check: ## Reject bad resolved sources and corrupted vendored bytes in an isolated copy
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/downstream.py negative-check --directory "$(DOWNSTREAM_DIR)" --revision "$(DOWNSTREAM_REV)"

.PHONY: test-package-guards
test-package-guards: ## Verify integrated facade documentation, consumer snippet and CI ownership
	cargo test --package oom-edit --test package_guards --offline --locked

.PHONY: test-config-public
test-config-public: ## Verify public config roundtrip, exact fallbacks, presence and safe persistence
	cargo test --package oom-edit --test config_public --offline --locked

.PHONY: test-release-versions
test-release-versions: ## Verify candidate versions, exact workspace pins and dependency boundaries
	cargo test --package oom-edit --test dependency_hygiene --offline --locked
	cargo test --package oom-edit-core --test dependency_hygiene --offline --locked

.PHONY: test-public-api
test-public-api: ## Compile the curated editor APIs and all privacy/boundary compile-fail cases
	cargo test --package oom-edit --test public_api --offline --locked
	cargo test --package oom-edit-core --test public_api --offline --locked
	cargo test --workspace --doc --offline --locked

.PHONY: test-embedding-example
test-embedding-example: ## Drive the public-API split host and shared key vectors headlessly
	cargo test --package oom-edit --test embedding_example --offline --locked
	cargo test -p oom-edit --offline --locked --lib terminal_key_vectors_cover_legacy_and_enhanced_equivalence

.PHONY: test-pane-public
test-pane-public: ## Run external-consumer pane API and behavior tests
	cargo test --package oom-edit --test pane_public --offline --locked

.PHONY: test-pane-lifecycle
test-pane-lifecycle: ## Run public pane lifecycle and file-policy tests
	cargo test --package oom-edit --test pane_lifecycle --offline --locked

.PHONY: test-pane-disk
test-pane-disk: ## Run public disk reconciliation and injected poll/safe-point tests
	cargo test -p oom-edit --offline --locked --test pane_disk
	cargo test -p oom-edit --offline --locked --lib app::disk_watch::tests

.PHONY: test-pane-bindings
test-pane-bindings: ## Verify host hints, status, registry and real input ownership
	cargo test -p oom-edit --offline --locked --test bindings_public
	cargo test -p oom-edit --offline --locked --lib command::
	cargo test -p oom-edit --offline --locked --lib overlay::palette::tests
	cargo test -p oom-edit --offline --locked --lib widgets::which_key::tests
	cargo test -p oom-edit --offline --locked --lib app::metadata_tests
	cargo test -p oom-edit --offline --locked --lib snapshot_tests

.PHONY: test-standalone-host
test-standalone-host: ## Check standalone public-pane routing and host protocol parity
	cargo test -p oom-edit --offline --locked --test dependency_hygiene standalone_startup_and_loop_use_only_the_public_pane -- --exact
	cargo test -p oom-edit --offline --locked --lib event::tests
	cargo test -p oom-edit --offline --locked --lib standalone::tests
	cargo test -p oom-edit --offline --locked --test standalone_parity

.PHONY: test-pane-theme-config
test-pane-theme-config: ## Verify public live theme and runtime configuration updates
	cargo test -p oom-edit --offline --locked --test pane_public live_theme

.PHONY: feature-workflow-test
feature-workflow-test: ## Test feature-workflow helper scripts
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s .agents/skills/feature-workflow/tests -p 'test_*.py'

.PHONY: tui-perf-test
tui-perf-test: ## Test TUI performance evidence tooling
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_tui_performance.py'

.PHONY: ci-workflow-test
ci-workflow-test: ## Test the GitHub Actions workflow contract
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_ci_workflow.py'

.PHONY: drd-coverage-test
drd-coverage-test: ## Validate the embeddable-pane DRD coverage inventory
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_drd_coverage.py'

.PHONY: coverage-test-list
coverage-test-list: ## Compile and list the actual Rust cases for strict requirement coverage
	cargo test --workspace --offline --locked -- --list

.PHONY: coverage-check
coverage-check: ## Reject planned, missing, unknown, ignored or nonexistent requirement cases
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/drd_coverage.py --final

.PHONY: test-update-snapshots
test-update-snapshots: ## Re-run tests with OOM_UPDATE_SNAPSHOTS=1 to (re)write golden files
	OOM_UPDATE_SNAPSHOTS=1 bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked

.PHONY: test-update-pane-snapshots
test-update-pane-snapshots: ## Update only the owned-pane size golden
	OOM_UPDATE_SNAPSHOTS=1 cargo test -p oom-edit --offline --locked --lib snapshot_tests::golden_owned_pane_sizes -- --exact

.PHONY: test-all
test-all: test ## Tests + example builds
	cargo build --examples --offline --locked

.PHONY: terminal-guard-pty-test
terminal-guard-pty-test: ## Run the terminal guard's native PTY and signal tests
	cargo test --package oom-edit --test terminal_guard --offline --locked

# ---------------------------------------------------------------------------
# Format
# ---------------------------------------------------------------------------
.PHONY: fmt
fmt: ## Auto-format the workspace
	cargo fmt --all
	rustfmt --edition 2021 fixtures/downstream/src/lib.rs

.PHONY: fmt-check
fmt-check: ## Format check (CI gate)
	cargo fmt --all -- --check
	rustfmt --edition 2021 --check fixtures/downstream/src/lib.rs

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
.PHONY: ci
ci: ## Run release, core, strict coverage, example, documentation and performance gates
	$(MAKE) build-release
	$(MAKE) check
	$(MAKE) coverage-check
	$(MAKE) build-examples
	$(MAKE) doc
	$(MAKE) bench-check
	$(MAKE) bench

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
	if cargo fmt --all -- --check 2>&1 && rustfmt --edition 2021 --check fixtures/downstream/src/lib.rs 2>&1; then \
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
	if PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s .agents/skills/feature-workflow/tests -p 'test_*.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_tui_performance.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_ci_workflow.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_drd_coverage.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_regenerate_markdown.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_downstream.py' 2>&1 \
		&& bash scripts/with-isolated-config.sh cargo test --workspace --offline --locked 2>&1; then \
		echo "[PASS] test"; PASS=$$((PASS + 1)); \
	else \
		echo "[FAIL] test"; FAIL=$$((FAIL + 1)); test_ok=false; \
	fi; \
	echo ""; \
	echo "deny"; \
	if $(DENY_COMMAND) $(DENY_FLAGS) 2>&1; then \
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
deny: ## License/ban/advisory checks (CI gate; requires network)
	$(DENY_COMMAND) $(DENY_FLAGS)

.PHONY: audit
audit: ## RustSec advisory checks (CI gate; requires network)
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
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --offline --locked

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
bench: ## Run exact asserting release performance gates
	cargo bench --workspace --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/tui_performance.py gate
	$(MAKE) bench-pane-frame

.PHONY: bench-pane-frame
bench-pane-frame: ## Assert 200x60 owned-frame conversion is at most 1 ms in release
	cargo test -p oom-edit --release --offline --locked --lib perf_tests::pane_frame_conversion_release_limit -- --exact --ignored --nocapture --test-threads=1

.PHONY: bench-analysis
bench-analysis: ## Run the exact read-only analysis release benchmark
	OOM_BENCH_ANALYSIS_ONLY=1 cargo bench -p oom-edit-core --offline --locked --bench performance

.PHONY: bench-first-frame-profile
bench-first-frame-profile: ## Report first-frame parse, construction and layout costs in release
	OOM_BENCH_FIRST_FRAME_PROFILE=1 cargo bench -p oom-edit-core --offline --locked --bench performance

.PHONY: grammar-generate
grammar-generate: ## Regenerate the Markdown block parser offline with Tree-sitter 0.26.3
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/regenerate_markdown.py --generator "$(TREE_SITTER)" --output "$(GRAMMAR_OUTPUT)"

.PHONY: grammar-generation-test
grammar-generation-test: ## Test pinned offline Markdown regeneration and optimization guards
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_regenerate_markdown.py'

.PHONY: test-first-frame
test-first-frame: test-first-frame-leaves ## Verify Markdown parser build profiles and byte-exact unchanged leaf mapping
	cargo test -p oom-edit-core --offline --locked --test dependency_hygiene markdown_generated_parsers_respect_optimized_cargo_profiles -- --exact
	cargo test -p oom-edit-core --offline --locked --lib syntax::tests::prose_batching_

.PHONY: test-first-frame-leaves
test-first-frame-leaves: ## Check byte-exact parser-leaf provenance for first-frame optimization
	cargo test -p oom-edit-core --offline --locked --lib rendered::blocks::first_frame_tests
	cargo test -p oom-edit-core --offline --locked --lib rendered::wrap::tests::mapped_

.PHONY: bench-check
bench-check: ## Run asserting debug performance smoke gates
	cargo test -p oom-spell --offline --locked --test perf_smoke
	cargo test -p oom-edit-core --offline --locked --test perf_smoke
	cargo test -p oom-edit --offline --locked --lib perf_tests::tui_gutter_debug_performance_smoke -- --exact --ignored --test-threads=1

.PHONY: tui-perf-record
tui-perf-record: ## Record TUI performance TSV (BRANCH_ROLE, OUTPUT, TRIALS)
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/tui_performance.py --root "$(PERF_ROOT)" record --role "$(BRANCH_ROLE)" --output "$(OUTPUT)" --trials "$(TRIALS)"

.PHONY: tui-perf-baseline-prepare
tui-perf-baseline-prepare: ## Clone an immutable baseline locally without changing this worktree (BASELINE_DIR, BASELINE_REV)
	test -n "$(BASELINE_DIR)"
	test -n "$(BASELINE_REV)"
	git clone --local --no-hardlinks --no-checkout "$(CURDIR)" "$(BASELINE_DIR)"
	git -C "$(BASELINE_DIR)" checkout --detach "$(BASELINE_REV)"

.PHONY: tui-perf-compare
tui-perf-compare: ## Compare baseline/candidate TUI TSV evidence
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/tui_performance.py compare --baseline "$(BASELINE)" --candidate "$(CANDIDATE)" --output "$(OUTPUT)"

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
