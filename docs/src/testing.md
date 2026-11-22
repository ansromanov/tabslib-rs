# Testing

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

## The local gate

```sh
just check     # fmt, clippy, tests, docs, the no-format core build
```

CI runs the same steps on every pull request, plus each AI connector on its
own, the MSRV build, and the `tabs` CLI.
