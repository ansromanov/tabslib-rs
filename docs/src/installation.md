# Installation

tabslib needs Rust 1.88 or newer. It is not
yet published to crates.io, so depend on it from Git.

## As a library

```sh
cargo add tabslib --git https://github.com/ansromanov/tabslib-rs
```

or in `Cargo.toml`:

```toml
[dependencies]
tabslib = { git = "https://github.com/ansromanov/tabslib-rs" }
```

Pin a release with `tag = "v0.1.1"`, or a revision with `rev = "<commit>"`.

## Choosing features

The defaults cover the GP codec, the ASCII, HTML and PDF renderers, and the
generation and assessment modules. Everything else is opt-in:

| feature | adds | default |
| --- | --- | :-: |
| `gp` | GP7/8 read and write | yes |
| `ascii`, `html`, `pdf` | renderers | yes |
| `json` | versioned native JSON document format | yes |
| `drums`, `rhythm`, `harmony`, `riffs`, `arrangement`, `assessment` | generation and assessment | yes |
| `midi` | MIDI read and write | no |
| `musicxml` | MusicXML read and write | no |
| `wav` | PCM rendering through a SoundFont | no |
| `ai` | prompt construction and response validation, no network | no |
| `ai-anthropic`, `ai-openai-compatible`, `ai-gemini`, `ai-ollama` | HTTP connectors | no |

The smallest build is the model, inspection and edits alone, with `thiserror`
as the only dependency:

```toml
tabslib = { git = "https://github.com/ansromanov/tabslib-rs", default-features = false }
```

Add features by name:

```sh
cargo add tabslib --git https://github.com/ansromanov/tabslib-rs --features midi,musicxml
```

## The `tabs` command-line tool

The repository also ships `tabs`, a small CLI for everyday score chores
(`info`, `tab`, `transpose`, `slice`, `midi`):

```sh
cargo install --git https://github.com/ansromanov/tabslib-rs tabs
tabs info song.gp
tabs tab song.gp --start 3 --end 6
```
