use crate::LlmError;
use odoc_model::SecurityLevel;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;
use ts_rs::TS;
use url::{Host, Url};

const MAX_PROVIDER_ID_BYTES: usize = 64;
const MAX_LABEL_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_CHAT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ProviderKind {
    #[serde(rename = "openai_compatible")]
    #[ts(rename = "openai_compatible")]
    OpenAiCompatible,
    #[serde(rename = "anthropic")]
    #[ts(rename = "anthropic")]
    Anthropic,
    Ollama,
}

impl ProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
        }
    }
}

impl FromStr for ProviderKind {
    type Err = LlmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "openai_compatible" => Ok(Self::OpenAiCompatible),
            "anthropic" => Ok(Self::Anthropic),
            "ollama" => Ok(Self::Ollama),
            _ => Err(LlmError::InvalidConfiguration),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ProviderProfileInput {
    pub id: String,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    pub is_local: bool,
}

impl TryFrom<ProviderProfileInput> for ProviderProfile {
    type Error = LlmError;

    fn try_from(input: ProviderProfileInput) -> Result<Self, Self::Error> {
        validate_provider_id(&input.id)?;
        validate_label(&input.display_name)?;
        validate_model(&input.model)?;
        let url = validated_base_url(&input.base_url)?;
        Ok(Self {
            id: input.id,
            kind: input.kind,
            display_name: input.display_name.trim().to_owned(),
            base_url: url.as_str().trim_end_matches('/').to_owned(),
            model: input.model.trim().to_owned(),
            enabled: input.enabled,
            is_local: url_is_local(&url),
        })
    }
}

impl ProviderProfile {
    /// Revalidates persisted provider metadata before it is used for a network request.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::InvalidConfiguration`] when any field or the computed locality flag is
    /// invalid.
    pub fn validate(&self) -> Result<(), LlmError> {
        validate_provider_id(&self.id)?;
        validate_label(&self.display_name)?;
        validate_model(&self.model)?;
        let url = validated_base_url(&self.base_url)?;
        if self.is_local != url_is_local(&url) {
            return Err(LlmError::InvalidConfiguration);
        }
        Ok(())
    }

    pub fn input(&self) -> ProviderProfileInput {
        ProviderProfileInput {
            id: self.id.clone(),
            kind: self.kind,
            display_name: self.display_name.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            enabled: self.enabled,
        }
    }
}

pub fn builtin_provider_profiles() -> Vec<ProviderProfile> {
    vec![
        ProviderProfile {
            id: "openai-compatible".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "OpenAI 兼容".to_owned(),
            base_url: "https://api.openai.com".to_owned(),
            model: "gpt-4.1-mini".to_owned(),
            enabled: true,
            is_local: false,
        },
        ProviderProfile {
            id: "deepseek".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "DeepSeek".to_owned(),
            base_url: "https://api.deepseek.com".to_owned(),
            model: "deepseek-chat".to_owned(),
            enabled: true,
            is_local: false,
        },
        ProviderProfile {
            id: "anthropic".to_owned(),
            kind: ProviderKind::Anthropic,
            display_name: "Claude (Anthropic)".to_owned(),
            base_url: "https://api.anthropic.com".to_owned(),
            model: "claude-opus-5".to_owned(),
            enabled: true,
            is_local: false,
        },
        ProviderProfile {
            id: "cline-pass".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Cline Pass".to_owned(),
            base_url: "https://api.cline.bot/api/v1".to_owned(),
            model: "cline-pass/deepseek-v4.1-flash".to_owned(),
            enabled: false,
            is_local: false,
        },
        ProviderProfile {
            id: "ollama".to_owned(),
            kind: ProviderKind::Ollama,
            display_name: "Ollama".to_owned(),
            base_url: "http://127.0.0.1:11434".to_owned(),
            model: "qwen3:8b".to_owned(),
            enabled: true,
            is_local: true,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub json_mode: bool,
    pub security_level: SecurityLevel,
}

impl ChatRequest {
    /// Validates request size and sampling parameters before serialization or network access.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::InvalidRequest`] for empty or oversized input and invalid numeric
    /// parameters.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.messages.is_empty()
            || self
                .messages
                .iter()
                .any(|message| message.content.is_empty())
            || self
                .messages
                .iter()
                .map(|message| message.content.len())
                .sum::<usize>()
                > MAX_CHAT_BYTES
            || self.max_tokens == Some(0)
            || self
                .temperature
                .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
        {
            return Err(LlmError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChatResponse {
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChatDelta {
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Capabilities {
    pub streaming: bool,
    pub json_mode: bool,
    pub tool_calling: bool,
    pub embeddings: bool,
    pub context_window: Option<u32>,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub provider_id: String,
    pub model: String,
    pub latency_ms: u32,
    pub capabilities: Capabilities,
}

pub(crate) fn validated_base_url(value: &str) -> Result<Url, LlmError> {
    let url = Url::parse(value.trim()).map_err(|_| LlmError::InvalidConfiguration)?;
    let secure_transport =
        url.scheme() == "https" || (url.scheme() == "http" && url_is_local(&url));
    let valid = secure_transport
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if valid {
        Ok(url)
    } else {
        Err(LlmError::InvalidConfiguration)
    }
}

pub(crate) fn url_is_local(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost")
                || host.to_ascii_lowercase().ends_with(".localhost")
        }
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

fn validate_provider_id(value: &str) -> Result<(), LlmError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_PROVIDER_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(LlmError::InvalidConfiguration)
    }
}

fn validate_label(value: &str) -> Result<(), LlmError> {
    if value.trim().is_empty()
        || value.len() > MAX_LABEL_BYTES
        || value.chars().any(char::is_control)
    {
        Err(LlmError::InvalidConfiguration)
    } else {
        Ok(())
    }
}

fn validate_model(value: &str) -> Result<(), LlmError> {
    if value.trim().is_empty()
        || value.len() > MAX_MODEL_BYTES
        || value.chars().any(char::is_control)
    {
        Err(LlmError::InvalidConfiguration)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_locality_is_computed_from_the_url() {
        let local = ProviderProfile::try_from(ProviderProfileInput {
            id: "local".to_owned(),
            kind: ProviderKind::Ollama,
            display_name: "Local".to_owned(),
            base_url: "http://[::1]:11434/".to_owned(),
            model: "qwen3".to_owned(),
            enabled: true,
        })
        .expect("profile should be valid");
        assert!(local.is_local);
        assert_eq!(local.base_url, "http://[::1]:11434");

        let remote = ProviderProfile::try_from(ProviderProfileInput {
            base_url: "https://models.example.com".to_owned(),
            ..local.input()
        })
        .expect("profile should be valid");
        assert!(!remote.is_local);
    }

    #[test]
    fn profile_rejects_credentials_in_base_url() {
        let input = ProviderProfileInput {
            id: "remote".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Remote".to_owned(),
            base_url: "https://user:secret@example.com".to_owned(),
            model: "model".to_owned(),
            enabled: true,
        };
        assert_eq!(
            ProviderProfile::try_from(input),
            Err(LlmError::InvalidConfiguration)
        );
    }

    #[test]
    fn profile_requires_https_for_remote_services() {
        let input = ProviderProfileInput {
            id: "remote".to_owned(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Remote".to_owned(),
            base_url: "http://models.example.com".to_owned(),
            model: "model".to_owned(),
            enabled: true,
        };
        assert_eq!(
            ProviderProfile::try_from(input),
            Err(LlmError::InvalidConfiguration)
        );
    }

    #[test]
    fn chat_request_validation_rejects_invalid_sampling() {
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "hello".to_owned(),
            }],
            temperature: Some(f32::NAN),
            max_tokens: None,
            json_mode: false,
            security_level: SecurityLevel::Internal,
        };
        assert_eq!(request.validate(), Err(LlmError::InvalidRequest));
    }
}
