use odoc_llm::{
    ChatMessage, ChatRequest, ChatRole, LlmError, LlmProvider, ProviderClient, ProviderKind,
    ProviderProfile, ProviderProfileInput,
};
use odoc_model::SecurityLevel;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

struct MockResponse {
    status: &'static str,
    content_type: &'static str,
    chunks: Vec<Vec<u8>>,
    initial_delay: Duration,
    chunk_delay: Duration,
}

fn chat_request(security_level: SecurityLevel) -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "请回复：收到".to_owned(),
        }],
        temperature: Some(0.2),
        max_tokens: Some(64),
        json_mode: false,
        security_level,
    }
}

fn profile(kind: ProviderKind, base_url: String) -> ProviderProfile {
    ProviderProfile::try_from(ProviderProfileInput {
        id: "test-provider".to_owned(),
        kind,
        display_name: "Test provider".to_owned(),
        base_url,
        model: "test-model".to_owned(),
        enabled: true,
    })
    .expect("test profile should be valid")
}

async fn spawn_server(
    response: MockResponse,
) -> (String, oneshot::Receiver<Vec<u8>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let (request_tx, request_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("request should connect");
        let request = read_request(&mut socket).await;
        let _ = request_tx.send(request);

        tokio::time::sleep(response.initial_delay).await;
        let content_length = response.chunks.iter().map(Vec::len).sum::<usize>();
        let headers = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.status, response.content_type, content_length
        );
        if socket.write_all(headers.as_bytes()).await.is_err() {
            return;
        }
        for chunk in response.chunks {
            if socket.write_all(&chunk).await.is_err() {
                return;
            }
            let _ = socket.flush().await;
            tokio::time::sleep(response.chunk_delay).await;
        }
    });
    (format!("http://{address}"), request_rx, task)
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = socket.read(&mut buffer).await.expect("request should read");
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = find_bytes(&request, b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }
    request
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[tokio::test]
async fn compatible_api_roots_are_respected_for_chat_models_and_embeddings() {
    for root in [
        "",
        "/v1",
        "/compatible-mode/v1",
        "/api/paas/v4",
        "/v2",
        "/v1beta/openai",
        "/api/v1",
        "/openai/v1",
    ] {
        for trailing_slash in [false, true] {
            for operation in ["chat/completions", "models", "embeddings"] {
                let body = match operation {
                    "chat/completions" => {
                        r#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#
                    }
                    "models" => r#"{"data":[{"id":"test-model"}]}"#,
                    _ => r#"{"data":[{"embedding":[0.1,0.2]}]}"#,
                };
                let (host, request_rx, task) = spawn_server(MockResponse {
                    status: "200 OK",
                    content_type: "application/json",
                    chunks: vec![body.as_bytes().to_vec()],
                    initial_delay: Duration::ZERO,
                    chunk_delay: Duration::ZERO,
                })
                .await;
                let suffix = if trailing_slash { "/" } else { "" };
                let client = ProviderClient::new(
                    profile(
                        ProviderKind::OpenAiCompatible,
                        format!("{host}{root}{suffix}"),
                    ),
                    None,
                )
                .expect("client should build");
                match operation {
                    "chat/completions" => {
                        client
                            .chat(chat_request(SecurityLevel::Internal))
                            .await
                            .expect("chat");
                    }
                    "models" => {
                        assert_eq!(
                            client.list_models().await.expect("models"),
                            vec!["test-model"]
                        );
                    }
                    _ => {
                        assert_eq!(
                            client
                                .embed(&["sample".to_owned()], SecurityLevel::Internal)
                                .await
                                .expect("embeddings")
                                .len(),
                            1
                        );
                    }
                }
                let request = request_rx.await.expect("request should arrive");
                let request = String::from_utf8_lossy(&request);
                let method = if operation == "models" { "GET" } else { "POST" };
                let expected_root = if root.is_empty() { "/v1" } else { root };
                assert!(
                    request.starts_with(&format!(
                        "{method} {expected_root}/{operation} HTTP/1.1\r\n"
                    )),
                    "unexpected endpoint: {}",
                    request.lines().next().unwrap_or_default()
                );
                task.await.expect("server should finish");
            }
        }
    }
}

#[tokio::test]
async fn openai_chat_uses_compatible_endpoint_and_keeps_secret_out_of_json() {
    let body = r#"{"choices":[{"message":{"content":"收到"},"finish_reason":"stop"}],"usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::OpenAiCompatible, format!("{base_url}/v1")),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");

    let response = client
        .chat(chat_request(SecurityLevel::Internal))
        .await
        .expect("chat should succeed");
    assert_eq!(response.content, "收到");
    assert_eq!(response.usage.expect("usage should exist").total_tokens, 10);

    let request = request_rx.await.expect("request should be captured");
    let request = String::from_utf8_lossy(&request);
    assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer top-secret-key")
    );
    let body = request.split("\r\n\r\n").nth(1).expect("body should exist");
    assert!(!body.contains("top-secret-key"));
    assert!(body.contains("\"model\":\"test-model\""));
    assert!(body.contains("\"max_tokens\":64"));
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn openai_chat_accepts_cline_success_data_envelope() {
    let body = r#"{"success":true,"data":{"choices":[{"message":{"content":"收到"},"finish_reason":"stop"}],"usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}}}"#;
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::OpenAiCompatible, format!("{base_url}/v1")),
        Some("cline-key".to_owned()),
    )
    .expect("client should build");

    let response = client
        .chat(chat_request(SecurityLevel::Internal))
        .await
        .expect("wrapped chat should succeed");
    assert_eq!(response.content, "收到");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(response.usage.expect("usage should exist").total_tokens, 10);
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn cline_pass_omits_the_completion_cap() {
    let body = r#"{"success":true,"data":{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1,"total_tokens":6}}}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        ProviderProfile::try_from(ProviderProfileInput {
            id: "cline-pass".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Cline Pass".to_owned(),
            base_url: format!("{base_url}/api/v1"),
            model: "cline-pass/deepseek-v4.1-flash".to_owned(),
            enabled: true,
        })
        .expect("profile"),
        Some("cline-key".to_owned()),
    )
    .expect("client should build");
    let mut request = chat_request(SecurityLevel::Internal);
    request.max_tokens = Some(512);

    client.chat(request).await.expect("chat should succeed");

    let request =
        String::from_utf8_lossy(&request_rx.await.expect("request should be captured")).to_string();
    let body = request.split("\r\n\r\n").nth(1).expect("body should exist");
    assert!(
        !body.contains("\"max_tokens\""),
        "Cline Pass must let the gateway choose the output limit"
    );
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn openai_stream_handles_fragmented_sse_and_done_marker() {
    let event_one =
        "data: {\"choices\":[{\"delta\":{\"content\":\"你\"},\"finish_reason\":null}]}\n\n";
    let event_two = "data: {\"choices\":[{\"delta\":{\"content\":\"好\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let split = event_one.find('你').expect("UTF-8 content should exist") + 1;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "text/event-stream",
        chunks: vec![
            event_one.as_bytes()[..split].to_vec(),
            event_one.as_bytes()[split..].to_vec(),
            event_two.as_bytes().to_vec(),
        ],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::from_millis(5),
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::OpenAiCompatible, base_url), None)
        .expect("client should build");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);

    client
        .chat_stream(chat_request(SecurityLevel::Internal), sender)
        .await
        .expect("stream should complete");
    let mut deltas = Vec::new();
    while let Some(delta) = receiver.recv().await {
        deltas.push(delta);
    }
    assert_eq!(
        deltas
            .iter()
            .map(|delta| delta.content.as_str())
            .collect::<String>(),
        "你好"
    );
    assert_eq!(
        deltas
            .last()
            .and_then(|delta| delta.finish_reason.as_deref()),
        Some("stop")
    );
    // The usage-bearing final chunk only arrives when the request asks for it.
    let request_bytes = request_rx.await.expect("request should arrive");
    let request = String::from_utf8_lossy(&request_bytes);
    assert!(request.contains("\"stream_options\":{\"include_usage\":true}"));
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn ollama_supports_standard_and_newline_delimited_responses() {
    let body = r#"{"message":{"role":"assistant","content":"收到"},"done":true,"done_reason":"stop","prompt_eval_count":4,"eval_count":2}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::Ollama, base_url), None)
        .expect("client should build");
    let response = client
        .chat(chat_request(SecurityLevel::Secret))
        .await
        .expect("local Ollama should be allowed for secret documents");
    assert_eq!(response.content, "收到");
    assert_eq!(response.usage.expect("usage should exist").total_tokens, 6);
    let request_bytes = request_rx.await.expect("request should arrive");
    let request = String::from_utf8_lossy(&request_bytes);
    assert!(request.starts_with("POST /api/chat HTTP/1.1\r\n"));
    task.await.expect("mock server should finish");

    let first = "{\"message\":{\"content\":\"逐\"},\"done\":false}\n".as_bytes();
    let second =
        "{\"message\":{\"content\":\"行\"},\"done\":true,\"done_reason\":\"stop\"}\n".as_bytes();
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/x-ndjson",
        chunks: vec![first[..9].to_vec(), first[9..].to_vec(), second.to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::from_millis(5),
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::Ollama, base_url), None)
        .expect("client should build");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    client
        .chat_stream(chat_request(SecurityLevel::Secret), sender)
        .await
        .expect("stream should complete");
    let mut text = String::new();
    while let Some(delta) = receiver.recv().await {
        text.push_str(&delta.content);
    }
    assert_eq!(text, "逐行");
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn errors_are_stable_redacted_and_invalid_json_is_rejected() {
    let leaked = br#"{"error":"top-secret-key was rejected"}"#;
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "401 Unauthorized",
        content_type: "application/json",
        chunks: vec![leaked.to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::OpenAiCompatible, base_url),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");
    let error = client
        .chat(chat_request(SecurityLevel::Internal))
        .await
        .expect_err("HTTP error should fail");
    assert_eq!(error, LlmError::HttpStatus(401));
    assert!(!format!("{error:?} {error}").contains("top-secret-key"));
    task.await.expect("mock server should finish");

    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![b"not json".to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::Ollama, base_url), None)
        .expect("client should build");
    assert_eq!(
        client.chat(chat_request(SecurityLevel::Internal)).await,
        Err(LlmError::InvalidResponse)
    );
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "307 Temporary Redirect",
        content_type: "text/plain",
        chunks: Vec::new(),
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::Ollama, base_url), None)
        .expect("client should build");
    assert_eq!(
        client.chat(chat_request(SecurityLevel::Internal)).await,
        Err(LlmError::HttpStatus(307))
    );
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn timeout_cancellation_and_security_checks_happen_safely() {
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![b"{}".to_vec()],
        initial_delay: Duration::from_millis(150),
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::with_timeout(
        profile(ProviderKind::Ollama, base_url),
        None,
        Duration::from_millis(30),
    )
    .expect("client should build");
    assert_eq!(
        client.chat(chat_request(SecurityLevel::Internal)).await,
        Err(LlmError::Timeout)
    );
    task.await.expect("mock server should finish");

    let remote = ProviderClient::new(
        profile(
            ProviderKind::OpenAiCompatible,
            "https://models.example.com".to_owned(),
        ),
        None,
    )
    .expect("client should build");
    assert_eq!(
        remote.chat(chat_request(SecurityLevel::Secret)).await,
        Err(LlmError::RemoteProviderBlocked)
    );

    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    assert_eq!(
        remote
            .chat_stream(chat_request(SecurityLevel::Internal), sender)
            .await,
        Err(LlmError::Cancelled)
    );
}

#[tokio::test]
async fn anthropic_chat_uses_messages_endpoint_with_api_key_header() {
    let body = r#"{"id":"msg_01","type":"message","role":"assistant","model":"test-model","content":[{"type":"text","text":"收到"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":8,"output_tokens":2}}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::Anthropic, format!("{base_url}/v1")),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");
    let request = ChatRequest {
        messages: vec![
            ChatMessage {
                role: ChatRole::System,
                content: "你是公文助手".to_owned(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "请回复：收到".to_owned(),
            },
        ],
        temperature: Some(0.2),
        max_tokens: Some(4096),
        json_mode: false,
        security_level: SecurityLevel::Internal,
    };

    let response = client.chat(request).await.expect("chat should succeed");
    assert_eq!(response.content, "收到");
    assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(response.usage.expect("usage should exist").total_tokens, 10);

    let request = request_rx.await.expect("request should be captured");
    let request = String::from_utf8_lossy(&request);
    assert!(request.starts_with("POST /v1/messages HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("x-api-key: top-secret-key")
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("anthropic-version: 2023-06-01")
    );
    let body = request.split("\r\n\r\n").nth(1).expect("body should exist");
    assert!(!body.contains("top-secret-key"));
    assert!(body.contains("\"model\":\"test-model\""));
    assert!(body.contains("\"max_tokens\":4096"));
    assert!(body.contains("\"system\":\"你是公文助手\""));
    assert!(!body.contains("temperature"));
    assert!(!body.contains("\"role\":\"system\""));
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn anthropic_stream_parses_content_block_deltas_and_stop() {
    let event_one = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":8,\"output_tokens\":0}}}\n\n";
    let event_two = "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"你\"}}\n\n";
    let event_three = "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"好\"}}\n\n";
    // Real Anthropic streams never carry `input_tokens` on `message_delta`; the prompt count
    // only arrives on `message_start` and must be merged by the client.
    let event_four = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n";
    let event_five = "data: {\"type\":\"message_stop\"}\n\n";
    let split = event_two.find('你').expect("UTF-8 content should exist") + 1;
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "text/event-stream",
        chunks: vec![
            event_one.as_bytes().to_vec(),
            event_two.as_bytes()[..split].to_vec(),
            event_two.as_bytes()[split..].to_vec(),
            event_three.as_bytes().to_vec(),
            event_four.as_bytes().to_vec(),
            event_five.as_bytes().to_vec(),
        ],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::from_millis(5),
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::Anthropic, base_url),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);

    client
        .chat_stream(chat_request(SecurityLevel::Internal), sender)
        .await
        .expect("stream should complete");
    let mut deltas = Vec::new();
    while let Some(delta) = receiver.recv().await {
        deltas.push(delta);
    }
    assert_eq!(
        deltas
            .iter()
            .map(|delta| delta.content.as_str())
            .collect::<String>(),
        "你好"
    );
    assert_eq!(
        deltas
            .last()
            .and_then(|delta| delta.finish_reason.as_deref()),
        Some("end_turn")
    );
    assert_eq!(
        deltas
            .last()
            .and_then(|delta| delta.usage.as_ref())
            .expect("usage should exist")
            .total_tokens,
        12
    );
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn list_models_parses_each_provider_kind() {
    let openai_body = r#"{"object":"list","data":[{"id":"gpt-4o"},{"id":"gpt-4.1-mini"}]}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![openai_body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::OpenAiCompatible, format!("{base_url}/v1")),
        None,
    )
    .expect("client should build");
    let models = client
        .list_models()
        .await
        .expect("openai model list should parse");
    assert_eq!(models, ["gpt-4.1-mini", "gpt-4o"]);
    let request = request_rx.await.expect("request should arrive");
    assert!(String::from_utf8_lossy(&request).starts_with("GET /v1/models HTTP/1.1\r\n"));
    task.await.expect("mock server should finish");
    drop(models);

    let ollama_body = r#"{"models":[{"name":"qwen3:8b"},{"name":"llama3:8b"}]}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![ollama_body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(profile(ProviderKind::Ollama, base_url), None)
        .expect("client should build");
    assert_eq!(
        client
            .list_models()
            .await
            .expect("ollama model list should parse"),
        ["llama3:8b", "qwen3:8b"]
    );
    let request = request_rx.await.expect("request should arrive");
    assert!(String::from_utf8_lossy(&request).starts_with("GET /api/tags HTTP/1.1\r\n"));
    task.await.expect("mock server should finish");

    let anthropic_body = r#"{"data":[{"id":"claude-opus-5"},{"id":"claude-sonnet-5"}]}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![anthropic_body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::Anthropic, base_url),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");
    assert_eq!(
        client
            .list_models()
            .await
            .expect("anthropic model list should parse"),
        ["claude-opus-5", "claude-sonnet-5"]
    );
    let request = request_rx.await.expect("request should arrive");
    let request = String::from_utf8_lossy(&request);
    assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("x-api-key: top-secret-key")
    );
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn openai_embeddings_hit_the_embeddings_endpoint_and_return_vectors() {
    let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]},{"embedding":[-0.1,0.0,0.5]}]}"#;
    let (base_url, request_rx, task) = spawn_server(MockResponse {
        status: "200 OK",
        content_type: "application/json",
        chunks: vec![body.as_bytes().to_vec()],
        initial_delay: Duration::ZERO,
        chunk_delay: Duration::ZERO,
    })
    .await;
    let client = ProviderClient::new(
        profile(ProviderKind::OpenAiCompatible, format!("{base_url}/v1")),
        Some("top-secret-key".to_owned()),
    )
    .expect("client should build");

    let vectors = client
        .embed(
            &["第一段".to_owned(), "第二段".to_owned()],
            SecurityLevel::Internal,
        )
        .await
        .expect("embedding should succeed");
    assert_eq!(vectors, vec![vec![0.1, 0.2, 0.3], vec![-0.1, 0.0, 0.5]]);

    let request = request_rx.await.expect("request should be captured");
    let request = String::from_utf8_lossy(&request);
    assert!(request.starts_with("POST /v1/embeddings HTTP/1.1\r\n"));
    let body = request.split("\r\n\r\n").nth(1).expect("body should exist");
    assert!(body.contains("\"model\":\"test-model\""));
    assert!(body.contains("\"input\":[\"第一段\",\"第二段\"]"));
    assert!(!body.contains("top-secret-key"));
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn non_openai_providers_report_embeddings_as_unsupported() {
    let client = ProviderClient::new(
        profile(
            ProviderKind::Anthropic,
            "https://api.example.com".to_owned(),
        ),
        Some("key".to_owned()),
    )
    .expect("client should build");
    let error = client
        .embed(&["文本".to_owned()], SecurityLevel::Internal)
        .await
        .expect_err("embedding should be unsupported");
    assert_eq!(error.code(), "model_embedding_unavailable");
}

#[tokio::test]
async fn restricted_documents_never_embed_to_a_remote_provider() {
    let client = ProviderClient::new(
        profile(
            ProviderKind::OpenAiCompatible,
            "https://api.example.com".to_owned(),
        ),
        Some("key".to_owned()),
    )
    .expect("client should build");
    // A remote OpenAI-compatible provider must refuse to embed secret/confidential text,
    // exactly like chat: the security gate is at the provider boundary, not the caller.
    for level in [SecurityLevel::Secret, SecurityLevel::Confidential] {
        let error = client
            .embed(&["涉密内容".to_owned()], level)
            .await
            .expect_err("restricted embedding must be blocked");
        assert_eq!(error.code(), "remote_provider_blocked");
    }
    // Internal documents pass the gate and only then fail on the (unreachable) network,
    // proving the block above was the security gate and not a generic network error.
    let error = client
        .embed(&["内部内容".to_owned()], SecurityLevel::Internal)
        .await
        .expect_err("request to unreachable host must fail");
    assert_ne!(error.code(), "remote_provider_blocked");
}
