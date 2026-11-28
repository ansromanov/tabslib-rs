# AGENTS.md — tabslib-rs

> Canonical instructions for this repository, read by Claude Code, Codex and other
> agents. Keep it short: no file tree and no symbol index, they rot. Use `ls`, `grep`
> and the module docs instead.

tabslib-rs is tablature and deterministic music generation for Rust. Licence: MIT OR Apache-2.0. This repository is public: nothing private
(credentials, host names, internal paths, other people's licensed material) goes in.

## Layout

- `src/model.rs`, `src/edits.rs`, `src/selection.rs` — the immutable document model and deterministic edits.
- `src/format/` — one adapter per format, each behind a Cargo feature; the core builds with `thiserror` only.
- `src/generation/`, `src/inspect/`, `src/evaluation.rs` — generation, analysis and assessment; AI connectors are optional features and off by default.
- `extras/` — supplementary crates; `docs/` — the mdBook; `examples/` — runnable examples.

## Rules worth knowing

- Everything is deterministic: the same input and seed give the same score. Do not read the clock, the environment or an unseeded RNG.
- The core must not depend on a format adapter (`just core-only` checks it).
- Public repository: no private corpora, licensed tablature or MIDI, or links to private projects. Read [CONTENT-POLICY.md](CONTENT-POLICY.md) before adding fixtures.

## Gate

Run this before opening a pull request. CI runs the same checks.

```sh
just check   # fmt, clippy --all-features, tests, rustdoc -D warnings, core-only, source gates, file size
```

## Testing

Each non-`mod.rs` module has a sibling `foo_test.rs` (`scripts/check-source-gates.py` enforces it for changed files; escape hatch: `[skip-tests: reason]` in a commit message). Changed files stay at or below 700 lines. Add or update tests with every behaviour change; say in the pull request why
something cannot be tested.

## Workflow

- Work in a git worktree on a branch, never on `main`. Fetch and base the branch on the latest `origin/main`.
- `main` is protected: every change is a pull request with green CI. Never push to `main`.
- Right after opening a pull request, turn on auto-merge: `gh pr merge --auto --squash`. GitHub merges it when the
  required checks pass; do not merge by hand and do not use `--admin`. Dependabot's pull requests get this from a workflow.
- Commit messages and pull request titles are [Conventional Commits](https://www.conventionalcommits.org)
  (`feat:`, `fix:`, `docs:`, `refactor:`, `ci:`, `chore:`); the changelog and versions come from them.
- Files stay at or below 800 production lines (`scripts/check-file-size.py`). Split the module; never raise
  the limit or add to `scripts/file-size-baseline.txt` (it can only shrink).
- Update the documentation in the same pull request as the behaviour: `docs/src` (mdBook), rustdoc and README.md.
- Tags `vMAJOR.MINOR.PATCH`; `release.yml` publishes the notes from `git-cliff`. Before tagging, also try `cargo add --git … --tag` in a fresh project: the lockfile hides dependency breaks.
- Never commit secrets. CI scans for them; a finding is a blocker, not noise.
