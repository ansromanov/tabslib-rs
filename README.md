# tabslib

[![CI](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-github.io-blue)](https://ansromanov.github.io/tabslib-rs/)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![MSRV](https://img.shields.io/badge/rust-1.85%2B-orange)

A general-purpose tablature library for Rust.

An owned, immutable document model for fretted-instrument notation, codecs for
the formats that carry it, and deterministic edits over both.

Written from published format behaviour. No dependency on any vendor library.

What it is *not*: it does not decide what to play. Generation, arrangement,
style modelling and quality scoring are out of scope by design — see
[Scope](#scope).

## Formats

Formats are adapters behind a trait; the core knows nothing about files.
Reading and writing are separate capabilities, because a rendering is a view of
a score rather than a container for one.

| adapter | reads | writes | round-trips | feature |
| --- | :-: | :-: | :-: | --- |
| `gp` — version 7 and 8 containers | yes | yes | yes | `gp` *(default)* |
| `ascii` — plain-text tablature | no | yes | no | `ascii` |
| `html` | no | planned | no | `html` |
| `pdf` | no | planned | no | `pdf` |
| `midi` | yes | yes | yes | `midi` |
| `musicxml` | yes | yes | yes | `musicxml` |
| `wav` — PCM rendering | no | yes | no | `wav` |

`cargo build --no-default-features` gives you the model, inspection and edits
with no format code and `thiserror` as the only dependency. CI enforces it.

See [docs/architecture.md](docs/architecture.md).

## Status — 0.1

Reading and writing are covered by deterministic model and codec tests. The GP
adapter retains unmodelled container data, and the interchange/rendering
adapters are feature-gated.

| | |
| --- | --- |
| container | zip read and write, deterministic (fixed timestamp) |
| read | tracks, tunings, colours, master bars, sections, time signatures, clefs, bars, voices, beats, rhythms, notes, techniques |
| techniques | palm mute, dead, tapped, hammer-on/pull-off, let-ring, vibrato, slide (with flags), bend (with values), harmonics (with type and fret) |
| write | the above, in the shapes the originating application emits |
| **not yet** | stylesheets, extended properties, lyrics, chord diagrams, automations, older container versions, HTML/PDF renderers |

Unknown GP container entries and unmodelled GPIF content are retained where the
adapter can patch the edited model in place; structural edits fall back to
deterministic regeneration.

## Usage

```rust
use tabslib::{format::gp::Gp, ReadFormat, WriteFormat};

let bytes = std::fs::read("song.gp")?;

// whichever compiled-in adapter recognises the bytes
let doc = tabslib::load(&bytes)?;
// or name the format, for a precise error when it is malformed
let doc = Gp::read(&bytes)?;

println!("{} — {} notes, {} sections", doc.title, doc.note_count(), doc.section_count());

let out = Gp::write(&doc)?;
```

Adding a format means implementing one trait and touching nothing else:

```rust
impl WriteFormat for MyRendering {
    const NAME: &'static str = "myrendering";
    fn write(doc: &Document) -> Result<Vec<u8>> { /* ... */ }
}
```

## Scope

The library does **model, codecs and deterministic edits**. Before adding
anything, ask:

> Can this be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

Transposing a track can. "Add a fill where the section changes" cannot — that
needs a model of a style, and belongs in a downstream crate built on this one.

## Design notes

Three decisions worth stating, because each is a place the format punishes a
plausible-looking implementation.

**Rhythm is never a float.** Converting a written duration to a `f64` and
looking it up in a table needs a fallback for values that miss, and that
fallback silently rewrites the music — a missed lookup turns a triplet into a
quarter note while the note count stays correct, so nothing downstream notices.
`Rhythm` keeps the written value, the augmentation dots and the tuplet
separately, and `as_fraction()` is exact integer arithmetic.

**Several techniques live in attributes, not element names.** The XML payload
writes `<Property name="PalmMuted">`. A reader that matches on element names
sees `Property` and finds nothing, so a file full of palm mutes reads as having
none. `Element::property()` exists so that lookup is explicit.

**Rhythms are referenced, not inlined.** The payload writes `<Rhythm ref="N"/>`
against a keyed `<Rhythm id="N">` table, and spells sixteenths `16th`, not
`Sixteenth`. Writing either differently produces a file that neither the
originating application nor a correct reader resolves.

Nodes reference each other by id. There are no `parent`, `next` or `previous`
links, so ownership stays a tree and edits stay value-shaped.

## Testing

```sh
cargo test
cargo run --release --example bench -- song.gp
```

No files are needed and none are committed. `tabslib::fixtures` generates test
documents in code from generic instrumental material — every note value plain,
dotted, triplet and quintuplet; repeated frets; a two-octave scale run; power
chords; one bar per technique; standard drum patterns (straight beat, double
kick, blast, fill); six time signatures with section markers; a bass line.

The tests assert **invariants**: save, reload, and check that the duration
histogram, the technique census and every string/fret position are unchanged.

**Rule for new tests:** state what the test would report if the feature under
test were silently dropped. If the answer is "pass", the test is wrong. Note
count, title and track count all stay correct while rhythm and articulation are
being destroyed, so none of them is sufficient on its own.

These tests are mutation-checked. Removing tuplet output from the writer — the
exact shape of a real defect, where note count stays correct and every triplet
becomes a quarter — fails two of them.

`fixtures` is public, so downstream crates can use the same documents.

## Contributing

Conventions — architecture, Rust guidelines, testing layout and the PR workflow
— are in [AGENTS.md](AGENTS.md), written for both people and agents. Start with
[CONTRIBUTING.md](CONTRIBUTING.md).

`main` is protected; every change lands through a pull request that passes
format, clippy, tests, documentation and an MSRV check.

## Trademarks

Guitar Pro is a trademark of Arobas Music. This project is not affiliated with,
endorsed by, or derived from Arobas Music or its software. Format names are used
only to describe what this library can read and write.

## License

MIT
