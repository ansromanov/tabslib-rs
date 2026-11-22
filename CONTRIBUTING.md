# Contributing

## Quick start

```sh
git clone https://github.com/ansromanov/tabslib-rs
cd tabslib-rs
just check          # fmt, clippy, tests, docs — runs in seconds
```

Without `just`, the same gate is `cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test --all-features` and `cargo doc --no-deps --all-features`.

## The short version

- `main` is protected. Every change lands through a pull request that passes
  the full gate.
- Commit messages follow Conventional Commits; releases are cut from them.
- Tests are co-located in sibling `_test.rs` files and read no files from disk;
  `tabslib::fixtures` generates what they need.
- Every test must assert something that *changes* when the feature under test is
  silently dropped. A note count does not.
- Public items are documented; `cargo doc` runs with warnings denied.

## What belongs here

Model, codecs, deterministic edits, and style-neutral generation and
assessment. Ask:

> Can this be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

If not, it belongs in the calling application. See [ROADMAP.md](ROADMAP.md).

Read [CONTENT-POLICY.md](CONTENT-POLICY.md) before adding any file: no
third-party musical content and no copyrighted reference texts, ever.

## License of contributions

The project is licensed under [FSL-1.1-MIT](LICENSE.md). By submitting a pull
request you agree that your contribution is licensed under the same terms.
