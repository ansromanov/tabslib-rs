# Formats

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

See [Architecture](architecture.md).

## GP adapter coverage

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

## Reading and writing

```rust,ignore
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

```rust,ignore
impl WriteFormat for MyRendering {
    const NAME: &'static str = "myrendering";
    fn write(doc: &Document) -> Result<Vec<u8>> { /* ... */ }
}
```
