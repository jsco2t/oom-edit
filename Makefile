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
	@grep -E '^[a-zA-Z0-9_-]+:.*##' $(MAKEFILE_LIST) \
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
test: feature-workflow-test tui-perf-test realistic-perf-test acceptance-1mb-perf-test rss-stability-test ci-workflow-test drd-coverage-test grammar-generation-test downstream-tool-test terminal-guard-tool-test ## Run the full test suite
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

.PHONY: test-paste-work
test-paste-work: ## Verify bounded paste indexing and atomic borrowed front-matter refresh
	cargo test --package oom-edit-core --lib --offline --locked paste_refresh_borrows_updated_text
	cargo test --package oom-edit-core --test dependency_hygiene --offline --locked paste_byte_offsets_use_the_rope_index -- --exact
	cargo test --package oom-edit-core --lib --offline --locked navigation_and_frames_reuse_materialization_layout_and_line_index_work

.PHONY: test-shift-v
test-shift-v: ## Verify terminal Shift+V line Select and modifier boundaries in core and both hosts
	cargo test --package oom-edit-core --test session_integration --offline --locked shift_v
	cargo test --package oom-edit --offline --locked --lib shift_v
	cargo test --package oom-edit --test embedding_example --offline --locked shift_v
	$(MAKE) test-embedding-example

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

.PHONY: realistic-perf-test
realistic-perf-test: ## Test realistic fixture and measurement contracts
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_realistic_performance.py'
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_interaction_performance.py'
	cargo test --package oom-edit --test realistic_fixtures --offline --locked

.PHONY: test-realistic-performance
test-realistic-performance: realistic-perf-test ## Verify realistic fixture and runner shape

.PHONY: acceptance-1mb-perf-test
acceptance-1mb-perf-test: ## Test exact large-note acceptance runner contracts
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_acceptance_1mb.py'
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_acceptance_1mb_prose_cycles.py'

.PHONY: rss-stability-test
rss-stability-test: ## Test current-RSS stability runner contracts
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_rss_stability.py'

.PHONY: test-incremental
test-incremental: ## Run bounded-update differential and propagation tests
	cargo test -p oom-edit-core --lib syntax::tests --offline --locked
	cargo test -p oom-edit-core --lib rendered::retained::tests --offline --locked
	cargo test -p oom-edit-core --lib rendered::rows::tests --offline --locked

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
terminal-guard-pty-test: terminal-guard-tool-test ## Run the terminal guard's native PTY and signal tests
	cargo test --package oom-edit --test terminal_guard --offline --locked

.PHONY: terminal-guard-tool-test
terminal-guard-tool-test: ## Verify native PTY terminal-state comparison guards
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_terminal_guard_pty.py'

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
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_realistic_performance.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_interaction_performance.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_acceptance_1mb.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_acceptance_1mb_prose_cycles.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_rss_stability.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_ci_workflow.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_drd_coverage.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_regenerate_markdown.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_downstream.py' 2>&1 \
		&& PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_terminal_guard_pty.py' 2>&1 \
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

.PHONY: bench-realistic
bench-realistic: ## Assert cold public-pane performance across realistic Markdown classes (OUTPUT=optional raw path)
	cargo build --release --package oom-edit --example performance_realistic --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/realistic_performance.py gate --binary target/release/examples/performance_realistic --trials 5 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-realistic-record
bench-realistic-record: ## Record realistic baseline (TRIALS=1 OUTPUT=/tmp/file.jsonl PERF_SKIP_LAYOUT=1)
	cargo build --release --package oom-edit --example performance_realistic --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/realistic_performance.py record --binary target/release/examples/performance_realistic --trials $(or $(TRIALS),1) $(if $(PERF_SKIP_LAYOUT),--skip-layout,) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-rss-stability
bench-rss-stability: ## Assert 400-cycle current-RSS stability for exact Rust/Go edits, mode transitions and reloads (OUTPUT=raw path)
	cargo build --release --package oom-edit --example performance_rss_stability --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/rss_stability.py --gate --binary target/release/examples/performance_rss_stability --fixture examples/kitchen-sink-1mb.md --source-root . --trials 3 --count 400 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-rss-stability-record
bench-rss-stability-record: ## Record exact current-RSS cycles without gate (COUNT=100/200/300/400 TRIALS=1 SCENARIO=rust-reload OUTPUT=raw path)
	cargo build --release --package oom-edit --example performance_rss_stability --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/rss_stability.py --binary target/release/examples/performance_rss_stability --fixture examples/kitchen-sink-1mb.md --source-root . --trials $(or $(TRIALS),1) --count $(or $(COUNT),100) $(if $(SCENARIO),--scenario $(SCENARIO),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-interactions-record
bench-interactions-record: ## Record public-pane interactions (TRIALS=5 ITERATIONS=21 CASE=source-line SIZE=1048576 OUTPUT=/tmp/file.jsonl)
	cargo build --release --package oom-edit --example performance_realistic --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/interaction_performance.py --binary target/release/examples/performance_realistic --trials $(or $(TRIALS),5) --iterations $(or $(ITERATIONS),21) $(if $(CASE),--case $(CASE),) $(if $(SIZE),--size $(SIZE),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-interactions
bench-interactions: ## Assert 1 MiB public-pane local edits, scrolling, and cold/warmed mode returns
	cargo build --release --package oom-edit --example performance_realistic --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/interaction_performance.py --gate --binary target/release/examples/performance_realistic --trials 5 --iterations 21 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-record
bench-acceptance-1mb-record: ## Record exact kitchen-sink keyboard and Select latency (TRIALS, OUTPUT, SCENARIO)
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb.py --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials $(or $(TRIALS),5) $(if $(SCENARIO),--scenario $(SCENARIO),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb
bench-acceptance-1mb: ## Assert exact kitchen-sink keyboard, Select and edit latency
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb.py --gate --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials 5 $(if $(OUTPUT),--output $(OUTPUT),)
	cargo build --release --package oom-edit --bin oom-edit --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_pty.py --gate --binary target/release/oom-edit --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials 5 --keys 1000

.PHONY: bench-acceptance-1mb-navigation
bench-acceptance-1mb-navigation: ## Assert sustained 1 MiB navigation through the public pane and 63x229 standalone PTY
	cargo build --release --package oom-edit --example performance_acceptance_1mb --bin oom-edit --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb.py --gate --navigation-only --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials 5
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_pty.py --gate --binary target/release/oom-edit --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials 5 --keys 1000 --rows 63 --cols 229 --term xterm-kitty

.PHONY: bench-acceptance-1mb-select-motion
bench-acceptance-1mb-select-motion: ## Assert 1 MiB Select motion latency in flat and retained views (OUTPUT=optional raw path)
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb.py --select-motion-gate --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials 7 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-edit-cycles
bench-acceptance-1mb-edit-cycles: ## Assert 100 exact Rust/Go fence delete/change/undo cycles through the public pane (OUTPUT=optional raw path)
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_edit_cycles.py --gate --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --count 100 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-prose-cycles-record
bench-acceptance-1mb-prose-cycles-record: ## Record exact 1 MiB prose/list Select delete/change/undo cycles (COUNT, OUTPUT)
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_prose_cycles.py --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --count $(or $(COUNT),1) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-prose-cycles
bench-acceptance-1mb-prose-cycles: ## Assert 100 exact 1 MiB prose/list Select delete/change/undo cycles each (OUTPUT)
	cargo build --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_prose_cycles.py --gate --binary target/release/examples/performance_acceptance_1mb --fixture examples/kitchen-sink-1mb.md --source-root . --count 100 $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-pty-record
bench-acceptance-1mb-pty-record: ## Record standalone PTY output timing (TRIALS, OUTPUT, REGION, ROWS, COLS, SERIAL_DELAY_MS, BURST_CADENCE_MS, TERM_NAME, POST_EDIT, FIRST_FRAME_TIMEOUT_S)
	cargo build --release --package oom-edit --bin oom-edit --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_pty.py --binary target/release/oom-edit --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials $(or $(TRIALS),5) --keys $(or $(KEYS),1000) --rows $(or $(ROWS),41) --cols $(or $(COLS),100) --serial-delay-ms $(or $(SERIAL_DELAY_MS),2) --burst-cadence-ms $(or $(BURST_CADENCE_MS),2) --term $(or $(TERM_NAME),xterm-256color) --first-frame-timeout-s $(or $(FIRST_FRAME_TIMEOUT_S),15) $(if $(POST_EDIT),--post-edit,) $(if $(REGION),--region $(REGION),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-core-phases
bench-acceptance-1mb-core-phases: ## Record exact-note core construction and Select projection phases (TRIALS, OUTPUT)
	cargo build --release --package oom-edit-core --example performance_acceptance_1mb_core --offline --locked
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_core.py --binary target/release/examples/performance_acceptance_1mb_core --fixture examples/kitchen-sink-1mb.md --source-root . --role candidate --trials $(or $(TRIALS),5) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-mutation-phases
bench-acceptance-1mb-mutation-phases: ## Trace exact-note mutation parser and model work
	cargo test -p oom-edit-core --lib --offline --locked acceptance_1mb_mutation_work_profile -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-prose-batch-profile
bench-acceptance-1mb-prose-batch-profile: ## Compare exact-note multi-range source batching and full-model costs
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_prose_batch_profile -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-prose-model-window
bench-acceptance-1mb-prose-model-window: ## Compare affected 1 MiB Markdown block windows to the complete builder
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_local_model_windows_match_full_builder -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-prose-row-window
bench-acceptance-1mb-prose-row-window: ## Compare affected 1 MiB block rows to the complete rendered builder
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_local_block_rows_match_complete_builder -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-feasibility
bench-acceptance-1mb-feasibility: ## Trace contiguous large-fence Vim and source-update cost
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_contiguous_fence_edit_profile -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-injection-parse
bench-acceptance-1mb-injection-parse: ## Compare incremental and full code-injection parse on exact note
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_injection_incremental_profile -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-fence-model
bench-acceptance-1mb-fence-model: ## Compare local large-fence model splice to complete builder
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_fence_model_splice_matches_full_builder -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-selection-prototype
bench-acceptance-1mb-selection-prototype: ## Compare indexed short Select projection to full-layout oracle
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_bounded_character_projection_matches_complete -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-code-rows
bench-acceptance-1mb-code-rows: ## Compare local source-highlight code rows to complete renderer
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_source_highlight_builds_exact_code_rows -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-source-prototype
bench-acceptance-1mb-source-prototype: ## Compare bounded source-fence edits and highlighting to fresh analysis
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_bounded_source_fence_matches_fresh -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-row-splice
bench-acceptance-1mb-row-splice: ## Compare lazy-offset large-fence rows to full layout
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_fence_row_splice_matches_complete_layout -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-vim-splice
bench-acceptance-1mb-vim-splice: ## Compare one-undo rope delete with the projected full-replace reference
	cargo test -p oom-edit-core --release --lib --offline --locked acceptance_1mb_contiguous_vim_delete_matches_reference -- --ignored --nocapture

.PHONY: bench-acceptance-1mb-baseline-prepare
bench-acceptance-1mb-baseline-prepare: tui-perf-baseline-prepare ## Build clean main probe without changing the active worktree (BASELINE_DIR, BASELINE_REV)
	cp crates/oom-edit/examples/performance_acceptance_1mb.rs "$(BASELINE_DIR)/crates/oom-edit/examples/performance_acceptance_1mb.rs"
	cp crates/oom-edit-core/examples/performance_acceptance_1mb_core.rs "$(BASELINE_DIR)/crates/oom-edit-core/examples/performance_acceptance_1mb_core.rs"
	cp examples/kitchen-sink-1mb.md "$(BASELINE_DIR)/examples/kitchen-sink-1mb.md"
	cargo build --manifest-path "$(BASELINE_DIR)/Cargo.toml" --release --package oom-edit --example performance_acceptance_1mb --offline --locked
	cargo build --manifest-path "$(BASELINE_DIR)/Cargo.toml" --release --package oom-edit-core --example performance_acceptance_1mb_core --offline --locked
	cargo build --manifest-path "$(BASELINE_DIR)/Cargo.toml" --release --package oom-edit --bin oom-edit --offline --locked

.PHONY: bench-acceptance-1mb-baseline-record
bench-acceptance-1mb-baseline-record: ## Record clean main against exact fixture (BASELINE_DIR, TRIALS, OUTPUT, SCENARIO)
	test -n "$(BASELINE_DIR)"
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb.py --binary "$(BASELINE_DIR)/target/release/examples/performance_acceptance_1mb" --fixture "$(BASELINE_DIR)/examples/kitchen-sink-1mb.md" --source-root "$(BASELINE_DIR)" --role main --trials $(or $(TRIALS),5) $(if $(SCENARIO),--scenario $(SCENARIO),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-baseline-pty-record
bench-acceptance-1mb-baseline-pty-record: ## Record clean main terminal output (BASELINE_DIR, TRIALS, OUTPUT, REGION, ROWS, COLS, SERIAL_DELAY_MS, BURST_CADENCE_MS, TERM_NAME, POST_EDIT)
	test -n "$(BASELINE_DIR)"
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_pty.py --binary "$(BASELINE_DIR)/target/release/oom-edit" --fixture "$(BASELINE_DIR)/examples/kitchen-sink-1mb.md" --source-root "$(BASELINE_DIR)" --role main --trials $(or $(TRIALS),5) --keys $(or $(KEYS),1000) --rows $(or $(ROWS),41) --cols $(or $(COLS),100) --serial-delay-ms $(or $(SERIAL_DELAY_MS),2) --burst-cadence-ms $(or $(BURST_CADENCE_MS),2) --term $(or $(TERM_NAME),xterm-256color) $(if $(POST_EDIT),--post-edit,) $(if $(REGION),--region $(REGION),) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-baseline-core-phases
bench-acceptance-1mb-baseline-core-phases: ## Record clean main core phases (BASELINE_DIR, TRIALS, OUTPUT)
	test -n "$(BASELINE_DIR)"
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_core.py --binary "$(BASELINE_DIR)/target/release/examples/performance_acceptance_1mb_core" --fixture "$(BASELINE_DIR)/examples/kitchen-sink-1mb.md" --source-root "$(BASELINE_DIR)" --role main --trials $(or $(TRIALS),5) $(if $(OUTPUT),--output $(OUTPUT),)

.PHONY: bench-acceptance-1mb-compare
bench-acceptance-1mb-compare: ## Compare pane/PTY/core raw profiles (PANE_MAIN, PANE_CANDIDATE, PTY_MAIN, PTY_CANDIDATE, CORE_MAIN, CORE_CANDIDATE)
	PYTHONDONTWRITEBYTECODE=1 python3 scripts/acceptance_1mb_compare.py --pane-main "$(PANE_MAIN)" --pane-candidate "$(PANE_CANDIDATE)" --pty-main "$(PTY_MAIN)" --pty-candidate "$(PTY_CANDIDATE)" --core-main "$(CORE_MAIN)" --core-candidate "$(CORE_CANDIDATE)"

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
