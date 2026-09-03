//! The `.gp` adapter: a zip container carrying an XML payload.
//!
//! Reads and writes, so it is the one adapter the round-trip invariants apply
//! to in full. Everything format-specific lives under this module — the
//! container layout, the XML tree, the note-value spellings — so the core model
//! never learns what a file looks like.

mod container;
mod dom;
mod note_value;
mod parse;
mod write;

pub use dom::{parse_xml, Element};

use crate::error::Result;
use crate::format::{ReadFormat, WriteFormat};
use crate::model::{Document, SourceState};

/// The `.gp` container format, versions 7 and 8.
#[derive(Debug, Clone, Copy, Default)]
pub struct Gp;

impl ReadFormat for Gp {
    const NAME: &'static str = "gp";

    /// A zip archive whose payload names a score. Cheap enough to try on any
    /// input, and wrong only for an archive that coincidentally holds a file
    /// with that name.
    fn detect(bytes: &[u8]) -> bool {
        bytes.starts_with(b"PK\x03\x04") && container::read_payload(bytes).is_ok()
    }

    fn read(bytes: &[u8]) -> Result<Document> {
        let (payload, entries) = container::read_source(bytes)?;
        let mut doc = parse::parse(&payload)?;
        doc.source = Some(SourceState {
            container: bytes.to_vec(),
            baseline: write::write(&doc),
            payload,
            entries,
        });
        Ok(doc)
    }
}

impl WriteFormat for Gp {
    const NAME: &'static str = "gp";

    fn write(doc: &Document) -> Result<Vec<u8>> {
        let generated = write::write(doc);
        if let Some(source) = &doc.source {
            if source.baseline == generated {
                return Ok(source.container.clone());
            }
            if let Some(patched) = write::patch_source(doc, &source.payload) {
                return container::write_payload_with_entries(&patched, &source.entries);
            }
        }
        if let Some(source) = &doc.source {
            return container::write_payload_with_entries(&generated, &source.entries);
        }
        container::write_payload(&generated)
    }
}

/// Parses an XML payload directly, without the surrounding container.
pub fn parse_payload(xml: &str) -> Result<Document> {
    parse::parse(xml)
}

/// Serialises to an XML payload, without wrapping it in a container.
pub fn write_payload(doc: &Document) -> String {
    write::write(doc)
}
