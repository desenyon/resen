use resen::{
    config::{Config, Secrets},
    domain::ProviderKind,
    provider::{self, ModelEvent, SseDecoder, cli_text, parse_sse},
};
use tokio::sync::mpsc;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

#[test]
fn sse_decoder_survives_every_utf8_chunk_boundary() {
    let data =
        "data: {\"choices\":[{\"delta\":{\"content\":\"研究€\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
    for split in 0..data.len() {
        let mut decoder = SseDecoder::default();
        let mut frames = decoder.push(&data.as_bytes()[..split]).unwrap();
        frames.extend(decoder.push(&data.as_bytes()[split..]).unwrap());
        assert_eq!(frames.len(), 2, "split {split}");
        assert_eq!(
            parse_sse(&frames[0], false).unwrap().0.as_deref(),
            Some("研究€")
        );
        assert!(parse_sse(&frames[1], false).unwrap().1);
    }
}
#[test]
fn decoder_handles_comments_multiple_data_lines_and_limits() {
    let mut decoder = SseDecoder::default();
    assert!(decoder.push(b": ping\n\n").unwrap().is_empty());
    assert_eq!(
        decoder.push(b"data: first\ndata: second\n\n").unwrap(),
        ["first\nsecond"]
    );
    assert!(decoder.push(&vec![b'a'; 1_000_001]).is_err());
}
#[test]
fn parse_rejects_errors_and_marks_truncation() {
    assert!(parse_sse("{\"error\":{\"message\":\"bad\"}}", false).is_err());
    assert!(parse_sse("not JSON", false).is_err());
    assert!(
        parse_sse("{\"choices\":[{\"finish_reason\":\"length\"}]}", false)
            .unwrap()
            .2
    );
    assert!(parse_sse("{\"type\":\"message_stop\"}", true).unwrap().1);
}
#[test]
fn decoder_preserves_frame_order_with_mixed_line_endings() {
    let mut decoder = SseDecoder::default();
    assert_eq!(
        decoder
            .push(b"data: first\r\n\r\ndata: second\n\ndata: third\r\n\r\n")
            .unwrap(),
        ["first", "second", "third"]
    );
}
#[test]
fn cli_protocols_accept_only_final_assistant_text() {
    assert_eq!(cli_text(&serde_json::json!({"type":"item.completed","item":{"type":"agent_message","text":"memo"}}),true).unwrap(),"memo\n");
    assert!(cli_text(&serde_json::json!({"type":"item.completed","item":{"type":"command_execution","text":"secret command"}}),true).is_none());
    assert_eq!(cli_text(&serde_json::json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"memo"}]}}),false).unwrap(),"memo");
}

#[tokio::test]
async fn compatible_stream_contract_and_connection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data":[{"id":"fixture-model"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/v1/chat/completions")).and(header("authorization","Bearer fixture-token")).and(body_partial_json(serde_json::json!({"model":"fixture-model","stream":true,"max_tokens":4096,"messages":[{"role":"system","content":"system"},{"role":"user","content":"question"}]}))).respond_with(ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string("data: {\"choices\":[{\"delta\":{\"content\":\"Evidence [1]\"}}]}\n\ndata: [DONE]\n\n")).expect(1).mount(&server).await;
    let config = Config {
        provider: ProviderKind::Compatible,
        endpoint: format!("{}/v1", server.uri()),
        model: "fixture-model".into(),
        ..Config::default()
    };
    let mut secrets = Secrets::default();
    secrets.set("RESEN_MODEL_API_KEY", "fixture-token".into());
    assert!(
        provider::check(&config, &secrets)
            .await
            .unwrap()
            .contains("available")
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    provider::generate(
        &config,
        &secrets,
        "system",
        "question",
        std::path::Path::new("."),
        &tx,
    )
    .await
    .unwrap();
    assert!(matches!(rx.try_recv().unwrap(),ModelEvent::Delta(s)if s=="Evidence [1]"));
}
#[tokio::test]
async fn anthropic_request_and_stream_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/messages")).and(header("x-api-key","anthropic-fixture-token")).and(header("anthropic-version","2023-06-01")).and(body_partial_json(serde_json::json!({"model":"test","system":"system","max_tokens":4096}))).respond_with(ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string("event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Memo\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n")).expect(1).mount(&server).await;
    let config = Config {
        provider: ProviderKind::Anthropic,
        endpoint: server.uri(),
        model: "test".into(),
        ..Config::default()
    };
    let mut secrets = Secrets::default();
    secrets.set("ANTHROPIC_API_KEY", "anthropic-fixture-token".into());
    let (tx, mut rx) = mpsc::unbounded_channel();
    provider::generate(
        &config,
        &secrets,
        "system",
        "question",
        std::path::Path::new("."),
        &tx,
    )
    .await
    .unwrap();
    assert!(matches!(rx.try_recv().unwrap(),ModelEvent::Delta(s)if s=="Memo"));
}
#[tokio::test]
async fn incomplete_stream_preserves_delta_and_fails() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"choices\":[{\"delta\":{\"content\":\"Partial\"}}]}\n\n"),
        )
        .mount(&server)
        .await;
    let config = Config {
        provider: ProviderKind::Compatible,
        endpoint: server.uri(),
        model: "test".into(),
        ..Config::default()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let result = provider::generate(
        &config,
        &Secrets::default(),
        "system",
        "question",
        std::path::Path::new("."),
        &tx,
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("before its completion")
    );
    assert!(matches!(rx.try_recv().unwrap(),ModelEvent::Delta(s)if s=="Partial"));
}
#[tokio::test]
async fn nonstreaming_compatible_response_preserves_output_and_truncation_warning() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"choices":[{"message":{"content":"Partial fixture memo [1]"},"finish_reason":"length"}]}))).expect(1).mount(&server).await;
    let config = Config {
        provider: ProviderKind::Compatible,
        endpoint: server.uri(),
        model: "fixture".into(),
        ..Config::default()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    provider::generate(
        &config,
        &Secrets::default(),
        "system",
        "question",
        std::path::Path::new("."),
        &tx,
    )
    .await
    .unwrap();
    assert!(matches!(rx.try_recv().unwrap(),ModelEvent::Delta(s) if s=="Partial fixture memo [1]"));
    assert!(matches!(rx.try_recv().unwrap(),ModelEvent::Warning(s) if s.contains("incomplete")));
}
#[tokio::test]
async fn model_http_errors_do_not_expose_provider_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string("secret key diagnostic must not leak"),
        )
        .mount(&server)
        .await;
    let config = Config {
        provider: ProviderKind::Compatible,
        endpoint: server.uri(),
        model: "test".into(),
        ..Config::default()
    };
    let (tx, _rx) = mpsc::unbounded_channel();
    let error = provider::generate(
        &config,
        &Secrets::default(),
        "system",
        "question",
        std::path::Path::new("."),
        &tx,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("401"));
    assert!(!error.contains("secret key"));
}
