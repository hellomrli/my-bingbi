use crate::types::path_matches_prefix;
use crate::{Frontmatter, KbError, KbHit, KbNote, KbProvider, KbQuery, SearchMode};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::broadcast;
use url::Url;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const FNS_MCP_NOTE_LIST_LIMIT: usize = 100;

/// Minimal Streamable HTTP MCP adapter for FNS-compatible note tools.
///
/// The adapter intentionally uses only the read/write note tools and keeps the token in a
/// private request header. A fresh MCP session is negotiated per operation, which is slower than
/// a long-lived session but avoids leaking stale session identifiers across vault changes.
#[derive(Clone)]
pub struct McpProvider {
    endpoint: Url,
    token: HeaderValue,
    vault: String,
    client: Client,
    events: broadcast::Sender<crate::ChangeEvent>,
}

impl McpProvider {
    /// Builds an MCP-backed knowledge-base provider.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] when the endpoint, vault or token is malformed,
    /// and a network error when the HTTP client cannot be constructed.
    pub fn new(endpoint: &str, token: &str, vault: impl Into<String>) -> Result<Self, KbError> {
        let endpoint = crate::types::parse_remote_or_local_url(endpoint.trim())?;
        let vault = vault.into().trim().to_owned();
        if vault.is_empty() || vault.len() > 256 || vault.chars().any(char::is_control) {
            return Err(KbError::InvalidConfiguration);
        }
        let token = token.trim();
        if token.is_empty() || token.len() > 16 * 1024 || token.chars().any(char::is_control) {
            return Err(KbError::InvalidConfiguration);
        }
        let token = HeaderValue::from_str(token).map_err(|_| KbError::InvalidConfiguration)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("bingbi-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| KbError::InvalidConfiguration)?;
        let (events, _) = broadcast::channel(64);
        Ok(Self {
            endpoint,
            token,
            vault,
            client,
            events,
        })
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, KbError> {
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "bingbi", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        let initialized = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        });
        let mut session_id: Option<String> = None;
        for payload in [initialize, initialized, request] {
            let mut builder = self
                .client
                .post(self.endpoint.clone())
                .header(
                    AUTHORIZATION,
                    format!("Bearer {}", self.token.to_str().unwrap_or_default()),
                )
                .header("Accept", "application/json, text/event-stream")
                .header("Content-Type", "application/json")
                .header("MCP-Protocol-Version", "2025-03-26")
                .json(&payload);
            if let Some(value) = session_id.as_deref() {
                builder = builder.header("MCP-Session-Id", value);
            }
            let response = builder.send().await.map_err(|error| {
                if error.is_timeout() {
                    KbError::Timeout
                } else {
                    KbError::Network
                }
            })?;
            if response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::FORBIDDEN
            {
                return Err(KbError::HttpStatus(response.status().as_u16()));
            }
            if !response.status().is_success() {
                return Err(KbError::HttpStatus(response.status().as_u16()));
            }
            if let Some(value) = response.headers().get("MCP-Session-Id") {
                session_id = value.to_str().ok().map(str::to_owned);
            }
            // Stream the body and stop as soon as the cap is exceeded, instead of buffering
            // the whole response first (see fns::finish_json for the same reasoning).
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| KbError::InvalidResponse)?;
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err(KbError::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            if payload.get("method").and_then(Value::as_str) == Some("tools/call") {
                let value = parse_json_or_sse(&bytes)?;
                if let Some(error) = value.get("error") {
                    let _ = error;
                    return Err(KbError::InvalidResponse);
                }
                let result = value.get("result").cloned().unwrap_or(value);
                return tool_payload(result).ok_or(KbError::InvalidResponse);
            }
        }
        Err(KbError::InvalidResponse)
    }
}

#[async_trait]
impl KbProvider for McpProvider {
    async fn search(&self, query: &KbQuery) -> Result<Vec<KbHit>, KbError> {
        query.validate()?;
        let path_prefix = query
            .path_prefix
            .as_deref()
            .map(crate::types::normalize_relative_path)
            .transpose()?;
        let result = self
            .call_tool(
                "note_list",
                json!({
                    "vault": query.vault.as_deref().unwrap_or(&self.vault),
                    "keyword": query.keyword,
                    "searchMode": match query.search_mode { SearchMode::Path => "path", SearchMode::Content | SearchMode::Regex => "content" }
                }),
            )
            .await?;
        let rows = extract_json_payload(result).ok_or(KbError::InvalidResponse)?;
        let mut hits = Vec::new();
        if let Some(notes) = rows
            .get("notes")
            .and_then(Value::as_array)
            .or_else(|| rows.as_array())
        {
            for note in notes {
                if let Some(mut hit) =
                    hit_from_mcp(note, query.vault.as_deref().unwrap_or(&self.vault))
                {
                    if !path_matches_prefix(&hit.path, path_prefix.as_deref()) {
                        continue;
                    }
                    if !query.filters.is_empty() {
                        let note = match self.get(&hit.path).await {
                            Ok(note) => note,
                            Err(KbError::NotFound) => continue,
                            Err(error) => return Err(error),
                        };
                        if !query
                            .filters
                            .iter()
                            .all(|(key, value)| note.frontmatter.get(key) == Some(value))
                        {
                            continue;
                        }
                        hit.frontmatter = note.frontmatter;
                        if hit.snippet.is_empty() {
                            hit.snippet = note.content.chars().take(480).collect();
                        }
                    }
                    hits.push(hit);
                }
            }
        }
        hits.truncate(query.normalized_limit() as usize);
        Ok(hits)
    }

    async fn get(&self, path: &str) -> Result<KbNote, KbError> {
        crate::types::validate_relative_path(path)?;
        let result = self
            .call_tool("note_get", json!({"vault": self.vault, "path": path}))
            .await?;
        let value = extract_json_payload(result).ok_or(KbError::InvalidResponse)?;
        note_from_mcp(&value, &self.vault, path)
    }

    async fn backlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        self.link_paths("note_get_backlinks", path).await
    }

    async fn outlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        self.link_paths("note_get_outlinks", path).await
    }

    async fn write(&self, path: &str, content: &str) -> Result<(), KbError> {
        crate::types::validate_relative_path(path)?;
        if content.len() > crate::types::MAX_NOTE_BYTES
            || crate::types::contains_invalid_note_control(content)
        {
            return Err(KbError::InvalidNote);
        }
        self.call_tool(
            "note_create_or_update",
            json!({"vault": self.vault, "path": path, "content": content}),
        )
        .await?;
        let _ = self.events.send(crate::ChangeEvent {
            kind: crate::ChangeKind::Modified,
            vault: self.vault.clone(),
            path: path.to_owned(),
            old_path: None,
            cursor: None,
        });
        Ok(())
    }

    async fn patch_frontmatter(
        &self,
        path: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), KbError> {
        crate::types::validate_relative_path(path)?;
        if fields.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 128
                || key.chars().any(char::is_control)
                || value.len() > 16 * 1024
                || value.chars().any(char::is_control)
        }) {
            return Err(KbError::InvalidNote);
        }
        let updates = serde_json::to_string(fields).map_err(|_| KbError::InvalidNote)?;
        self.call_tool(
            "note_patch_frontmatter",
            json!({"vault": self.vault, "path": path, "updates": updates, "remove": "[]"}),
        )
        .await?;
        let _ = self.events.send(crate::ChangeEvent {
            kind: crate::ChangeKind::Modified,
            vault: self.vault.clone(),
            path: path.to_owned(),
            old_path: None,
            cursor: None,
        });
        Ok(())
    }

    async fn list_notes(
        &self,
        path_prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<KbNote>, KbError> {
        let limit = limit.max(1) as usize;
        let prefix = path_prefix
            .map(crate::types::normalize_relative_path)
            .transpose()?;
        let result = self
            .call_tool(
                "note_list",
                json!({"vault": self.vault, "keyword": "", "searchMode": "path"}),
            )
            .await?;
        let value = extract_json_payload(result).ok_or(KbError::InvalidResponse)?;
        let rows = value
            .get("notes")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .ok_or(KbError::InvalidResponse)?;
        if rows.len() >= FNS_MCP_NOTE_LIST_LIMIT && limit > rows.len() {
            return Err(KbError::ListingTruncated);
        }
        let mut notes = Vec::new();
        for row in rows {
            let Some(hit) = hit_from_mcp(row, &self.vault) else {
                continue;
            };
            if !path_matches_prefix(&hit.path, prefix.as_deref()) {
                continue;
            }
            match self.get(&hit.path).await {
                Ok(note) => notes.push(note),
                Err(KbError::NotFound) => {}
                Err(error) => return Err(error),
            }
            if notes.len() >= limit {
                break;
            }
        }
        Ok(notes)
    }

    fn watch(&self) -> Option<broadcast::Receiver<crate::ChangeEvent>> {
        Some(self.events.subscribe())
    }
}

impl McpProvider {
    async fn link_paths(&self, tool: &str, path: &str) -> Result<Vec<String>, KbError> {
        crate::types::validate_relative_path(path)?;
        let result = self
            .call_tool(tool, json!({"vault": self.vault, "path": path}))
            .await?;
        let value = extract_json_payload(result).ok_or(KbError::InvalidResponse)?;
        let links = value
            .get("links")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .ok_or(KbError::InvalidResponse)?;
        let mut paths = links
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .or_else(|| item.get("path").and_then(Value::as_str).map(str::to_owned))
                    .or_else(|| {
                        item.get("sourcePath")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .or_else(|| {
                        item.get("targetPath")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
            })
            .filter_map(|path| crate::types::normalize_relative_path(&path).ok())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn parse_json_or_sse(bytes: &[u8]) -> Result<Value, KbError> {
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| KbError::InvalidResponse)?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .next_back()
        .map(str::trim)
        .ok_or(KbError::InvalidResponse)?;
    serde_json::from_str(data).map_err(|_| KbError::InvalidResponse)
}

fn extract_json_payload(value: Value) -> Option<Value> {
    match value {
        Value::String(text) => serde_json::from_str(&text).ok(),
        Value::Array(values) => values.into_iter().find_map(extract_json_payload),
        Value::Object(mut object) => {
            if let Some(value) = object.remove("text") {
                return extract_json_payload(value);
            }
            Some(Value::Object(object))
        }
        other => Some(other),
    }
}

fn tool_payload(mut result: Value) -> Option<Value> {
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if let Value::Object(object) = &mut result {
        if let Some(value) = object.remove("structuredContent") {
            return extract_json_payload(value);
        }
        if let Some(value) = object.remove("content") {
            return extract_json_payload(value);
        }
    }
    extract_json_payload(result)
}

fn hit_from_mcp(value: &Value, vault: &str) -> Option<KbHit> {
    let object = value.as_object()?;
    let path = object.get("path")?.as_str()?.replace('\\', "/");
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(&path))
        .to_owned();
    Some(KbHit {
        vault: object
            .get("vault")
            .and_then(Value::as_str)
            .unwrap_or(vault)
            .to_owned(),
        path,
        title,
        snippet: object
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(480)
            .collect(),
        score: 0.0,
        frontmatter: frontmatter_from_value(object.get("frontmatter")),
    })
}

fn note_from_mcp(value: &Value, vault: &str, fallback: &str) -> Result<KbNote, KbError> {
    let root = value.as_object().ok_or(KbError::InvalidResponse)?;
    let object = root.get("note").and_then(Value::as_object).unwrap_or(root);
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .replace('\\', "/");
    crate::types::validate_relative_path(&path)?;
    let content = object
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if content.len() > crate::types::MAX_NOTE_BYTES {
        return Err(KbError::TooLarge);
    }
    let parsed = crate::local::parse_markdown_note(vault, &path, &content)?;
    Ok(KbNote {
        vault: value
            .get("vault")
            .and_then(Value::as_str)
            .or_else(|| object.get("vault").and_then(Value::as_str))
            .unwrap_or(vault)
            .to_owned(),
        path: path.clone(),
        title: object
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&parsed.title)
            .to_owned(),
        content: parsed.content,
        frontmatter: if parsed.frontmatter.is_empty() {
            frontmatter_from_value(object.get("frontmatter"))
        } else {
            parsed.frontmatter
        },
        tags: if parsed.tags.is_empty() {
            string_array(object.get("tags"))
        } else {
            parsed.tags
        },
        links: if parsed.links.is_empty() {
            string_array(object.get("links"))
        } else {
            parsed.links
        },
        modified_at: object
            .get("mtime")
            .or_else(|| object.get("updatedTimestamp"))
            .or_else(|| object.get("lastTime"))
            .and_then(Value::as_i64),
    })
}

fn frontmatter_from_value(value: Option<&Value>) -> Frontmatter {
    let mut result = Frontmatter::new();
    if let Some(Value::Object(fields)) = value {
        for (key, value) in fields {
            if let Some(text) = value.as_str() {
                result.insert(key.clone(), text.to_owned());
            } else if let Some(number) = value.as_i64() {
                result.insert(key.clone(), number.to_string());
            }
        }
    }
    result
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
