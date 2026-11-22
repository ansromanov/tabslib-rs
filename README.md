# tabslib

**Tablature and deterministic music generation for Rust.**

[![CI](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-book%20%26%20API-blue)](https://ansromanov.github.io/tabslib-rs/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
![MSRV](https://img.shields.io/badge/rust-1.88%2B-orange)

```sh
cargo add tabslib --git https://github.com/ansromanov/tabslib-rs
```

tabslib reads, inspects, edits and writes fretted-instrument notation — Guitar
Pro 7/8, MIDI, MusicXML — and renders it as ASCII tablature, HTML, PDF or WAV.
On the same document model it generates drums, bass, harmony, solos, riffs and
whole arrangements from a seed, and assesses the result. Everything is
deterministic: the same input and seed always give the same score.

```rust
use tabslib::format::{ascii, gp::Gp};
use tabslib::{pitch, ReadFormat, WriteFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = tabslib::fixtures::power_chords(); // or tabslib::load(&std::fs::read("song.gp")?)?
    pitch::transpose(&mut doc, 2)?;                  // up a whole tone
    println!("{}", ascii::render_track(&doc, 0, 0, 3)?);

    let bytes = Gp::write(&doc)?;                    // lossless round trip
    assert_eq!(Gp::read(&bytes)?.note_count(), doc.note_count());
    Ok(())
}
```

## Features

- **Formats** — GP7/8 read and write with a lossless round trip; MIDI and
  MusicXML read and write; ASCII, HTML, PDF and WAV rendering. Each format is a
  feature; the core builds with `thiserror` as its only dependency.
- **Exact model** — an owned, immutable document with rhythm as exact
  fractions, never floats, and nodes linked by id.
- **Inspection** — statistics, key and mode with confidence, chords with Roman
  numerals, bar integrity.
- **Deterministic edits** — transpose, retune (keeping pitch or fingering),
  slice, splice, techniques over a selection; guards refuse to leave an
  over-full bar or unplayable note.
- **Generation** — seeded drums with limb-aware playability, bass, harmony,
  second guitar, solos, riffs from a declarative spec, arrangements and search.
- **Assessment** — structural and quality metrics compared against reference
  values you supply; nothing style-specific is compiled in.
- **AI, optional** — rationale and candidate comparison through Anthropic,
  OpenAI-compatible, Gemini or Ollama connectors, off by default.
- **No score files in the repo** — every test builds its documents in code
  with `tabslib::fixtures`.

## Status

**0.1.1.** Everything above is implemented and covered by more than
500 tests, clippy runs with warnings denied, and every public item is documented.
The API may still change before 1.0, and the crate is not yet on crates.io.
See the [roadmap](ROADMAP.md) for what comes next.

| area | state |
| --- | --- |
| GP7/8 codec | complete for the modelled content; lossless round trip |
| MIDI, MusicXML | read and write |
| ASCII, HTML, PDF, WAV | write |
| Inspection and edits | complete |
| Generation and assessment | complete, API may change |
| AI connectors | complete, off by default |
| GP not yet modelled | stylesheets, extended properties, lyrics, chord diagrams, automations |

## Install

As a library (Rust 1.88+):

```sh
cargo add tabslib --git https://github.com/ansromanov/tabslib-rs
# pick formats and features
cargo add tabslib --git https://github.com/ansromanov/tabslib-rs --features midi,musicxml,ai-anthropic
```

The `tabs` command-line tool (`info`, `tab`, `diff`, `transpose`, `retune`, `slice`, `append`, `section`, `silence`, `drop-track`, `midi`):

```sh
cargo install --git https://github.com/ansromanov/tabslib-rs tabs
tabs tab song.gp --start 3 --end 6
```

See [installation](https://ansromanov.github.io/tabslib-rs/installation.html)
for the full feature table.

## Documentation

- [The tabslib book](https://ansromanov.github.io/tabslib-rs/) — quick start,
  formats, the application API, generation, AI connectors, architecture
- [API reference](https://ansromanov.github.io/tabslib-rs/api/tabslib/) — rustdoc
  for every public item
- [Examples](examples/) — runnable with `cargo run --example <name>`

## Development

```sh
just check     # fmt, clippy, tests, docs, the no-format core build
just test      # tests only
just doc-open  # build and open the API docs
```

Without `just`, see [CONTRIBUTING.md](CONTRIBUTING.md) for the plain `cargo`
commands. CI runs the same gate on every pull request.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the
workflow and [CONTENT-POLICY.md](CONTENT-POLICY.md) before adding any file: no
third-party musical content, ever.

## Trademarks

Guitar Pro is a trademark of Arobas Music. This project is not affiliated with,
endorsed by, or derived from Arobas Music or its software. Format names are used
only to describe what this library can read and write.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
