# Releases

Tabslib uses Conventional Commits to choose SemVer increments. `feat` commits
bump the minor version, `fix` and `perf` bump the patch version, and breaking
changes bump the major version. `build`, `chore`, `ci`, `docs`, `refactor`, and
`test` commits do not create a release by themselves.

Before a release, determine the next version from the latest tag and commits:

```sh
git-cliff --offline --bumped-version
```

Update `package.version` in both `Cargo.toml` and `Cargo.lock` in a reviewed PR,
merge it after the normal CI gate passes, then create an annotated `vMAJOR.MINOR.PATCH` tag on that
main commit and push the tag. The release workflow checks that the tag matches
`Cargo.toml`, generates `CHANGELOG.md` and release notes, and publishes a GitHub
Release with the changelog attached. The crate is not yet published to
crates.io; releases are source tags and GitHub release notes.
