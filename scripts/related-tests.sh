#!/usr/bin/env bash
# Prints a cargo-nextest filter expression covering the tests related to the
# files changed against a base ref, or nothing when the change is broad enough
# that everything should run.
#
# The co-located test convention makes the mapping direct: a change to
# src/gpif/parse.rs implicates src/gpif/parse_test.rs.
#
# A filter that matches no tests is NOT a pass. Callers must check the count and
# fall back to the whole suite -- test names describe behaviour rather than
# modules, so a module-derived filter can legitimately select nothing, and an
# empty run reports green while testing exactly nothing.
#
# Usage: scripts/related-tests.sh [base-ref]      (default: origin/main)
set -euo pipefail

base="${1:-origin/main}"
# Diff the base against the working tree, not against HEAD, so uncommitted
# edits are covered too -- otherwise a local run selects tests for work that is
# already committed and ignores what you are actually changing.
changed="$(git diff --name-only "$base"; git ls-files --others --exclude-standard)"

# Anything that changes what the whole crate compiles to means run everything.
if echo "$changed" | grep -qE '^(Cargo\.(toml|lock)|rustfmt\.toml|src/lib\.rs|src/model\.rs|src/error\.rs)$'; then
  exit 0
fi


mods=""
while read -r f; do
  case "$f" in
    src/*.rs) m="$(basename "$f" .rs)"; mods="$mods ${m%_test}" ;;
    tests/*.rs) mods="$mods $(basename "$f" .rs)" ;;
  esac
done <<< "$changed"

# grep exits 1 on no match, which under `set -e` would kill the script on the
# perfectly normal "nothing relevant changed" path.
mods="$(echo "$mods" | tr ' ' '\n' | grep -v '^$' | sort -u || true)"
[ -z "$mods" ] && exit 0

expr=""
for m in $mods; do
  [ -n "$expr" ] && expr="$expr | "
  expr="${expr}test(/$m/)"
done
echo "$expr"
