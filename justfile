# tabslib — see AGENTS.md for conventions.

default:
    @just --list

# The full local gate. Runs in seconds; run it rather than reasoning about
# whether a change was safe.
check: fmt-check lint test doc

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

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

doc-open:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --open

bench file:
    cargo run --release --example bench -- "{{file}}"

# Fresh worktree on a branch off origin/main. Every agent works in one.
new name:
    git fetch origin
    git worktree add .agent/worktrees/{{replace(name, "/", "-")}} -b {{name}} origin/main
    @echo "cd .agent/worktrees/{{replace(name, '/', '-')}}"

worktree-rm name:
    git worktree remove .agent/worktrees/{{replace(name, "/", "-")}}

# Rebase onto fresh main and push.
push:
    git fetch origin
    git rebase origin/main
    git push --force-with-lease -u origin HEAD

# fmt, gate, push, and open a PR that closes the issue.
ship issue:
    just fmt
    just check
    just push
    gh pr create --base main --fill --body "$(git log origin/main..HEAD --format='%s%n%n%b')

    Closes #{{issue}}"

clean:
    cargo clean
