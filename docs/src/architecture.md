# Architecture

## The shape

```
                    model
         (Document, Rhythm, Technique)
          knows nothing about files
                      |
      +---------------+---------------+
      |               |               |
   inspect          edits          fixtures
  (analysis)    (deterministic)  (generated)
      |               |               |
      +---------------+---------------+
                      |
                   format
        ReadFormat  /   \  WriteFormat
                   /     \
          gp, midi, musicxml  ascii, html, pdf, wav
             (read/write)     (write)
```

One direction of dependency: adapters depend on the model, never the reverse.
Nothing in `model`, `inspect`, `edits` or `fixtures` names a file format, and
CI proves it by building with `--no-default-features` and failing if a format
library appears in the dependency graph.

## Why two traits

`ReadFormat` and `WriteFormat` are separate because reading and writing are
separate capabilities.

| adapter | reads | writes | round-trips |
| --- | :-: | :-: | :-: |
| `gp` | yes | yes | **yes** |
| `midi` | yes | yes | **yes** |
| `musicxml` | yes | yes | **yes** |
| `ascii`, `html`, `pdf` | no | yes | no |
| `wav` | no | yes | no |

A rendering is lossy by construction: an ASCII tab or a PDF is a *view* of a
score, not a container for one. Under one combined trait each rendering would
have to supply a `read` that fails at run time — an unimplementable method,
which is worse than an absent one — and the round-trip test suite would invite
a case for PDF that can never pass.

Split, the invariant has an exact meaning: **the round-trip tests apply to every
adapter implementing both traits, and to nothing else.**

## Where format knowledge is allowed to live

Only inside an adapter. Three things moved out of the core to make that true:

- `NoteValue::as_gpif()` was a serialiser on the model. The model described
  music and also knew how one particular file spells a sixteenth. It is now
  `format/gp/note_value.rs`, private to the adapter.
- `Error::NotGp7`, `Error::Zip` and `Error::Xml` put one format's failures in
  the crate-wide error type. Adapters now report through
  `Error::Format { format, source }`, boxed, so enabling or disabling an
  adapter cannot change the shape of `Error`.
- `load`/`save` in `lib.rs` called the `gp` codec directly. `load` now asks
  each compiled-in reader whether it recognises the bytes.

The asymmetry between reading and writing belongs to the adapter too. The `gp`
reader accepts `16th` and `Sixteenth`; the writer emits only `16th`, because
that is what the originating application produces. **Parse defensively, write
exactly** — and both halves of that rule are the adapter's business.

## Adding an adapter

Implement one trait. Touch nothing that already exists.

```rust
use tabslib::{model::Document, Result, WriteFormat};

struct MyFormat;

impl WriteFormat for MyFormat {
    const NAME: &'static str = "myformat";
    fn write(doc: &Document) -> Result<Vec<u8>> { /* ... */ }
}
```

`tests/adapter_boundary.rs` does exactly this from the test crate, and exists so
that if adding an adapter ever requires reaching inside `tabslib`, the boundary
has leaked and that file stops compiling.

An adapter that can also read implements `ReadFormat` as well, at which point
the round-trip invariants automatically apply to it.

## Features

```toml
default = ["gp", "ascii"]
gp       = ["dep:zip", "dep:quick-xml"]
ascii    = []
html     = []
pdf      = []
midi     = []
musicxml = ["dep:quick-xml"]
wav      = ["dep:rustysynth"]
```

Adapters are feature-gated so their dependencies are opt-in. A consumer that
only renders ASCII should not compile a zip implementation, and a consumer that
only reads `.gp` should not compile a PDF writer.

`--no-default-features` leaves the model, inspection, edits and fixtures, with
`thiserror` as the only dependency. Every adapter module is opt-in through its
feature. The `gp` adapter retains the original container entries and GPIF
payload on read so an unedited load/save can return it byte-for-byte;
model-owned scalar note changes are patched into that retained payload, while
structural changes fall back to deterministic regeneration. The MIDI and
MusicXML adapters provide read/write conversion; `ascii`, `html`, `pdf` and
`wav` are write-only views.

The optional `wav` feature uses a pure-Rust SoundFont2 synthesizer for offline
stereo PCM rendering. `format::wav::compile_performance_events` exposes the
sample-positioned events before synthesis, including tempo changes and repeat
playback order; rendering reads the caller's `.sf2` file only when requested.

The model also carries format-neutral mixer state, ties, repeat navigation,
alternate endings, and track-local percussion articulation mappings. GPIF and
MusicXML adapters map their respective representations into those fields;
playback-oriented inspection and MIDI export use repeat order when it is
present, and percussion inspection/export resolves raw track-local IDs through
their observed MIDI pitches and broad kit roles.

`inspect` may expose general key/mode determination and descriptive score
statistics. It reports evidence about an existing document; it does not choose
notes, apply a style profile, calibrate against reference data, or invoke AI.
General music theory is allowed; style-specific composition rules belong to the
caller.

Edit guards compare each resulting voice with its `(track, bar, voice)` state
before the edit. Existing over-full voices are preserved, while a change that
increases one or creates a new one is rejected. Pitch transformations likewise
skip percussion tracks, whose `fret` field carries an instrument number rather
than a fretted-string position.

## Generation and arrangement

Generation, arrangement, assessment, and harmony modules live in
`src/generation/` and build directly on the same document model. Style data is
supplied by the caller.

Applications can use the stable `tabslib::facade` path for inspection, edits,
generation, arrangement, assessment, and optional byte-oriented format
adapters. The facade operates on `Document`; it does not restore a
format-library score model or silently route unsupported work to another
engine. See [the native API guide](api.md) for request and result types
and capability statuses.

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
