//! The boundary is only real if a format can be added from outside the crate,
//! implementing one trait and touching nothing that already exists.
//!
//! This adapter lives entirely in the test crate. If adding it ever requires a
//! change inside `tabslib`, the boundary has leaked and this file will stop
//! compiling — which is the point of keeping it.

use tabslib::model::Document;
use tabslib::{fixtures, Result, WriteFormat};

/// A write-only adapter, the shape every rendering takes: it produces a view of
/// a score and cannot read one back.
struct Census;

impl WriteFormat for Census {
    const NAME: &'static str = "census";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        let mut out = format!("{}\n", doc.title);
        for (name, kind) in tabslib::inspect::summary(doc).tracks {
            out.push_str(&format!("{name}\t{kind:?}\n"));
        }
        out.push_str(&format!("notes\t{}\n", doc.note_count()));
        Ok(out.into_bytes())
    }
}

#[test]
fn a_write_only_adapter_can_be_added_from_outside_the_crate() {
    for (name, doc) in fixtures::all() {
        let bytes = Census::write(&doc).expect("write");
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(text.starts_with(&doc.title), "{name}: title missing");
        assert!(
            text.contains(&format!("notes\t{}", doc.note_count())),
            "{name}: note count missing"
        );
    }
}

/// A rendering implements `WriteFormat` alone, so the round-trip invariant does
/// not apply to it and cannot be written by accident. That is enforced by the
/// type system rather than by a comment: `Census` has no `read`, so
/// `tabslib::load` on its output does not compile as a round-trip and the
/// bytes are correctly rejected at run time.
#[test]
fn a_rendering_is_not_mistaken_for_a_container() {
    let bytes = Census::write(&fixtures::scale_run()).expect("write");
    assert!(
        tabslib::load(&bytes).is_err(),
        "a rendering must not be readable back as a document"
    );
}
