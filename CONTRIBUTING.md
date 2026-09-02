# Contributing

Conventions live in [AGENTS.md](AGENTS.md) — architecture, Rust guidelines,
testing layout and the PR workflow. It is written for both people and agents.

## Quick start

```sh
git clone https://github.com/ansromanov/tabslib-rs
cd tabslib-rs
just check          # fmt, clippy, tests, docs — runs in seconds
```

## The short version

- `main` is protected. Every change lands through a pull request.
- Every PR closes an issue (`Closes #12`) and passes the full gate.
- Tests are co-located in sibling `_test.rs` files and read no files from disk;
  `tabslib::fixtures` generates what they need.
- Every test must assert something that *changes* when the feature under test is
  silently dropped. A note count does not.
- Public items are documented; `cargo doc` runs with warnings denied.

## What belongs here

Model, codecs and deterministic edits. Ask:

> Can this be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

If not, it belongs in a downstream crate. See [ROADMAP.md](ROADMAP.md).

Read [CONTENT-POLICY.md](CONTENT-POLICY.md) before adding any file: no
third-party musical content and no copyrighted reference texts, ever.
