#![cfg(feature = "ai-http")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};
use tabslib::generation::ai::RationaleProvider;
use tabslib::generation::ai::{capture_request, PromptInput};
use tabslib::generation::assessment::assess;

struct MockResponse {
    status: &'static str,
    body: String,
    delay: Duration,
}

fn mock_endpoint(responses: Vec<MockResponse>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        let mut responders = Vec::new();
        for response in responses {
            let (mut request_stream, _) = listener.accept().unwrap();
            request_stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            requests.push(read_request(&mut request_stream));
            responders.push(thread::spawn(move || {
                if !response.delay.is_zero() {
                    thread::sleep(response.delay);
                }
                let body = response.body.as_bytes();
                let _ = write!(
                    request_stream,
                    "HTTP/1.1 {}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n",
                    response.status,
                    body.len()
                );
                let _ = request_stream.write_all(body);
            }));
        }
        for responder in responders {
            responder.join().unwrap();
        }
        requests
    });
    (format!("http://{address}/completion"), server)
}

fn mock_response(status: &'static str, body: impl Into<String>) -> MockResponse {
    MockResponse {
        status,
        body: body.into(),
        delay: Duration::ZERO,
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0; 2048];
    let mut expected_length = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "client closed before sending a complete request");
        bytes.extend_from_slice(&buffer[..read]);
        if expected_length.is_none() {
            if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                expected_length = Some(header_end + 4 + content_length);
            }
        }
        if expected_length.is_some_and(|length| bytes.len() >= length) {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn captured_request() -> tabslib::generation::ai::CapturedRequest {
    let report = assess(&tabslib::fixtures::repeated_frets());
    capture_request("adapter-test", PromptInput::rationale("candidate", &report)).unwrap()
}

fn request_parts(request: &str) -> (&str, Value) {
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    (headers, serde_json::from_str(body).unwrap())
}

fn rationale_response(text: &str) -> String {
    json!({"rationale": text, "findings": []}).to_string()
}

#[cfg(feature = "ai-openai-compatible")]
#[test]
fn openai_compatible_adapter_sends_structured_chat_request() {
    use tabslib::generation::ai::OpenAiCompatibleProvider;

    let response = json!({
        "choices": [{ "message": { "content": rationale_response("OpenAI-compatible result") } }]
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let provider = OpenAiCompatibleProvider::new(endpoint, "test-model")
        .unwrap()
        .with_api_key("test-secret")
        .unwrap();
    let captured = captured_request();

    assert_eq!(provider.name(), "openai-compatible");
    assert_eq!(
        provider.complete(&captured).unwrap(),
        rationale_response("OpenAI-compatible result")
    );
    let requests = server.join().unwrap();
    let (headers, body) = request_parts(&requests[0]);
    assert!(headers
        .to_ascii_lowercase()
        .contains("authorization: bearer test-secret"));
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["response_format"]["type"], "json_object");
    assert_eq!(body["messages"][0]["content"], captured.prompt());
}

#[cfg(feature = "ai-ollama")]
#[test]
fn ollama_adapter_uses_native_non_streaming_json_chat() {
    use tabslib::generation::ai::OllamaProvider;

    let response = json!({
        "message": { "role": "assistant", "content": rationale_response("Ollama result") },
        "done": true
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let provider = OllamaProvider::new("test-model")
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap();
    let captured = captured_request();

    assert_eq!(provider.name(), "ollama");
    assert_eq!(
        provider.complete(&captured).unwrap(),
        rationale_response("Ollama result")
    );
    let requests = server.join().unwrap();
    let (_, body) = request_parts(&requests[0]);
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["format"], "json");
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"][0]["content"], captured.prompt());
}

#[cfg(feature = "ai-gemini")]
#[test]
fn gemini_adapter_uses_api_key_header_and_extracts_candidate_text() {
    use tabslib::generation::ai::GeminiProvider;

    let response = json!({
        "candidates": [{
            "content": {
                "parts": [
                    { "text": "{\"rationale\":\"Gemini " },
                    { "text": "result\",\"findings\":[]}" }
                ]
            }
        }]
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let provider = GeminiProvider::new("test-secret", "test-model")
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap();
    let captured = captured_request();

    assert_eq!(provider.name(), "gemini");
    let actual: Value = serde_json::from_str(&provider.complete(&captured).unwrap()).unwrap();
    let expected: Value = serde_json::from_str(&rationale_response("Gemini result")).unwrap();
    assert_eq!(actual, expected);
    let requests = server.join().unwrap();
    let (headers, body) = request_parts(&requests[0]);
    assert!(headers
        .to_ascii_lowercase()
        .contains("x-goog-api-key: test-secret"));
    assert_eq!(
        body["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert_eq!(body["contents"][0]["parts"][0]["text"], captured.prompt());
}

#[cfg(feature = "ai-openai-compatible")]
#[test]
fn openai_compatible_adapter_rejects_malformed_provider_responses() {
    use tabslib::generation::ai::OpenAiCompatibleProvider;

    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", r#"{"choices":[]}"#)]);
    let provider = OpenAiCompatibleProvider::new(endpoint, "test-model").unwrap();
    let error = provider.complete(&captured_request()).unwrap_err();

    assert!(error.contains("choices[0].message.content"));
    server.join().unwrap();
}

#[cfg(feature = "ai-openai-compatible")]
#[test]
fn openai_compatible_adapter_retries_transient_server_responses() {
    use tabslib::generation::ai::{OpenAiCompatibleProvider, ProviderOptions};

    let success = json!({
        "choices": [{
            "message": { "content": rationale_response("retry result") }
        }]
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![
        mock_response("503 Service Unavailable", "temporary failure"),
        mock_response("200 OK", success),
    ]);
    let options = ProviderOptions::default()
        .with_max_retries(1)
        .with_retry_delay(Duration::ZERO);
    let provider = OpenAiCompatibleProvider::new(endpoint, "test-model")
        .unwrap()
        .with_options(options)
        .unwrap();

    assert_eq!(
        provider.complete(&captured_request()).unwrap(),
        rationale_response("retry result")
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[cfg(feature = "ai-ollama")]
#[test]
fn ollama_provider_retries_transient_responses_and_records_request_count() {
    use tabslib::generation::ai::{OllamaProvider, ProviderOptions};

    let first_response = mock_response("503 Service Unavailable", "temporary failure");
    let second_response = mock_response(
        "200 OK",
        json!({
            "message": { "content": rationale_response("retry result") }
        })
        .to_string(),
    );
    let (endpoint, server) = mock_endpoint(vec![first_response, second_response]);
    let options = ProviderOptions::default()
        .with_max_retries(1)
        .with_retry_delay(Duration::ZERO);
    let provider = OllamaProvider::new("test-model")
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap()
        .with_options(options)
        .unwrap();

    assert_eq!(
        provider.complete(&captured_request()).unwrap(),
        rationale_response("retry result")
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[cfg(feature = "ai-ollama")]
#[test]
fn ollama_provider_does_not_retry_auth_or_not_found_failures() {
    use tabslib::generation::ai::{OllamaProvider, ProviderOptions};

    for (status, expected_reason) in [("401 Unauthorized", "auth"), ("404 Not Found", "not_found")]
    {
        let (endpoint, server) = mock_endpoint(vec![mock_response(status, "failure")]);
        let options = ProviderOptions::default()
            .with_max_retries(3)
            .with_retry_delay(Duration::ZERO);
        let provider = OllamaProvider::new("test-model")
            .unwrap()
            .with_endpoint(endpoint)
            .unwrap()
            .with_options(options)
            .unwrap();
        let error = provider.complete(&captured_request()).unwrap_err();
        assert!(error.starts_with(expected_reason), "{error}");
        assert_eq!(
            server.join().unwrap().len(),
            1,
            "{status} must fail immediately"
        );
    }
}

#[cfg(feature = "ai-gemini")]
#[test]
fn gemini_adapter_reports_http_failure_without_provider_body() {
    use tabslib::generation::ai::GeminiProvider;

    let (endpoint, server) = mock_endpoint(vec![mock_response(
        "429 Too Many Requests",
        "sensitive provider details",
    )]);
    let provider = GeminiProvider::new("test-secret", "test-model")
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap();
    let error = provider.complete(&captured_request()).unwrap_err();

    assert!(error.contains("HTTP 429"));
    assert!(!error.contains("sensitive provider details"));
    server.join().unwrap();
}

#[cfg(feature = "ai-anthropic")]
#[test]
fn anthropic_connector_sends_messages_request_and_joins_text_blocks() {
    use tabslib::generation::ai::{AnthropicProvider, ANTHROPIC_DEFAULT_MODEL};

    let response = json!({
        "stop_reason": "end_turn",
        "content": [
            { "type": "thinking", "thinking": "" },
            { "type": "text", "text": "{\"rationale\":\"Anthropic " },
            { "type": "text", "text": "result\",\"findings\":[]}" }
        ]
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let provider = AnthropicProvider::new("test-secret", ANTHROPIC_DEFAULT_MODEL)
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap()
        .with_max_tokens(512);
    let captured = captured_request();

    assert_eq!(provider.name(), "anthropic");
    let actual: Value = serde_json::from_str(&provider.complete(&captured).unwrap()).unwrap();
    let expected: Value = serde_json::from_str(&rationale_response("Anthropic result")).unwrap();
    assert_eq!(actual, expected);
    let requests = server.join().unwrap();
    let (headers, body) = request_parts(&requests[0]);
    let headers = headers.to_ascii_lowercase();
    assert!(headers.contains("x-api-key: test-secret"));
    assert!(headers.contains("anthropic-version: "));
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert_eq!(body["max_tokens"], 512);
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], captured.prompt());
}

#[cfg(feature = "ai-anthropic")]
#[test]
fn anthropic_connector_reports_refusal_instead_of_empty_text() {
    use tabslib::generation::ai::AnthropicProvider;

    let response = json!({ "stop_reason": "refusal", "content": [] }).to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let provider = AnthropicProvider::new("test-secret", "test-model")
        .unwrap()
        .with_endpoint(endpoint)
        .unwrap();

    let error = provider.complete(&captured_request()).unwrap_err();
    assert!(error.contains("declined"), "{error}");
    server.join().unwrap();
}

#[cfg(feature = "ai-anthropic")]
#[test]
fn ai_config_builds_anthropic_connector_with_default_model() {
    use tabslib::generation::ai::{AiConfig, AiConnector};

    let response = json!({
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": rationale_response("configured") }]
    })
    .to_string();
    let (endpoint, server) = mock_endpoint(vec![mock_response("200 OK", response)]);
    let config = AiConfig::new(AiConnector::Anthropic)
        .with_api_key("config-secret")
        .with_endpoint(endpoint);
    assert!(!format!("{config:?}").contains("config-secret"));
    let provider = config.build().unwrap();

    assert_eq!(provider.name(), "anthropic");
    let report = assess(&tabslib::fixtures::repeated_frets());
    let rationale = tabslib::generation::ai::generate_rationale(
        &provider,
        PromptInput::rationale("configured", &report),
    )
    .unwrap();
    assert_eq!(rationale.text, "configured");
    let requests = server.join().unwrap();
    let (headers, body) = request_parts(&requests[0]);
    assert!(headers
        .to_ascii_lowercase()
        .contains("x-api-key: config-secret"));
    assert_eq!(body["model"], "claude-sonnet-5-5");
}

#[cfg(feature = "ai-openai-compatible")]
#[test]
fn ai_config_requires_endpoint_and_model_where_the_connector_has_no_default() {
    use tabslib::generation::ai::{AiConfig, AiConnector, ProviderConfigError};

    let missing_endpoint = AiConfig::new(AiConnector::OpenAiCompatible).with_model("m");
    assert!(matches!(
        missing_endpoint.build(),
        Err(ProviderConfigError::MissingEndpoint)
    ));
    let missing_model = AiConfig::new(AiConnector::OpenAiCompatible).with_endpoint("http://x");
    assert!(matches!(
        missing_model.build(),
        Err(ProviderConfigError::EmptyModel)
    ));
}
