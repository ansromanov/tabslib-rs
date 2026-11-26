#!/usr/bin/env python3
"""Check changed Rust modules for size and co-located test coverage."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

LIMIT = 700


def tests_skipped() -> bool:
    """True when a commit since origin/main carries `[skip-tests: reason]`."""
    log = subprocess.run(
        ["git", "log", "origin/main..HEAD", "--format=%B"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    return "[skip-tests:" in log


def main() -> int:
    changed = {line.strip() for line in sys.stdin if line.strip()}
    failures: list[str] = []
    skip_tests = tests_skipped()
    for name in sorted(changed):
        if not name.startswith("src/") or not name.endswith(".rs"):
            continue
        path = Path(name)
        if name.endswith("_test.rs") or not path.is_file():
            continue
        with path.open(encoding="utf-8") as source:
            lines = sum(1 for _ in source)
        if lines > LIMIT:
            failures.append(f"{name}: {lines} lines exceeds the {LIMIT}-line limit; split it")
        if not skip_tests and path.name not in {"mod.rs", "lib.rs", "main.rs"}:
            sibling = path.with_name(f"{path.stem}_test.rs").as_posix()
            if sibling not in changed or not Path(sibling).is_file():
                failures.append(f"{name}: add or update sibling test {sibling} in this change")

    if failures:
        print("\n".join(failures))
        return 1
    print(f"[source-gates] changed Rust modules meet the {LIMIT}-line and sibling-test rules")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
