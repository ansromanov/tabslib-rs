# AGENTS.md — tabslib

Single source of truth for architecture, code style, testing layout, and the
branch/PR workflow. Read by every agent harness. Harness-specific files
(`CLAUDE.md`, `.github/copilot-instructions.md`, `GEMINI.md`, `.cursor/rules/`)
point here and add nothing except what is genuinely specific to that tool.

---

# 1. Project

A Guitar Pro score engine: read, inspect, edit and write `.gp` files.

**Scope test** — before adding anything, ask:

> Can this be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

Transposing a track can. "Add a fill where the section changes" cannot. The
second belongs in a downstream crate. This test is the whole architecture; see
`ROADMAP.md` for the phase plan and `CONTENT-POLICY.md` for what may not enter
the repository at all.

## Layout

```
src/
  lib.rs           crate root, load/save entry points
  model.rs         the document model
  error.rs         Error and Result
  container.rs     GP7/8 zip container
  gpif/
    mod.rs         codec entry
    dom.rs         minimal read-only XML tree
    parse.rs       GPIF -> Document
    write.rs       Document -> GPIF
  fixtures.rs      generated test documents (public)
tests/             integration tests
examples/          runnable examples, including bench
```

## Key invariants

These are the ones the format punishes. Do not regress them.

- **Rhythm is never a float.** `Rhythm` keeps the written value, the dots and
  the tuplet separately; `as_fraction()` is exact integer arithmetic. A lookup
  table over `f64` needs a fallback, and that fallback silently rewrites the
  music — a missed value turns a triplet into a quarter note while the note
  count stays correct, so nothing downstream notices.
- **Several techniques live in XML attributes, not element names.** GPIF writes
  `<Property name="PalmMuted">`. Match on element names and a file full of palm
  mutes reads as having none. Use `Element::property()`.
- **Rhythms are referenced, not inlined.** `<Rhythm ref="N"/>` against
  `<Rhythm id="N">`, and sixteenths spell `16th`, not `Sixteenth`.
- **Nodes reference each other by id.** No `parent`, `next` or `previous`
  fields. Ownership stays a tree and edits stay value-shaped.
- **Writes are byte-deterministic.** The zip timestamp is fixed. Saving the same
  document twice must produce identical bytes.

---

# 2. Rust guidelines

## Style

- **Indent** 4 spaces, no tabs. **Line length** 100 max.
- **Naming** `snake_case` for functions, vars and modules; `PascalCase` for
  types and enums.
- **Imports** grouped std → external → local, separated by blank lines. No
  wildcard imports except `use super::*;` in tests.
- **Doc comments on every public item.** `#![warn(missing_docs)]` is on and CI
  builds docs with warnings denied.
- **File-level module docs.** Every non-test `.rs` under `src/` opens with a
  `//!` block saying what the module does and which public items it owns.
  Keeping it in sync is part of any PR that changes the file's behaviour.
- **No emoji or non-ASCII** in source, except in tests exercising encoding.
- **Explicit `.clone()`** on non-`Copy` types. No hidden clones.

## Error handling

- `thiserror` for the crate's `Error`; no `anyhow` in library code.
- **No `unwrap` or `expect` in library paths.** Tests may use them freely.
- **No silently-swallowed failures.** `let _ = …` is acceptable only for
  genuinely best-effort work. A parse that cannot represent its input returns an
  error or a diagnostic — it never substitutes a default and continues.

  This last rule is the crate's central lesson. A codec that quietly substitutes
  a plausible value produces a file that is wrong in a way no count detects.

## Correctness

- **Exhaustive `match` on the crate's own enums.** No `_ =>` arm on `Technique`,
  `NoteValue` or any other closed set: adding a variant must break the build at
  every site that has to handle it. A catch-all arm is how a new technique gets
  silently dropped on write.
- **Bounds-checked access.** `.get(i)` for any index not provably in range.
- **Parse defensively, write exactly.** The reader tolerates variation in the
  input (`16th` and `Sixteenth` both parse). The writer emits exactly one form:
  the one Guitar Pro itself writes.
- **No `as` casts that can truncate** where the value is data. Use `try_into()`
  and handle the failure.

## Testing

Tests are **co-located** with the module they cover, in a sibling `_test.rs`
file — never inline `#[cfg(test)] mod tests { … }` blocks.

- `src/model.rs` → `src/model_test.rs`
- `src/gpif/parse.rs` → `src/gpif/parse_test.rs`

Each `_test.rs` opens with `use super::*;` and contains bare `#[test]`
functions. The source file wires it up with one line:

```rust
#[cfg(test)]
#[path = "model_test.rs"]
mod tests;
```

Cross-module and black-box tests live in `tests/`.

**No test may read a file from disk.** `tabslib::fixtures` generates the
documents tests need, from generic instrumental material — scales, repeated
frets, power chords, note values, standard drum patterns. Nothing transcribed
from a recording. A test that needs a new shape adds a fixture.

**The assertion rule.** State what your test would report if the feature under
test were silently dropped. **If the answer is "pass", the test is wrong.**

Note count, title and track count all stay correct while rhythm and
articulation are being destroyed. Assert the thing that changes: a duration
histogram, a technique census, a per-note string/fret list.

**Every code change ships with tests** — feature, fix or refactor. If a change
is genuinely untestable, say why in the PR body.

## File size

Source files under **700 lines**. Past that, split into a module directory.
When a module is split, its `_test.rs` splits in the same PR.

---

# 3. Dev flow

## Commands

| Command | Action |
|---|---|
| `just check` | fmt, clippy, tests, docs — the full local gate |
| `just test` | `cargo nextest run`, falling back to `cargo test` |
| `just fmt` | `cargo fmt --all` |
| `just lint` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `just doc` | Build docs with warnings denied |
| `just bench <file.gp>` | Timing on a real file |
| `just new <name>` | Fresh worktree + branch off `origin/main` |
| `just ship <issue>` | fmt, check, push, open a PR that closes the issue |
| `just` | List recipes |

The suite runs in well under a second, so there is no targeted-test recipe here
and no reason to skip the full run.

## Worktrees — mandatory for every agent

**Every agent — Claude Code, Codex, Cursor, opencode, Kilo Code, Antigravity or
any other — works in a dedicated git worktree, never in the primary checkout.**
Parallel sessions otherwise clobber each other's edits and switch the branch out
from under a running build.

```bash
just new feat/my-change
# equivalently:
git fetch origin
git worktree add .agent/worktrees/feat-my-change -b feat/my-change origin/main
cd .agent/worktrees/feat-my-change
```

`.agent/worktrees/` is gitignored. Remove the worktree when the PR merges:
`git worktree remove .agent/worktrees/<name>`.

Claude Code subagents get this automatically with `isolation: "worktree"`.

## Branching

Always branch from fresh `origin/main`, never from another feature branch.
Prefixes: `feat/`, `fix/`, `docs/`, `chore/`, `test/`, `refactor/`.

If a session starts on an auto-generated branch name, rename it before pushing.

## Pull requests — the only way to land

**`main` is protected. Direct pushes are rejected. Every change goes through a
PR, and every PR runs the full gate.**

```bash
just ship 12      # fmt, check, push, gh pr create with "Closes #12"
```

Rules, all enforced by CI or by `just`:

1. **Every PR closes an issue.** The body contains `Closes #<n>`. CI fails the
   PR otherwise; apply the `no-issue` label for genuinely issue-less changes.
2. **Every PR has a real body** saying what changed and why. Never empty, never
   the auto-stub, never a draft.
3. **Rebase onto fresh `origin/main` before pushing.** Never open a PR that
   needs a merge commit to be reviewable.
4. **Fix review comments on the same branch.** More commits update the PR;
   never open a second one.
5. **Green CI before merge.** fmt, clippy, tests, docs and MSRV all pass.
6. **Squash-merge**, so `main` keeps one commit per PR.

## Before committing

1. `just check` — the whole gate
2. Module docs updated if behaviour changed
3. `ROADMAP.md` checkbox ticked if a phase item landed
4. No `dbg!`, no stray `println!`, no commented-out code
5. No secrets, no absolute paths from a developer machine
6. No file that is not yours to publish — see `CONTENT-POLICY.md`

## Commit messages

`type(scope): summary` in the imperative, under 72 characters. Body explains
*why*, wrapped at 80. Prose, not bullet fragments.

State what was measured, not what was intended. "Save collapsed 640 triplets to
quarters" beats "improve rhythm handling".
