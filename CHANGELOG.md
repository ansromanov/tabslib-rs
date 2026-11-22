# Changelog

All notable changes are recorded here. Releases are cut from Conventional
Commits with `git-cliff`; see [releases](docs/src/releases.md).

## 0.1.1 - 2026-11-22

### Bug fixes

- Build against `zip` 2.6, which a dependent resolves without the lockfile;
  0.1.0 failed to compile there. CI now also tests with the newest compatible
  dependencies.

### Changes

- Minimum supported Rust version is 1.88 for every feature (was 1.85 for all
  but the AI connectors); the newest compatible dependencies need it.

## 0.1.0 - 2026-11-22

### Features

- Initial public release: GP7/8, MIDI and MusicXML codecs; ASCII, HTML, PDF and
  WAV renderers; inspection; deterministic edits; style-neutral generation,
  arrangement and assessment; optional AI connectors for Anthropic,
  OpenAI-compatible services, Gemini and Ollama, configured through `AiConfig`.

### Bug fixes

- `EditError`, `PitchError` and `SelectionError` implement `std::error::Error`,
  so they propagate with `?` (#4).
- Index-free iteration in five loops flagged by clippy on Rust 1.100 (#1).

### Documentation

- Every public item is documented and `missing_docs` is enforced (#4).
- The tabslib book and API reference on GitHub Pages (#5).

### Licensing

- Dual-licensed under MIT OR Apache-2.0 (#3).
