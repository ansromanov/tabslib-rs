# tabslib

[![CI](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ansromanov/tabslib-rs/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
![MSRV](https://img.shields.io/badge/rust-1.85%2B-orange)

A general-purpose tablature and deterministic music-generation library for Rust.

An owned, immutable document model for fretted-instrument notation, codecs for
the formats that carry it, and deterministic edits over both.

Written from published format behaviour. No dependency on any vendor library.

Generation, arrangement, and assessment capabilities operate on the same
native `Document` model. Use the stable [`facade`](src/facade.rs) entry point
for application workflows; style data is supplied by the caller.

See [the native API guide](docs/native-api.md) for the facade, capability
statuses, and examples.

## Formats

Formats are adapters behind a trait; the core knows nothing about files.
Reading and writing are separate capabilities, because a rendering is a view of
a score rather than a container for one.

| adapter | reads | writes | round-trips | feature |
| --- | :-: | :-: | :-: | --- |
| `gp` — version 7 and 8 containers | yes | yes | yes | `gp` *(default)* |
| `ascii` — plain-text tablature | no | yes | no | `ascii` |
| `html` — standalone responsive tablature | no | yes | no | `html` |
| `pdf` — deterministic print tablature | no | yes | no | `pdf` |
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
| **not yet** | stylesheets, extended properties, lyrics, chord diagrams, automations, older container versions |

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

The library owns the score model, codecs, deterministic edits, descriptive
inspection, and style-neutral generation, arrangement, and assessment. It does
not embed a genre, a style identity, reference data, or provider-specific AI
policy. Keep those decisions in the calling application and supply calibration
data as an external input.

## AI connectors

AI support is optional and off by default. The `ai` feature adds prompt
construction over structured assessment facts, response validation, blind
candidate comparison and the `RationaleProvider` trait; it makes no network
calls. Each connector is a separate feature on top of it:

| feature | service | API key (when not set on the config) |
| --- | --- | --- |
| `ai-anthropic` | Anthropic Messages API, default model `claude-sonnet-5-5` | `ANTHROPIC_API_KEY` |
| `ai-openai-compatible` | any OpenAI-compatible Chat Completions endpoint | `OPENAI_API_KEY`, optional |
| `ai-gemini` | Google Gemini | `GEMINI_API_KEY` |
| `ai-ollama` | an Ollama server | none |

Configure any of them through one type, `AiConfig`:

```rust,ignore
use tabslib::generation::ai::{generate_rationale, AiConfig, AiConnector, PromptInput, ProviderOptions};
use tabslib::generation::assessment::assess;

let provider = AiConfig::new(AiConnector::Anthropic)
    .with_options(ProviderOptions::default().with_max_retries(2))
    .build()?;

let report = assess(&document);
let rationale = generate_rationale(&provider, PromptInput::rationale("draft", &report))?;
```

`with_model`, `with_endpoint`, `with_api_key` and `with_max_tokens` override
the connector's defaults. The connectors are also public types
(`AnthropicProvider`, `OpenAiCompatibleProvider`, `GeminiProvider`,
`OllamaProvider`) for callers who prefer to build them directly, and any type
implementing `RationaleProvider` can stand in for them. Calls are synchronous;
run them on a blocking worker from async code. Retries are off by default.
The connectors need Rust 1.88 or newer; everything else builds on 1.85.

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

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CONTENT-POLICY.md](CONTENT-POLICY.md).
Every change lands through a pull request that passes format, clippy, tests and
documentation.

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
