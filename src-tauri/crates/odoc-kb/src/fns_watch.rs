use crate::{ChangeEvent, ChangeKind, KbError};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const MAX_WS_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SYNC_DETAIL_COUNT: usize = 100_000;
/// A host that accepts TCP but stalls the opening handshake must not park the sync
/// supervisor until the OS-level TCP timeout (minutes).
const WEBSOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounds the pending detail-page table against a hostile server that keeps sending frames
/// with fresh page indexes.
const MAX_PENDING_PAGES: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnsWatchConfig {
    pub base_url: String,
    pub token: String,
    pub vault: String,
    pub last_cursor: i64,
}

#[derive(Debug)]
pub enum FnsWatchEvent {
    Change(FnsPendingChange),
    Cursor(i64),
}

/// A sync change that is acknowledged only after the consumer durably applies it.
#[derive(Debug)]
pub struct FnsPendingChange {
    pub event: ChangeEvent,
    pub advance_cursor: bool,
    persisted: Option<oneshot::Sender<bool>>,
}

impl FnsPendingChange {
    pub fn commit(mut self) {
        if let Some(sender) = self.persisted.take() {
            let _ = sender.send(true);
        }
    }

    pub fn reject(mut self) {
        if let Some(sender) = self.persisted.take() {
            let _ = sender.send(false);
        }
    }
}

/// Connects to FNS WebSocket sync and emits normalized note changes until the caller drops the
/// receiver or the connection ends. The WS credential must carry `p:ws` scope.
///
/// # Errors
///
/// Returns an error when the configuration is invalid, the endpoint cannot be derived, or the
/// connection and its opening handshake frames fail.
#[allow(
    clippy::too_many_lines,
    reason = "the sync handshake and its message loop read as one protocol transcript"
)]
pub async fn watch_fns_notes(
    config: FnsWatchConfig,
) -> Result<mpsc::Receiver<Result<FnsWatchEvent, KbError>>, KbError> {
    validate_config(&config)?;
    let endpoint = websocket_endpoint(&config.base_url)?;
    let (stream, _) =
        tokio::time::timeout(WEBSOCKET_CONNECT_TIMEOUT, connect_async(endpoint.as_str()))
            .await
            .map_err(|_| KbError::Network)?
            .map_err(|_| KbError::Network)?;
    let (mut writer, mut reader) = stream.split();
    writer
        .send(Message::Text(
            frame("Authorization", &Value::String(config.token.clone())).into(),
        ))
        .await
        .map_err(|_| KbError::Network)?;
    writer
        .send(Message::Text(
            frame(
                "ClientInfo",
                &json!({
                    "name": "Bingbi",
                    "version": env!("CARGO_PKG_VERSION"),
                    "type": "desktop",
                    "isDesktop": true,
                    "isLinux": cfg!(target_os = "linux"),
                    "isWin": cfg!(target_os = "windows"),
                    "offlineSyncStrategy": "newTimeMerge"
                }),
            )
            .into(),
        ))
        .await
        .map_err(|_| KbError::Network)?;
    let context = format!("bingbi-{}", config.last_cursor.max(0));
    writer
        .send(Message::Text(
            frame(
                "NoteSync",
                &json!({
                    "context": context,
                    "vault": config.vault,
                    "lastTime": config.last_cursor.max(0),
                    "notes": [],
                    "batchIndex": 0,
                    "totalBatches": 1
                }),
            )
            .into(),
        ))
        .await
        .map_err(|_| KbError::Network)?;

    let vault = config.vault;
    let sync_context = context;
    let (sender, receiver) = mpsc::channel(128);
    tokio::spawn(async move {
        let mut pages = BTreeMap::<i64, PendingPage>::new();
        let mut legacy_page = None;
        let mut last_acked_page = -1_i64;
        let mut last_page_index = None;
        let mut target_cursor = None;
        let mut expected_details = None;
        let mut completed_details = 0_usize;
        let mut cursor_sent = false;
        loop {
            let message = tokio::select! {
                () = sender.closed() => return,
                message = reader.next() => match message {
                    Some(message) => message,
                    None => return,
                },
            };
            let result = match message {
                Ok(Message::Text(text)) if text.len() <= MAX_WS_MESSAGE_BYTES => {
                    parse_server_frame(text.as_ref(), &vault)
                }
                Ok(Message::Ping(payload)) => {
                    if writer.send(Message::Pong(payload)).await.is_err() {
                        let _ = sender.send(Err(KbError::Network)).await;
                        return;
                    }
                    continue;
                }
                Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_)) => continue,
                Ok(Message::Close(_)) => break,
                Ok(Message::Text(_)) => Err(KbError::TooLarge),
                Err(_) => Err(KbError::Network),
            };
            match result {
                Ok(frame) => {
                    if let Some(cursor) = frame.cursor {
                        target_cursor = Some(cursor);
                    }
                    if let Some(count) = frame.end_detail_count {
                        expected_details = Some(count);
                    }
                    if let Some(page) = frame.page {
                        legacy_page = Some(page.page_index);
                        if page.is_last {
                            last_page_index = Some(page.page_index);
                        }
                        if pages.len() >= MAX_PENDING_PAGES && !pages.contains_key(&page.page_index)
                        {
                            let _ = sender.send(Err(KbError::TooLarge)).await;
                            return;
                        }
                        pages.insert(
                            page.page_index,
                            PendingPage {
                                remaining: page.total_count,
                                is_last: page.is_last,
                            },
                        );
                    }
                    if frame.request_first_page
                        && send_page_ack(&mut writer, &sync_context, &vault, -1)
                            .await
                            .is_err()
                    {
                        let _ = sender.send(Err(KbError::Network)).await;
                        return;
                    }
                    for detail in frame.details {
                        let belongs_to_sync =
                            detail.context.as_deref() == Some(sync_context.as_str());
                        if let Some(event) = detail.event {
                            let (persisted, confirmation) = oneshot::channel();
                            if sender
                                .send(Ok(FnsWatchEvent::Change(FnsPendingChange {
                                    event,
                                    advance_cursor: !belongs_to_sync,
                                    persisted: Some(persisted),
                                })))
                                .await
                                .is_err()
                            {
                                return;
                            }
                            if !matches!(confirmation.await, Ok(true)) {
                                let _ = sender.send(Err(KbError::SyncRejected)).await;
                                return;
                            }
                        }
                        if belongs_to_sync {
                            completed_details = completed_details.saturating_add(1);
                        }
                        if belongs_to_sync
                            && let Some(page_index) = detail.page_index.or(legacy_page)
                            && let Some(page) = pages.get_mut(&page_index)
                        {
                            page.remaining = page.remaining.saturating_sub(1);
                        }
                    }

                    let mut ack_page = None;
                    loop {
                        let next = last_acked_page.saturating_add(1);
                        let Some(page) = pages.get(&next) else {
                            break;
                        };
                        if page.remaining > 0 || page.is_last {
                            break;
                        }
                        last_acked_page = next;
                        ack_page = Some(next);
                    }
                    if let Some(page_index) = ack_page
                        && send_page_ack(&mut writer, &sync_context, &vault, page_index)
                            .await
                            .is_err()
                    {
                        let _ = sender.send(Err(KbError::Network)).await;
                        return;
                    }

                    let expected_complete =
                        expected_details.is_some_and(|expected| completed_details >= expected);
                    let pages_complete = last_page_index.is_some_and(|last| {
                        (0..=last)
                            .all(|index| pages.get(&index).is_some_and(|page| page.remaining == 0))
                    });
                    let no_paging_complete = pages.is_empty() && expected_complete;
                    if !cursor_sent
                        && target_cursor.is_some()
                        && (expected_details == Some(0) || pages_complete || no_paging_complete)
                    {
                        cursor_sent = true;
                        if sender
                            .send(Ok(FnsWatchEvent::Cursor(target_cursor.unwrap_or_default())))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            }
        }
    });
    Ok(receiver)
}

fn validate_config(config: &FnsWatchConfig) -> Result<(), KbError> {
    if config.token.trim().is_empty()
        || config.token.len() > 16 * 1024
        || config.token.chars().any(char::is_control)
        || config.vault.trim().is_empty()
        || config.vault.chars().any(char::is_control)
        || config.last_cursor < 0
    {
        return Err(KbError::InvalidConfiguration);
    }
    Ok(())
}

fn websocket_endpoint(base_url: &str) -> Result<Url, KbError> {
    let mut url = crate::types::parse_fns_base_url(base_url)?;
    url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
        .map_err(|()| KbError::InvalidConfiguration)?;
    let base = url.path().trim_end_matches('/');
    let path = if base.ends_with("/api") {
        format!("{base}/user/sync")
    } else if base.is_empty() {
        "/api/user/sync".to_owned()
    } else {
        format!("{base}/api/user/sync")
    };
    url.set_path(&path);
    // v2 keeps the JSON protocol but adds a page index to each detail envelope.  The server
    // ignores these query parameters when it only supports the legacy stop-and-wait protocol.
    url.query_pairs_mut()
        .append_pair("pv", "2")
        .append_pair("pb", "0");
    Ok(url)
}

fn frame(action: &str, value: &Value) -> String {
    format!("{action}|{value}")
}

async fn send_page_ack<S>(
    writer: &mut S,
    context: &str,
    vault: &str,
    page_index: i64,
) -> Result<(), KbError>
where
    S: futures_util::Sink<Message> + Unpin,
{
    writer
        .send(Message::Text(
            frame(
                "NoteSyncPageAck",
                &json!({"context": context, "vault": vault, "pageIndex": page_index}),
            )
            .into(),
        ))
        .await
        .map_err(|_| KbError::Network)
}

#[derive(Debug, Default)]
struct ParsedFrame {
    details: Vec<ParsedDetail>,
    cursor: Option<i64>,
    page: Option<SyncPage>,
    request_first_page: bool,
    end_detail_count: Option<usize>,
}

#[derive(Debug)]
struct ParsedDetail {
    event: Option<ChangeEvent>,
    page_index: Option<i64>,
    context: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct PendingPage {
    remaining: usize,
    is_last: bool,
}

#[derive(Debug, Clone, Copy)]
struct SyncPage {
    page_index: i64,
    total_count: usize,
    is_last: bool,
}

fn parse_server_frame(frame: &str, fallback_vault: &str) -> Result<ParsedFrame, KbError> {
    let (action, payload) = frame.split_once('|').ok_or(KbError::InvalidResponse)?;
    let payload: Value = serde_json::from_str(payload).map_err(|_| KbError::InvalidResponse)?;
    if payload.get("status").and_then(Value::as_bool) == Some(false) {
        return Err(KbError::InvalidResponse);
    }
    let vault = payload
        .get("vault")
        .and_then(Value::as_str)
        .unwrap_or(fallback_vault)
        .to_owned();
    let data = payload.get("data").unwrap_or(&payload);
    if action == "NoteSyncPage" {
        let page_index = data
            .get("pageIndex")
            .and_then(Value::as_i64)
            .filter(|value| *value >= 0)
            .ok_or(KbError::InvalidResponse)?;
        let total_count = data
            .get("totalCount")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value <= MAX_SYNC_DETAIL_COUNT)
            .ok_or(KbError::InvalidResponse)?;
        let is_last = data
            .get("isLast")
            .and_then(Value::as_bool)
            .ok_or(KbError::InvalidResponse)?;
        return Ok(ParsedFrame {
            page: Some(SyncPage {
                page_index,
                total_count,
                is_last,
            }),
            ..ParsedFrame::default()
        });
    }
    if action == "NoteSyncEnd" {
        let queued = [
            "needUploadCount",
            "needModifyCount",
            "needSyncMtimeCount",
            "needDeleteCount",
        ]
        .into_iter()
        .filter_map(|key| data.get(key).and_then(Value::as_u64))
        .sum::<u64>();
        let queued = usize::try_from(queued).map_err(|_| KbError::TooLarge)?;
        if queued > MAX_SYNC_DETAIL_COUNT {
            return Err(KbError::TooLarge);
        }
        let nested = data
            .get("messages")
            .and_then(Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|message| {
                        let action = message.get("action")?.as_str()?;
                        let data = message.get("data").unwrap_or(&Value::Null);
                        Some(ParsedDetail {
                            event: change_from_action(action, data, &vault),
                            page_index: detail_page_index(&payload, data),
                            context: payload
                                .get("context")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let has_nested = !nested.is_empty();
        return Ok(ParsedFrame {
            cursor: data.get("lastTime").and_then(Value::as_i64),
            request_first_page: queued > 0 && !has_nested,
            end_detail_count: Some(queued),
            details: nested,
            ..ParsedFrame::default()
        });
    }
    let is_detail = is_sync_detail_action(action)
        || payload.get("pageIndex").is_some()
        || data.get("pageIndex").is_some();
    Ok(ParsedFrame {
        details: is_detail
            .then(|| ParsedDetail {
                event: change_from_action(action, data, &vault),
                page_index: detail_page_index(&payload, data),
                context: payload
                    .get("context")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
            .into_iter()
            .collect(),
        ..ParsedFrame::default()
    })
}

fn is_sync_detail_action(action: &str) -> bool {
    matches!(
        action,
        "NoteSyncModify"
            | "NoteSyncDelete"
            | "NoteSyncRename"
            | "NoteSyncMtime"
            | "NoteSyncNeedPush"
    )
}

/// Detail envelopes use a one-based page index on the current FNS wire protocol.  The payload
/// field on the page metadata remains zero-based, so only the envelope is translated here.
fn detail_page_index(envelope: &Value, data: &Value) -> Option<i64> {
    envelope
        .get("pageIndex")
        .and_then(Value::as_i64)
        .and_then(|value| (value > 0).then_some(value - 1))
        .or_else(|| data.get("pageIndex").and_then(Value::as_i64))
}

fn change_from_action(action: &str, data: &Value, vault: &str) -> Option<ChangeEvent> {
    let path = data.get("path")?.as_str()?.replace('\\', "/");
    let cursor = data
        .get("lastTime")
        .or_else(|| data.get("updatedTimestamp"))
        .and_then(Value::as_i64);
    match action {
        "NoteSyncModify" | "NoteSyncMtime" => Some(ChangeEvent {
            kind: ChangeKind::Modified,
            vault: vault.to_owned(),
            path,
            old_path: None,
            cursor,
        }),
        "NoteSyncDelete" => Some(ChangeEvent {
            kind: ChangeKind::Deleted,
            vault: vault.to_owned(),
            path,
            old_path: None,
            cursor,
        }),
        "NoteSyncRename" => Some(ChangeEvent {
            kind: ChangeKind::Modified,
            vault: vault.to_owned(),
            path,
            old_path: data
                .get("oldPath")
                .and_then(Value::as_str)
                .map(|value| value.replace('\\', "/")),
            cursor,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_note_frames_and_preserves_rename_source() {
        let frame = parse_server_frame(
            r#"NoteSyncRename|{"vault":"政务库","data":{"path":"新.md","oldPath":"旧.md","lastTime":18}}"#,
            "fallback",
        )
        .unwrap();
        let event = frame.details[0].event.as_ref().expect("change detail");
        assert_eq!(event.path, "新.md");
        assert_eq!(event.old_path.as_deref(), Some("旧.md"));
        assert_eq!(event.cursor, Some(18));
    }

    #[test]
    fn parses_sync_cursor_and_page_metadata() {
        let end = parse_server_frame(
            r#"NoteSyncEnd|{"status":true,"data":{"lastTime":99}}"#,
            "政务库",
        )
        .unwrap();
        assert_eq!(end.cursor, Some(99));
        let page = parse_server_frame(
            r#"NoteSyncPage|{"status":true,"data":{"pageIndex":0,"totalCount":2,"isLast":false}}"#,
            "政务库",
        )
        .unwrap()
        .page
        .unwrap();
        assert_eq!(page.page_index, 0);
        assert_eq!(page.total_count, 2);
        assert!(!page.is_last);
    }
}
