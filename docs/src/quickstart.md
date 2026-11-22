# Quick start

Add the crate (see [Installation](installation.md)), then:

```rust,ignore
{{#include ../../examples/quickstart.rs:11:}}
```

Run it from a checkout with `cargo run --example quickstart`, or pass a file:
`cargo run --example quickstart -- song.gp`.

## What happened

1. **Load.** `tabslib::load` picks whichever compiled-in adapter recognises the
   bytes. Name the format (`Gp::read`) when you want a precise error for a
   malformed file. `tabslib::fixtures` builds documents in code, which is how
   every test in the crate works without score files.
2. **Inspect.** `facade::inspect` answers read-only questions: statistics,
   key and mode, chords, bar integrity.
3. **Edit.** Edits change the document in place and refuse to produce an
   unplayable note or an over-full bar.
4. **Render and write.** Renderers (`ascii`, `html`, `pdf`, `wav`) are
   write-only views. Containers (`gp`, `midi`, `musicxml`) read and write, and
   the GP round trip is lossless.

## More examples

| example | shows |
| --- | --- |
| [`native_generation`](https://github.com/ansromanov/tabslib-rs/blob/main/examples/native_generation.rs) | inspection, edits, seeded drum and bass generation, harmony, arrangement and assessment |
| [`export_track`](https://github.com/ansromanov/tabslib-rs/blob/main/examples/export_track.rs) | one track as JSON notes in MIDI ticks, repeats expanded |
| [`export_drums`](https://github.com/ansromanov/tabslib-rs/blob/main/examples/export_drums.rs) | the drum track as JSON notes in MIDI ticks, for a DAW |
| [`restyle`](https://github.com/ansromanov/tabslib-rs/blob/main/examples/restyle.rs) | batch presentation edits: fader levels, zoom, tuning label in the file name |
| [`bench`](https://github.com/ansromanov/tabslib-rs/blob/main/examples/bench.rs) | load, save and walk timing for one file |
