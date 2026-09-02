use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use ts_rs::TS;
use url::Url;

pub const KB_CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_SEARCH_LIMIT: u32 = 20;
pub const MAX_SEARCH_LIMIT: u32 = 100;
pub const MAX_NOTE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_QUERY_BYTES: usize = 16 * 1024;

pub type Frontmatter = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum KbProviderKind {
    #[default]
    Fns,
    LocalVault,
    Mcp,
}

/// Non-sensitive knowledge-base settings. FNS credentials are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "settings record mirroring independent user toggles, not a state machine"
)]
pub struct KbConfig {
    pub schema_version: u32,
    pub enabled: bool,
    pub provider_kind: KbProviderKind,
    pub base_url: String,
    pub vault: String,
    pub local_path: String,
    #[serde(default)]
    pub index_enabled: bool,
    #[serde(default)]
    pub auto_tagging_enabled: bool,
    #[serde(default)]
    pub sync_enabled: bool,
    #[serde(default)]
    pub mcp_url: String,
}

impl Default for KbConfig {
    fn default() -> Self {
        Self {
            schema_version: KB_CONFIG_SCHEMA_VERSION,
            enabled: false,
            provider_kind: KbProviderKind::Fns,
            base_url: "http://127.0.0.1:9000".to_owned(),
            vault: String::new(),
            local_path: String::new(),
            index_enabled: true,
            auto_tagging_enabled: false,
            sync_enabled: true,
            mcp_url: String::new(),
        }
    }
}

impl KbConfig {
    /// Trims user-entered metadata and validates the non-sensitive provider settings.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] for unsupported schemas, unsafe URLs, missing
    /// enabled-provider fields, relative local paths, or control characters.
    pub fn normalized(mut self) -> Result<Self, KbError> {
        self.base_url = self.base_url.trim().trim_end_matches('/').to_owned();
        self.vault = self.vault.trim().to_owned();
        self.local_path = self.local_path.trim().to_owned();
        self.mcp_url = self.mcp_url.trim().trim_end_matches('/').to_owned();
        self.validate()?;
        Ok(self)
    }

    /// Validates the provider-specific fields without mutating them.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] when enabled settings are incomplete or unsafe.
    pub fn validate(&self) -> Result<(), KbError> {
        let common_valid = self.schema_version == KB_CONFIG_SCHEMA_VERSION
            && self.base_url.len() <= 2048
            && self.vault.len() <= 256
            && self.local_path.len() <= 4096
            && self.mcp_url.len() <= 2048
            && !self.base_url.chars().any(char::is_control)
            && !self.vault.chars().any(char::is_control)
            && !self.local_path.chars().any(char::is_control)
            && !self.mcp_url.chars().any(char::is_control);
        if !common_valid {
            return Err(KbError::InvalidConfiguration);
        }
        if !self.enabled {
            return Ok(());
        }
        if self.vault.is_empty() {
            return Err(KbError::InvalidConfiguration);
        }
        match self.provider_kind {
            KbProviderKind::Fns => parse_fns_base_url(&self.base_url).map(|_| ()),
            KbProviderKind::LocalVault => {
                let path = Path::new(&self.local_path);
                if self.local_path.is_empty() || !path.is_absolute() {
                    Err(KbError::InvalidConfiguration)
                } else {
                    Ok(())
                }
            }
            KbProviderKind::Mcp => parse_remote_or_local_url(&self.mcp_url).map(|_| ()),
        }
    }

    /// Reports whether all configured knowledge-base I/O remains on this machine.
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self.provider_kind {
            KbProviderKind::LocalVault => true,
            KbProviderKind::Mcp => parse_remote_or_local_url(&self.mcp_url)
                .ok()
                .and_then(|url| url.host_str().map(is_local_host))
                .unwrap_or(false),
            KbProviderKind::Fns => parse_fns_base_url(&self.base_url)
                .ok()
                .and_then(|url| url.host_str().map(is_local_host))
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum SearchMode {
    Path,
    #[default]
    Content,
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KbQuery {
    pub vault: Option<String>,
    pub keyword: String,
    #[serde(default)]
    pub search_content: bool,
    #[serde(default)]
    pub search_mode: SearchMode,
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub filters: Frontmatter,
    #[serde(default)]
    #[ts(type = "number")]
    pub limit: u32,
}

impl Default for KbQuery {
    fn default() -> Self {
        Self {
            vault: None,
            keyword: String::new(),
            search_content: true,
            search_mode: SearchMode::Content,
            path_prefix: None,
            filters: Frontmatter::new(),
            limit: DEFAULT_SEARCH_LIMIT,
        }
    }
}

impl KbQuery {
    pub fn normalized_limit(&self) -> u32 {
        if self.limit == 0 {
            DEFAULT_SEARCH_LIMIT
        } else {
            self.limit.min(MAX_SEARCH_LIMIT)
        }
    }

    /// Validates query size, path and filter constraints before a provider performs I/O.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidQuery`] or [`KbError::InvalidPath`] for malformed or unsafe
    /// query values.
    pub fn validate(&self) -> Result<(), KbError> {
        if self.keyword.len() > MAX_QUERY_BYTES
            || self.keyword.chars().any(char::is_control)
            || self
                .vault
                .as_deref()
                .is_some_and(|value| value.len() > 256 || value.chars().any(char::is_control))
        {
            return Err(KbError::InvalidQuery);
        }
        if let Some(prefix) = self.path_prefix.as_deref() {
            validate_relative_path(prefix)?;
        }
        for (key, value) in &self.filters {
            if key.is_empty()
                || key.len() > 128
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
            {
                return Err(KbError::InvalidQuery);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KbHit {
    pub vault: String,
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    pub frontmatter: Frontmatter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KbNote {
    pub vault: String,
    pub path: String,
    pub title: String,
    pub content: String,
    pub frontmatter: Frontmatter,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    #[ts(type = "number | null")]
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChangeEvent {
    pub kind: ChangeKind,
    pub vault: String,
    pub path: String,
    pub old_path: Option<String>,
    #[ts(type = "number | null")]
    pub cursor: Option<i64>,
}

#[derive(Debug, Error)]
pub enum KbError {
    #[error("knowledge-base provider configuration is invalid")]
    InvalidConfiguration,
    #[error("knowledge-base query is invalid")]
    InvalidQuery,
    #[error("knowledge-base path is invalid")]
    InvalidPath,
    #[error("knowledge-base note was not found")]
    NotFound,
    #[error("knowledge-base note is too large")]
    TooLarge,
    #[error("knowledge-base network request failed")]
    Network,
    #[error("knowledge-base request timed out")]
    Timeout,
    #[error("knowledge-base service returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("knowledge-base response is invalid")]
    InvalidResponse,
    #[error("knowledge-base markdown/frontmatter is invalid")]
    InvalidNote,
    #[error("knowledge-base filesystem operation failed")]
    Io(#[source] std::io::Error),
    #[error("knowledge-base resource is unavailable")]
    Unsupported,
    #[error("knowledge-base provider returned a truncated listing")]
    ListingTruncated,
    #[error("knowledge-base sync change was not persisted")]
    SyncRejected,
}

impl KbError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "kb_invalid_config",
            Self::InvalidQuery | Self::InvalidPath | Self::InvalidNote => "kb_invalid_input",
            Self::NotFound => "kb_note_not_found",
            Self::TooLarge => "kb_note_too_large",
            Self::Network => "kb_network_error",
            Self::Timeout => "kb_timeout",
            Self::HttpStatus(_) => "kb_http_error",
            Self::InvalidResponse => "kb_invalid_response",
            Self::Io(_) | Self::Unsupported => "kb_unavailable",
            Self::ListingTruncated => "kb_listing_truncated",
            Self::SyncRejected => "kb_sync_rejected",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "知识库配置无效。",
            Self::InvalidQuery | Self::InvalidPath | Self::InvalidNote => {
                "知识库查询或笔记路径格式无效。"
            }
            Self::NotFound => "没有找到指定的知识库笔记。",
            Self::TooLarge => "知识库笔记或响应超过大小限制。",
            Self::Network => "无法连接知识库服务。",
            Self::Timeout => "知识库服务响应超时。",
            Self::HttpStatus(401 | 403) => "知识库服务拒绝了凭据，请检查 FNS token。",
            Self::HttpStatus(_) => "知识库服务返回错误状态。",
            Self::InvalidResponse => "知识库服务返回了无法识别的数据。",
            Self::Io(_) | Self::Unsupported => "知识库资源暂不可用。",
            Self::ListingTruncated => {
                "MCP 服务最多返回 100 篇笔记，无法完成全量索引；请改用 FNS REST 或本地 Vault。"
            }
            Self::SyncRejected => "知识库变更未能写入本地索引，同步游标不会推进。",
        }
    }
}

pub(crate) fn parse_fns_base_url(value: &str) -> Result<Url, KbError> {
    parse_remote_or_local_url(value)
}

pub(crate) fn parse_remote_or_local_url(value: &str) -> Result<Url, KbError> {
    let url = Url::parse(value.trim_end_matches('/')).map_err(|_| KbError::InvalidConfiguration)?;
    let host = url
        .host_str()
        .ok_or(KbError::InvalidConfiguration)?
        .to_ascii_lowercase();
    let local = is_local_host(&host);
    if !((url.scheme() == "https") || (url.scheme() == "http" && local))
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_some_and(|value| value.len() > 253)
    {
        return Err(KbError::InvalidConfiguration);
    }
    Ok(url)
}

fn is_local_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || matches!(host.as_str(), "::1" | "[::1]")
}

/// Whether `path` lives under `prefix` on a directory boundary. A bare `starts_with` would
/// let the prefix "公文" also match "公文备份/x.md", so the boundary slash is required.
pub(crate) fn path_matches_prefix(path: &str, prefix: Option<&str>) -> bool {
    let Some(prefix) = prefix else {
        return true;
    };
    if prefix.is_empty() {
        return true;
    }
    let path = path.replace('\\', "/");
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

pub(crate) fn validate_relative_path(value: &str) -> Result<(), KbError> {
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(KbError::InvalidPath);
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(KbError::InvalidPath);
    }
    Ok(())
}

pub(crate) fn normalize_relative_path(value: &str) -> Result<String, KbError> {
    let normalized = value.replace('\\', "/");
    validate_relative_path(&normalized)?;
    Ok(normalized)
}

pub(crate) fn contains_invalid_note_control(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}
