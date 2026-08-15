use odoc_kb::{FnsProvider, KbError, KbProvider, KbQuery};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

struct MockResponse {
    status: &'static str,
    content_type: &'static str,
    body: &'static [u8],
    delay: Duration,
    extra_headers: &'static str,
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
        tokio::time::sleep(response.delay).await;
        let headers = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
            response.status,
            response.content_type,
            response.body.len(),
            response.extra_headers,
        );
        if socket.write_all(headers.as_bytes()).await.is_ok() {
            let _ = socket.write_all(response.body).await;
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
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    request
}

fn response(status: &'static str, body: &'static [u8]) -> MockResponse {
    MockResponse {
        status,
        content_type: "application/json",
        body,
        delay: Duration::ZERO,
        extra_headers: "",
    }
}

#[tokio::test]
async fn search_uses_one_api_prefix_expected_query_and_raw_token() {
    let body = br#"[{"vault":"\u653f\u52a1\u5e93","path":"\u516c\u6587/2026/a.md","title":"\u9632\u6c5b\u8bf7\u793a","snippet":"\u8bbe\u5907\u66f4\u65b0"}]"#;
    let (base_url, request_rx, task) = spawn_server(response("200 OK", body)).await;
    let provider = FnsProvider::new(
        &format!("{base_url}/api"),
        "raw-fns-token".to_owned(),
        Some("政务库".to_owned()),
    )
    .expect("provider should build");
    let hits = provider
        .search(&KbQuery {
            keyword: "防汛 设备".to_owned(),
            limit: 7,
            ..KbQuery::default()
        })
        .await
        .expect("search should succeed");
    assert_eq!(hits.len(), 1);

    let request = request_rx.await.expect("request should be captured");
    let request = String::from_utf8_lossy(&request);
    let request_line = request.lines().next().expect("request line should exist");
    assert!(request_line.starts_with("GET /api/notes?"));
    assert!(!request_line.contains("/api/api/"));
    assert!(request_line.contains("keyword=%E9%98%B2%E6%B1%9B+%E8%AE%BE%E5%A4%87"));
    assert!(request_line.contains("searchContent=true"));
    assert!(request_line.contains("searchMode=content"));
    assert!(request_line.contains("page=1"));
    assert!(request_line.contains("pageSize=7"));
    assert!(request_line.contains("vault=%E6%94%BF%E5%8A%A1%E5%BA%93"));
    let lower = request.to_ascii_lowercase();
    assert!(lower.contains("\r\nauthorization: raw-fns-token\r\n"));
    assert!(!lower.contains("authorization: bearer"));
    task.await.expect("mock server should finish");
}

#[tokio::test]
async fn maps_not_found_timeout_and_redirect_without_following() {
    let (base_url, _request_rx, task) = spawn_server(response("404 Not Found", b"{}")).await;
    let provider =
        FnsProvider::new(&base_url, "token".to_owned(), None).expect("provider should build");
    assert!(matches!(
        provider.get("missing.md").await,
        Err(KbError::NotFound)
    ));
    task.await.expect("404 server should finish");

    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        delay: Duration::from_millis(150),
        ..response("200 OK", br#"{"path":"a.md","content":"body"}"#)
    })
    .await;
    let provider = FnsProvider::with_timeout(
        &base_url,
        "token".to_owned(),
        None,
        Duration::from_millis(30),
    )
    .expect("provider should build");
    assert!(matches!(provider.get("a.md").await, Err(KbError::Timeout)));
    task.await.expect("timeout server should finish");

    let (base_url, _request_rx, task) = spawn_server(MockResponse {
        extra_headers: "Location: /api/note\r\n",
        ..response("307 Temporary Redirect", b"")
    })
    .await;
    let provider =
        FnsProvider::new(&base_url, "token".to_owned(), None).expect("provider should build");
    assert!(matches!(
        provider.get("a.md").await,
        Err(KbError::HttpStatus(307))
    ));
    task.await.expect("redirect server should finish");
}
