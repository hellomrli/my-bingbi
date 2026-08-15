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
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn openai_stream_handles_fragmented_sse_and_done_marker() {
    let event_one =
        "data: {\"choices\":[{\"delta\":{\"content\":\"你\"},\"finish_reason\":null}]}\n\n";
    let event_two = "data: {\"choices\":[{\"delta\":{\"content\":\"好\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let split = event_one.find('你').expect("UTF-8 content should exist") + 1;
    let (base_url, _request_rx, task) = spawn_server(MockResponse {
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
