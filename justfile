# tabslib — see CONTRIBUTING.md for conventions.

default:
    @just --list

# The full local gate. Runs in seconds; run it rather than reasoning about
# whether a change was safe.
check: fmt-check lint test doc core-only source-gates

# Fast deterministic gate for local iteration and the shared project baseline.
fast-test:
    cargo test --all-features

test-fast: fast-test

# Check changed Rust modules against the 700-line and sibling-test gates.
source-gates:
    #!/usr/bin/env bash
    set -euo pipefail
    git diff --name-only origin/main...HEAD | python3 scripts/check-source-gates.py

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    @if command -v cargo-nextest >/dev/null 2>&1; then \
        cargo nextest run --all-features; \
    else \
        cargo test --all-features; \
    fi

# Only the tests related to your diff, falling back to the whole suite when the
# change is broad or the filter matches nothing. The suite runs in milliseconds
# today, so this exists to stay correct as it grows rather than to save time.
test-pr base="origin/main":
    #!/usr/bin/env bash
    set -euo pipefail
    filter="$(scripts/related-tests.sh {{base}})"
    if [ -z "$filter" ]; then
        echo "Broad change: running the whole suite."
        cargo nextest run --all-features
        exit 0
    fi
    n=$(cargo nextest list --all-features -E "$filter" 2>/dev/null | grep -cE '^[[:space:]]+[[:alnum:]]' || true)
    echo "Filter '$filter' selects $n test(s)."
    if [ "$n" -eq 0 ]; then
        echo "Filter matched nothing; running the whole suite."
        cargo nextest run --all-features
    else
        cargo nextest run --all-features -E "$filter"
    fi

# The core must not depend on any format adapter.
core-only:
    cargo build --no-default-features
    @cargo tree --no-default-features -e normal --prefix none | grep -qE '^(zip|quick-xml) ' \
        && (echo "a format dependency is reachable without its feature"; exit 1) \
        || echo "core is format-independent"

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

doc-open:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --open

bench file:
    cargo run --release --example bench -- "{{file}}"

clean:
    cargo clean
