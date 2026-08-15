use crate::KbProvider;
use crate::types::{
    ChangeEvent, ChangeKind, Frontmatter, KbError, KbHit, KbNote, KbQuery, MAX_NOTE_BYTES,
    SearchMode, contains_invalid_note_control, normalize_relative_path, parse_fns_base_url,
    validate_relative_path,
};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::broadcast;
use url::Url;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INDEX_NOTES: u32 = 20_000;
const INDEX_PAGE_SIZE: u32 = 100;
const MAX_SEARCH_LIMIT_CANDIDATES: u32 = 100;

/// REST adapter for Fast Note Sync Service.
///
/// The bearer/token value is held only by this Rust object and is never part of a serializable
/// profile or a frontend binding.
#[derive(Clone)]
pub struct FnsProvider {
    base_url: Url,
    token: HeaderValue,
    vault: Option<String>,
    client: Client,
    events: broadcast::Sender<ChangeEvent>,
}

impl FnsProvider {
    /// Creates an FNS client with the default request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] when the URL, token or vault is unsafe.
    pub fn new(base_url: &str, token: String, vault: Option<String>) -> Result<Self, KbError> {
        Self::with_timeout(base_url, token, vault, DEFAULT_TIMEOUT)
    }

    /// Creates an FNS client with an explicit whole-request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] when the URL, token, vault or timeout is unsafe.
    pub fn with_timeout(
        base_url: &str,
        token: String,
        vault: Option<String>,
        timeout: Duration,
    ) -> Result<Self, KbError> {
        let url = parse_fns_base_url(base_url)?;
        let token = token.into_boxed_str();
        let token = token.trim();
        if token.is_empty()
            || token.len() > 16 * 1024
            || token.chars().any(char::is_control)
            || timeout.is_zero()
        {
            return Err(KbError::InvalidConfiguration);
        }
        let token = HeaderValue::from_str(token).map_err(|_| KbError::InvalidConfiguration)?;
        if vault
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        {
            return Err(KbError::InvalidConfiguration);
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5).min(timeout))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("bingbi-kb/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| KbError::InvalidConfiguration)?;
        let (events, _) = broadcast::channel(64);
        Ok(Self {
            base_url: url,
            token,
            vault,
            client,
            events,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn endpoint(&self, path: &str) -> Url {
        let mut url = self.base_url.clone();
        let base = url.path().trim_end_matches('/');
        let raw_suffix = path.trim_start_matches('/');
        let suffix = raw_suffix.strip_prefix("api/").unwrap_or(raw_suffix);
        let api_base = if base.is_empty() {
            "/api".to_owned()
        } else if base.ends_with("/api") {
            base.to_owned()
        } else {
            format!("{base}/api")
        };
        url.set_path(&format!("{api_base}/{suffix}"));
        url
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, self.endpoint(path))
            .header(AUTHORIZATION, self.token.clone())
            .header("Accept", "application/json")
    }

    async fn finish_json(&self, response: Response) -> Result<Value, KbError> {
        let status = response.status();
        if !status.is_success() {
            return Err(map_status(status));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(KbError::TooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|_| KbError::InvalidResponse)
    }

    async fn get_json(&self, path: &str, query: &[(String, String)]) -> Result<Value, KbError> {
        let response = self
            .request(reqwest::Method::GET, path)
            .query(query)
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        self.finish_json(response).await
    }

    fn vault_value<'a>(&'a self, query: &'a KbQuery) -> Option<&'a str> {
        query.vault.as_deref().or(self.vault.as_deref())
    }
}

#[async_trait]
impl KbProvider for FnsProvider {
    async fn search(&self, query: &KbQuery) -> Result<Vec<KbHit>, KbError> {
        query.validate()?;
        let path_prefix = query
            .path_prefix
            .as_deref()
            .map(normalize_relative_path)
            .transpose()?;
        let mut params = vec![
            ("keyword".to_owned(), query.keyword.clone()),
            ("searchContent".to_owned(), query.search_content.to_string()),
            (
                "searchMode".to_owned(),
                search_mode_text(query.search_mode).to_owned(),
            ),
            ("page".to_owned(), "1".to_owned()),
            (
                "pageSize".to_owned(),
                if query.filters.is_empty() && path_prefix.is_none() {
                    query.normalized_limit()
                } else {
                    MAX_SEARCH_LIMIT_CANDIDATES
                }
                .to_string(),
            ),
        ];
        if let Some(vault) = self.vault_value(query) {
            params.push(("vault".to_owned(), vault.to_owned()));
        }
        if let Some(prefix) = path_prefix.as_deref() {
            params.push(("path".to_owned(), prefix.to_owned()));
        }
        let value = self.get_json("api/notes", &params).await?;
        let rows = array_from_response(value).ok_or(KbError::InvalidResponse)?;
        let candidates = rows
            .into_iter()
            .filter_map(|row| hit_from_value(&row, self.vault_value(query).unwrap_or_default()))
            .collect::<Vec<_>>();
        let mut hits = Vec::new();
        for mut hit in candidates {
            if path_prefix
                .as_deref()
                .is_some_and(|prefix| !hit.path.starts_with(prefix))
            {
                continue;
            }
            if !query.filters.is_empty() {
                let note = match self.get(&hit.path).await {
                    Ok(note) => note,
                    Err(KbError::NotFound) => continue,
                    Err(error) => return Err(error),
                };
                if !filters_match(&note.frontmatter, &query.filters) {
                    continue;
                }
                hit.frontmatter = note.frontmatter;
                if hit.snippet.is_empty() {
                    hit.snippet = note.content.chars().take(480).collect();
                }
            }
            hits.push(hit);
            if hits.len() >= query.normalized_limit() as usize {
                break;
            }
        }
        hits.truncate(query.normalized_limit() as usize);
        Ok(hits)
    }

    async fn get(&self, path: &str) -> Result<KbNote, KbError> {
        validate_relative_path(path)?;
        let mut params = vec![("path".to_owned(), normalize_relative_path(path)?)];
        if let Some(vault) = self.vault.as_deref() {
            params.push(("vault".to_owned(), vault.to_owned()));
        }
        let value = self.get_json("api/note", &params).await?;
        note_from_value(&value, self.vault.as_deref().unwrap_or_default(), path)
    }

    async fn backlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        self.link_paths("api/note/backlinks", path).await
    }

    async fn outlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        self.link_paths("api/note/outlinks", path).await
    }

    async fn write(&self, path: &str, content: &str) -> Result<(), KbError> {
        validate_relative_path(path)?;
        if content.len() > MAX_NOTE_BYTES || contains_invalid_note_control(content) {
            return Err(KbError::InvalidNote);
        }
        let mut body = Map::new();
        body.insert("path".to_owned(), Value::String(path.replace('\\', "/")));
        body.insert("content".to_owned(), Value::String(content.to_owned()));
        if let Some(vault) = self.vault.as_deref() {
            body.insert("vault".to_owned(), Value::String(vault.to_owned()));
        }
        let response = self
            .request(reqwest::Method::POST, "api/note")
            .json(&body)
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        ensure_success(&response)?;
        let _ = self.events.send(ChangeEvent {
            kind: ChangeKind::Modified,
            vault: self.vault.clone().unwrap_or_default(),
            path: path.replace('\\', "/"),
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
        validate_relative_path(path)?;
        if fields
            .keys()
            .any(|key| key.is_empty() || key.len() > 128 || key.chars().any(char::is_control))
            || fields
                .values()
                .any(|value| value.len() > 16 * 1024 || value.chars().any(char::is_control))
        {
            return Err(KbError::InvalidQuery);
        }
        let body = json!({
            "vault": self.vault,
            "path": path.replace('\\', "/"),
            "updates": fields,
            "remove": Vec::<String>::new(),
        });
        let response = self
            .request(reqwest::Method::PATCH, "api/note/frontmatter")
            .json(&body)
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        ensure_success(&response)
    }

    async fn list_notes(
        &self,
        path_prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<KbNote>, KbError> {
        let limit = limit.clamp(1, MAX_INDEX_NOTES);
        let prefix = path_prefix.map(normalize_relative_path).transpose()?;
        let mut notes = Vec::new();
        let mut page = 1_u32;
        while notes.len() < limit as usize {
            let remaining = limit.saturating_sub(notes.len().try_into().unwrap_or(u32::MAX));
            let page_size = remaining.min(INDEX_PAGE_SIZE);
            let mut params = vec![
                ("keyword".to_owned(), String::new()),
                ("searchContent".to_owned(), "true".to_owned()),
                ("searchMode".to_owned(), "path".to_owned()),
                ("page".to_owned(), page.to_string()),
                ("pageSize".to_owned(), page_size.to_string()),
            ];
            if let Some(vault) = self.vault.as_deref() {
                params.push(("vault".to_owned(), vault.to_owned()));
            }
            if let Some(prefix) = prefix.as_deref() {
                params.push(("path".to_owned(), prefix.to_owned()));
            }
            let rows = array_from_response(self.get_json("api/notes", &params).await?)
                .ok_or(KbError::InvalidResponse)?;
            if rows.is_empty() {
                break;
            }
            let row_count = rows.len();
            for row in rows {
                let Some(hit) = hit_from_value(&row, self.vault.as_deref().unwrap_or_default())
                else {
                    continue;
                };
                if prefix
                    .as_deref()
                    .is_some_and(|value| !hit.path.starts_with(value))
                {
                    continue;
                }
                match self.get(&hit.path).await {
                    Ok(note) => notes.push(note),
                    Err(KbError::NotFound) => {}
                    Err(error) => return Err(error),
                }
                if notes.len() >= limit as usize {
                    break;
                }
            }
            if row_count < page_size as usize {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(notes)
    }

    fn watch(&self) -> Option<broadcast::Receiver<ChangeEvent>> {
        Some(self.events.subscribe())
    }
}

impl FnsProvider {
    async fn link_paths(&self, endpoint: &str, path: &str) -> Result<Vec<String>, KbError> {
        validate_relative_path(path)?;
        let mut params = vec![("path".to_owned(), normalize_relative_path(path)?)];
        if let Some(vault) = self.vault.as_deref() {
            params.push(("vault".to_owned(), vault.to_owned()));
        }
        let value = self.get_json(endpoint, &params).await?;
        let rows = array_from_response(value).ok_or(KbError::InvalidResponse)?;
        let mut paths = rows
            .iter()
            .filter_map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.get("path").and_then(Value::as_str).map(str::to_owned))
            })
            .filter_map(|value| normalize_relative_path(&value).ok())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn search_mode_text(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Path => "path",
        SearchMode::Content => "content",
        SearchMode::Regex => "regex",
    }
}

fn array_from_response(value: Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(rows) => Some(rows),
        Value::Object(mut object) => {
            for key in ["data", "list", "notes", "items", "results", "links"] {
                if let Some(value) = object.remove(key) {
                    if let Some(rows) = array_from_response(value) {
                        return Some(rows);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn hit_from_value(value: &Value, default_vault: &str) -> Option<KbHit> {
    let object = value.as_object()?;
    let path =
        normalize_relative_path(&string_field(object, &["path", "file", "filename"])?).ok()?;
    let title = string_field(object, &["title", "name"]).unwrap_or_else(|| {
        std::path::Path::new(&path)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(&path)
            .to_owned()
    });
    let snippet = string_field(object, &["snippet", "content", "text"]).unwrap_or_default();
    let frontmatter = map_field(object, &["frontmatter", "metadata"]).unwrap_or_default();
    let score = object
        .get("score")
        .and_then(Value::as_f64)
        .map_or(0.0, |value| value.to_string().parse::<f32>().unwrap_or(0.0));
    Some(KbHit {
        vault: string_field(object, &["vault"]).unwrap_or_else(|| default_vault.to_owned()),
        path,
        title,
        snippet,
        score,
        frontmatter,
    })
}

fn note_from_value(
    value: &Value,
    default_vault: &str,
    fallback_path: &str,
) -> Result<KbNote, KbError> {
    let root = value.as_object().ok_or(KbError::InvalidResponse)?;
    let value = root.get("data").unwrap_or(value);
    let object = value.as_object().ok_or(KbError::InvalidResponse)?;
    let path = normalize_relative_path(
        &string_field(object, &["path", "file"]).unwrap_or_else(|| fallback_path.to_owned()),
    )?;
    let content =
        string_field(object, &["content", "text", "body"]).ok_or(KbError::InvalidResponse)?;
    if content.len() > MAX_NOTE_BYTES || contains_invalid_note_control(&content) {
        return Err(KbError::TooLarge);
    }
    let mut note = crate::local::parse_markdown_note(default_vault, &path, &content)?;
    // The response may carry the server's own parsed frontmatter, which is the only source for
    // notes whose body has no YAML block. Values found inside the body still win, because the
    // body is the file itself.
    if let Some(mut frontmatter) = map_field(object, &["frontmatter", "metadata"]) {
        frontmatter.extend(std::mem::take(&mut note.frontmatter));
        note.frontmatter = frontmatter;
    }
    note.vault = string_field(root, &["vault"])
        .or_else(|| string_field(object, &["vault"]))
        .unwrap_or_else(|| default_vault.to_owned());
    if let Some(title) = string_field(object, &["title", "name"]) {
        note.title = title;
    }
    note.modified_at = object
        .get("mtime")
        .or_else(|| object.get("modifiedAt"))
        .or_else(|| object.get("lastTime"))
        .and_then(Value::as_i64);
    Ok(note)
}

fn string_field(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str).map(str::to_owned))
}

fn map_field(object: &Map<String, Value>, names: &[&str]) -> Option<Frontmatter> {
    let value = names
        .iter()
        .find_map(|name| object.get(*name))?
        .as_object()?;
    let mut result = Frontmatter::new();
    for (key, value) in value {
        if let Some(value) = value.as_str() {
            result.insert(key.clone(), value.to_owned());
        } else if let Some(value) = value.as_i64() {
            result.insert(key.clone(), value.to_string());
        } else if let Some(value) = value.as_bool() {
            result.insert(key.clone(), value.to_string());
        }
    }
    Some(result)
}

fn filters_match(frontmatter: &Frontmatter, filters: &Frontmatter) -> bool {
    filters
        .iter()
        .all(|(key, value)| frontmatter.get(key) == Some(value))
}

fn ensure_success(response: &Response) -> Result<(), KbError> {
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(map_status(status))
    }
}

fn map_status(status: StatusCode) -> KbError {
    if status == StatusCode::NOT_FOUND {
        KbError::NotFound
    } else {
        KbError::HttpStatus(status.as_u16())
    }
}

fn map_reqwest_error(error: &reqwest::Error) -> KbError {
    if error.is_timeout() {
        KbError::Timeout
    } else {
        KbError::Network
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_remote_and_local_fns_urls() {
        assert!(FnsProvider::new("https://fns.example/api", "token".to_owned(), None).is_ok());
        assert!(FnsProvider::new("http://127.0.0.1:9000/api", "token".to_owned(), None).is_ok());
        assert!(FnsProvider::new("http://fns.example", "token".to_owned(), None).is_err());
        assert!(FnsProvider::new("https://fns.example", String::new(), None).is_err());
    }

    #[test]
    fn parses_common_fns_shapes_without_credentials() {
        let value = json!({"path": "公文/a.md", "title": "测试", "content": "正文", "frontmatter": {"year": 2026}});
        let note = note_from_value(&value, "vault", "fallback.md").expect("note should parse");
        assert_eq!(note.path, "公文/a.md");
        assert_eq!(note.frontmatter.get("year"), Some(&"2026".to_owned()));
    }
}
