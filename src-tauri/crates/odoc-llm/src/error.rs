use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LlmError {
    #[error("the provider configuration is invalid")]
    InvalidConfiguration,
    #[error("the chat request is invalid")]
    InvalidRequest,
    #[error("the selected provider is disabled")]
    ProviderDisabled,
    #[error("the document security level forbids this remote provider")]
    RemoteProviderBlocked,
    #[error("the model request timed out")]
    Timeout,
    #[error("the model request was cancelled")]
    Cancelled,
    #[error("the model service could not be reached")]
    Network,
    #[error("the model service returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("the model service returned an unsupported response")]
    InvalidResponse,
}

impl LlmError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_provider_config",
            Self::InvalidRequest => "invalid_chat_request",
            Self::ProviderDisabled => "provider_disabled",
            Self::RemoteProviderBlocked => "remote_provider_blocked",
            Self::Timeout => "llm_timeout",
            Self::Cancelled => "llm_cancelled",
            Self::Network => "llm_network_error",
            Self::HttpStatus(_) => "llm_http_error",
            Self::InvalidResponse => "llm_invalid_response",
        }
    }

    pub const fn user_message(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "模型服务配置无效，请检查服务地址和模型名称。",
            Self::InvalidRequest => "发送给模型的内容格式无效。",
            Self::ProviderDisabled => "所选模型服务已停用。",
            Self::RemoteProviderBlocked => "当前公文密级禁止使用远程模型服务。",
            Self::Timeout => "模型服务响应超时。",
            Self::Cancelled => "模型请求已取消。",
            Self::Network => "无法连接模型服务，请检查地址和网络。",
            Self::HttpStatus(401 | 403) => "模型服务拒绝了凭据，请检查 API Key。",
            Self::HttpStatus(404) => "模型服务接口或模型不存在。",
            Self::HttpStatus(429) => "模型服务请求过于频繁，请稍后重试。",
            Self::HttpStatus(_) => "模型服务返回错误。",
            Self::InvalidResponse => "模型服务返回了无法识别的数据。",
        }
    }
}

pub(crate) fn map_reqwest_error(error: &reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Network
    }
}
