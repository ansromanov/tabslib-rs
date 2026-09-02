# tabslib

A Guitar Pro score engine in Rust: GP7/8 container and GPIF codec over an
owned, immutable document model.

Written from the file format itself. No dependency on any other Guitar Pro
library.

## Status — 0.1, codec only

Reading is solid on real files. Writing is correct for the modelled subset and
**not yet lossless for a whole file**.

| | |
| --- | --- |
| container | GP7/8 zip read and write, deterministic (fixed timestamp) |
| read | tracks, tunings, colours, master bars, sections, time signatures, clefs, bars, voices, beats, rhythms, notes, techniques |
| techniques | palm mute, dead, tapped, hammer-on/pull-off, let-ring, vibrato, slide (with flags), bend (with values), harmonics (with type and fret) |
| write | the above, in the shapes Guitar Pro itself emits |
| **not yet** | RSE / mixer state, stylesheets, XProperties, lyrics, chord diagrams, automations, repeats, alternate endings, tuplet output, MIDI and MusicXML codecs, legacy `.gp3/.gp4/.gp5` |

Because RSE and several container-level blocks are not modelled, a save
produces a smaller file than the source. Use it to read, and to round-trip the
modelled subset; do not use it to rewrite a finished arrangement yet.

## Usage

```rust
let bytes = std::fs::read("song.gp")?;
let doc = tabslib::load(&bytes)?;

println!("{} — {} notes, {} sections", doc.title, doc.note_count(), doc.section_count());

let out = tabslib::save(&doc)?;
```

## Design notes

Three decisions worth stating, because each is a place the format punishes a
plausible-looking implementation.

**Rhythm is never a float.** Converting a written duration to a `f64` and
looking it up in a table needs a fallback for values that miss, and that
fallback silently rewrites the music — a missed lookup turns a triplet into a
quarter note while the note count stays correct, so nothing downstream notices.
`Rhythm` keeps the written value, the augmentation dots and the tuplet
separately, and `as_fraction()` is exact integer arithmetic.

**Several techniques live in attributes, not element names.** GPIF writes
`<Property name="PalmMuted">`. A reader that matches on element names sees
`Property` and finds nothing, so a file full of palm mutes reads as having
none. `Element::property()` exists so that lookup is explicit.

**Rhythms are referenced, not inlined.** Guitar Pro writes `<Rhythm ref="N"/>`
against a keyed `<Rhythm id="N">` table, and spells sixteenths `16th`, not
`Sixteenth`. Writing either differently produces a file that neither Guitar Pro
nor a correct reader resolves.

Nodes reference each other by id. There are no `parent`, `next` or `previous`
links, so ownership stays a tree and edits stay value-shaped.

## Testing

```sh
cargo test                                   # unit tests
TABSLIB_CORPUS=/path/to/gp/files cargo test  # plus round-trip tests over real files
cargo run --release --example bench -- song.gp
```

The corpus tests walk whatever `.gp` files they find and assert **invariants**
— that a load/save/reload changes nothing measurable — rather than hardcoded
numbers, so they work against any collection. They skip when `TABSLIB_CORPUS`
is unset, and no Guitar Pro files are committed to this repository.

**Rule for new tests:** state what the test would report if the feature under
test were silently dropped. If the answer is "pass", the test is wrong. Note
count, title and track count all stay correct while rhythm and articulation are
being destroyed, so none of them is sufficient on its own.

## License

MIT
