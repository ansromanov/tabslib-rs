use super::*;
use crate::fixtures;

#[test]
fn document_json_is_deterministic_and_round_trips() {
    let document = fixtures::repeated_frets();
    let first = encode(&document).expect("fixture encodes");
    assert_eq!(first, encode(&document).expect("fixture encodes twice"));
    assert_eq!(decode(&first).expect("fixture decodes"), document);
    assert_eq!(
        decode_document(&encode_document(&document).expect("fixture bytes"))
            .expect("fixture bytes decode"),
        document
    );
    assert!(first.contains("\"schemaVersion\":1"));
}

#[test]
fn document_json_rejects_an_unknown_schema() {
    let json = encode(&fixtures::repeated_frets())
        .expect("fixture encodes")
        .replace("\"schemaVersion\":1", "\"schemaVersion\":2");
    assert!(decode(&json).is_err());
}
