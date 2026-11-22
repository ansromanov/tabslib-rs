# Changelog

All notable changes are recorded here. Releases are cut from Conventional
Commits with `git-cliff`; see [releases](docs/src/releases.md).

## Unreleased

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
- The tabslib book and API reference on GitHub Pages.

### Licensing

- Dual-licensed under MIT OR Apache-2.0 (#3).
