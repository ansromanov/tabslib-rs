Project conventions — architecture, Rust guidelines, testing layout and the
branch/PR workflow — live in AGENTS.md at the repository root. It is the single
source of truth for every agent working here. Read it before making changes.

Points that are easy to get wrong and are covered there in full:

- Work in a git worktree under .agent/worktrees/, never the primary checkout.
- Tests are co-located in sibling _test.rs files, never inline #[cfg(test)] mod
  blocks, and no test may read a file from disk — use tabslib::fixtures.
- Every test must assert something that changes when the feature under test is
  silently dropped. A note count does not.
- main is protected: every change lands through a pull request that closes an
  issue and passes the full CI gate.
