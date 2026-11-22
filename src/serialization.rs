//! Deterministic, versioned JSON encoding for the document model.

use crate::error::{Error, Result};
use crate::model::Document;
use crate::validation;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct Envelope<'a> {
    format: &'static str,
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    document: &'a Document,
}

#[derive(Deserialize)]
struct OwnedEnvelope {
    format: String,
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    document: Document,
}

/// Encodes a valid document as stable UTF-8 JSON.
pub fn encode(document: &Document) -> Result<String> {
    let issues = validation::validate(document);
    if let Some(issue) = issues.first() {
        return Err(Error::Malformed(format!(
            "{}: {}",
            issue.path, issue.message
        )));
    }
    serde_json::to_string(&Envelope {
        format: crate::NATIVE_DOCUMENT_FORMAT,
        schema_version: crate::NATIVE_DOCUMENT_SCHEMA_VERSION,
        document,
    })
    .map_err(|error| Error::Malformed(format!("document JSON encoding failed: {error}")))
}

/// Encodes a valid document as deterministic UTF-8 JSON bytes.
pub fn encode_document(document: &Document) -> Result<Vec<u8>> {
    Ok(encode(document)?.into_bytes())
}

/// Decodes and validates a versioned native JSON document.
pub fn decode(json: &str) -> Result<Document> {
    let envelope: OwnedEnvelope = serde_json::from_str(json)
        .map_err(|error| Error::Malformed(format!("invalid document JSON: {error}")))?;
    if envelope.format != crate::NATIVE_DOCUMENT_FORMAT {
        return Err(Error::Malformed(format!(
            "unsupported document format: {}",
            envelope.format
        )));
    }
    if envelope.schema_version != crate::NATIVE_DOCUMENT_SCHEMA_VERSION {
        return Err(Error::Malformed(format!(
            "unsupported document schema version: {}",
            envelope.schema_version
        )));
    }
    let issues = validation::validate(&envelope.document);
    if let Some(issue) = issues.first() {
        return Err(Error::Malformed(format!(
            "{}: {}",
            issue.path, issue.message
        )));
    }
    Ok(envelope.document)
}

/// Decodes and validates a versioned native JSON document from UTF-8 bytes.
pub fn decode_document(bytes: &[u8]) -> Result<Document> {
    let json = std::str::from_utf8(bytes)
        .map_err(|error| Error::Malformed(format!("document JSON is not UTF-8: {error}")))?;
    decode(json)
}

#[cfg(test)]
#[path = "serialization_test.rs"]
mod tests;
