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
  (read-only)   (deterministic)  (generated)
      |               |               |
      +---------------+---------------+
                      |
                   format
        ReadFormat  /   \  WriteFormat
                   /     \
          gp, midi      ascii, musicxml, wav
          (read/write)       (write)
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
| `ascii` | later | yes | no |
| `html` | no | yes | no |
| `pdf` | no | yes | no |

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
gp      = ["dep:zip", "dep:quick-xml"]
ascii   = []
midi    = []
musicxml = []
wav     = []
```

Adapters are feature-gated so their dependencies are opt-in. A consumer that
only renders ASCII should not compile a zip implementation, and a consumer that
only reads `.gp` should not compile a PDF writer.

`--no-default-features` leaves the model, inspection, edits and fixtures, with
`thiserror` as the only dependency. Every adapter module is opt-in through its
feature; `gp` additionally retains the original container and GPIF payload on
read so an unedited load/save can return it byte-for-byte. Model-owned scalar
note changes are patched into that retained payload; structural changes fall
back to deterministic regeneration.

## What is deliberately not here

Generation, arrangement, style modelling and quality scoring. They need a model
of a musical style; this crate models notation. The test is whether a capability
can be specified without naming a genre, a band, or a statistic measured over a
collection of music — and it belongs downstream when the answer is no.
