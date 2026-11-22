use super::*;
use serde_json::json;

#[test]
fn drum_response_rejects_unknown_categories() {
    let document = crate::fixtures::repeated_frets();
    let request = capture_drum_critique_request("test", "score", &document, 0)
        .expect("valid structured request");
    let response = json!({"schema_version": 1, "findings": [{
        "category": "unknown", "severity": "warning", "section": 0, "detail": "x"
    }]});
    assert!(parse_drum_critique_response(&request, &response.to_string()).is_err());
}
