# CLAUDE.md

Project conventions — architecture, Rust guidelines, testing layout and the
branch/PR workflow — live in **[AGENTS.md](AGENTS.md)**. It is the single source
of truth and is read by every agent harness. Read it first.

This file holds only what is specific to Claude Code.

## Subagents

Pass `isolation: "worktree"` when spawning subagents with the Agent tool, so
each works in its own git worktree and parallel edits never conflict. This is
the same rule AGENTS.md sets for top-level sessions.

## Verification

`just check` is the gate. It runs in seconds, so run it rather than reasoning
about whether a change was safe.

Do not report work as done before `just check` passes and the branch is pushed.
