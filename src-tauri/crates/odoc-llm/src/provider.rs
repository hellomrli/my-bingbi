use crate::decoder::{LineDecoder, SseDecoder};
use crate::error::map_reqwest_error;
use crate::{
    Capabilities, ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole, LlmError,
    ProviderKind, ProviderProfile, TokenUsage,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use url::Url;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Capabilities;

    /// Sends a non-streamed chat request.
    ///
    /// # Errors
    ///
    /// Returns a stable, redacted [`LlmError`] for validation, security, network, HTTP, timeout,
    /// and response parsing failures.
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;

    /// Streams chat deltas into `sender` until the provider reports completion.
    ///
    /// Closing the receiving side cancels an in-flight response at the next network yield.
    ///
    /// # Errors
    ///
    /// Returns a stable, redacted [`LlmError`] for validation, security, cancellation, network,
    /// HTTP, timeout, and stream parsing failures.
    async fn chat_stream(
        &self,
        request: ChatRequest,
        sender: Sender<ChatDelta>,
    ) -> Result<(), LlmError>;

    /// Embeds texts into vectors for semantic retrieval, when the provider supports it.
    ///
    /// Callers treat a [`LlmError::Unsupported`] response as "no semantic re-ranking" and fall
    /// back to lexical fusion; they must never fail a search because embeddings are missing.
    ///
    /// # Errors
    ///
    /// Returns a stable, redacted [`LlmError`] for validation, network, HTTP, and response
    /// parsing failures, or [`LlmError::Unsupported`] when the provider cannot embed.
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        Err(LlmError::Unsupported)
    }

    /// Lists the model identifiers the upstream service currently exposes.
    ///
    /// The default implementation returns an empty list so lightweight test providers do not need
    /// to implement model discovery.
    ///
    /// # Errors
    ///
    /// Returns a stable, redacted [`LlmError`] for validation, network, HTTP, timeout, and response
    /// parsing failures.
    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        Ok(Vec::new())
    }
}

pub struct ProviderClient {
    profile: ProviderProfile,
    client: Client,
    api_key: Option<String>,
}

impl ProviderClient {
    /// Creates a client from validated, non-sensitive provider metadata and an optional secret.
    ///
    /// The secret is held only in this Rust object and is never part of a serializable type.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::InvalidConfiguration`] for invalid metadata or an HTTP client that
    /// cannot be constructed.
    pub fn new(profile: ProviderProfile, api_key: Option<String>) -> Result<Self, LlmError> {
        Self::with_timeout(profile, api_key, DEFAULT_REQUEST_TIMEOUT)
    }

    /// Creates a provider with a custom whole-request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::InvalidConfiguration`] for invalid metadata, a zero timeout, or an HTTP
    /// client that cannot be constructed.
    pub fn with_timeout(
        profile: ProviderProfile,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, LlmError> {
        profile.validate()?;
        if timeout.is_zero() || api_key.as_ref().is_some_and(String::is_empty) {
            return Err(LlmError::InvalidConfiguration);
        }
        let client = Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT.min(timeout))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("bingbi/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| LlmError::InvalidConfiguration)?;
        Ok(Self {
            profile,
            client,
            api_key,
        })
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn validate_request(&self, request: &ChatRequest) -> Result<(), LlmError> {
        self.profile.validate()?;
        if !self.profile.enabled {
            return Err(LlmError::ProviderDisabled);
        }
        if !request.security_level.allows_remote_provider() && !self.profile.is_local {
            return Err(LlmError::RemoteProviderBlocked);
        }
        request.validate()
    }

    fn chat_endpoint(&self) -> Result<Url, LlmError> {
        let base = self.base_url()?;
        let base_path = base.path().trim_end_matches('/');
        let endpoint = match self.profile.kind {
            ProviderKind::OpenAiCompatible if base_path.ends_with("/v1") => "chat/completions",
            ProviderKind::OpenAiCompatible => "v1/chat/completions",
            ProviderKind::Anthropic if base_path.ends_with("/v1") => "messages",
            ProviderKind::Anthropic => "v1/messages",
            ProviderKind::Ollama if base_path.ends_with("/api") => "chat",
            ProviderKind::Ollama => "api/chat",
        };
        base.join(endpoint)
            .map_err(|_| LlmError::InvalidConfiguration)
    }

    fn models_endpoint(&self) -> Result<Url, LlmError> {
        let base = self.base_url()?;
        let base_path = base.path().trim_end_matches('/');
        let endpoint = match self.profile.kind {
            ProviderKind::OpenAiCompatible | ProviderKind::Anthropic
                if base_path.ends_with("/v1") =>
            {
                "models"
            }
            ProviderKind::OpenAiCompatible | ProviderKind::Anthropic => "v1/models",
            ProviderKind::Ollama if base_path.ends_with("/api") => "tags",
            ProviderKind::Ollama => "api/tags",
        };
        base.join(endpoint)
            .map_err(|_| LlmError::InvalidConfiguration)
    }

    fn embeddings_endpoint(&self) -> Result<Url, LlmError> {
        let base = self.base_url()?;
        let base_path = base.path().trim_end_matches('/');
        let endpoint = if base_path.ends_with("/v1") {
            "embeddings"
        } else {
            "v1/embeddings"
        };
        base.join(endpoint)
            .map_err(|_| LlmError::InvalidConfiguration)
    }

    fn base_url(&self) -> Result<Url, LlmError> {
        Url::parse(&format!("{}/", self.profile.base_url.trim_end_matches('/')))
            .map_err(|_| LlmError::InvalidConfiguration)
    }

    fn authorize(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.profile.kind == ProviderKind::Anthropic {
            if let Some(api_key) = self.api_key.as_deref() {
                builder = builder.header("x-api-key", api_key);
            }
            builder.header("anthropic-version", "2023-06-01")
        } else {
            if let Some(api_key) = self.api_key.as_deref() {
                builder = builder.bearer_auth(api_key);
            }
            builder
        }
    }

    fn post(&self, body: &impl Serialize) -> Result<reqwest::RequestBuilder, LlmError> {
        Ok(self.authorize(self.client.post(self.chat_endpoint()?).json(body)))
    }

    async fn send(&self, body: &impl Serialize) -> Result<Response, LlmError> {
        let response = self
            .post(body)?
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        ensure_success(response)
    }

    async fn send_models(&self) -> Result<Response, LlmError> {
        let response = self
            .authorize(self.client.get(self.models_endpoint()?))
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        ensure_success(response)
    }

    async fn chat_openai(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = OpenAiRequest::new(&self.profile.model, request, false);
        let response = self.send(&body).await?;
        let response: OpenAiResponse = response
            .json()
            .await
            .map_err(|_| LlmError::InvalidResponse)?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or(LlmError::InvalidResponse)?;
        Ok(ChatResponse {
            content: choice.message.content,
            finish_reason: choice.finish_reason,
            usage: response.usage.map(OpenAiUsage::into_usage),
        })
    }

    async fn stream_openai(
        &self,
        request: ChatRequest,
        sender: Sender<ChatDelta>,
    ) -> Result<(), LlmError> {
        let body = OpenAiRequest::new(&self.profile.model, request, true);
        let response = self.send(&body).await?;
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut terminal = false;

        loop {
            tokio::select! {
                () = sender.closed() => return Err(LlmError::Cancelled),
                next = stream.next() => match next {
                    Some(Ok(chunk)) => {
                        for event in decoder.push(&chunk)? {
                            if process_openai_event(&event, &sender).await? {
                                return Ok(());
                            }
                            terminal |= openai_event_is_terminal(&event)?;
                        }
                    }
                    Some(Err(error)) => return Err(map_reqwest_error(&error)),
                    None => break,
                }
            }
        }
        for event in decoder.finish()? {
            if process_openai_event(&event, &sender).await? {
                return Ok(());
            }
            terminal |= openai_event_is_terminal(&event)?;
        }
        if terminal {
            Ok(())
        } else {
            Err(LlmError::InvalidResponse)
        }
    }

    async fn chat_ollama(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = OllamaRequest::new(&self.profile.model, request, false);
        let response = self.send(&body).await?;
        let response: OllamaResponse = response
            .json()
            .await
            .map_err(|_| LlmError::InvalidResponse)?;
        if !response.done {
            return Err(LlmError::InvalidResponse);
        }
        Ok(response.into_chat_response())
    }

    async fn stream_ollama(
        &self,
        request: ChatRequest,
        sender: Sender<ChatDelta>,
    ) -> Result<(), LlmError> {
        let body = OllamaRequest::new(&self.profile.model, request, true);
        let response = self.send(&body).await?;
        let mut stream = response.bytes_stream();
        let mut decoder = LineDecoder::default();

        loop {
            tokio::select! {
                () = sender.closed() => return Err(LlmError::Cancelled),
                next = stream.next() => match next {
                    Some(Ok(chunk)) => {
                        for line in decoder.push(&chunk)? {
                            if process_ollama_line(&line, &sender).await? {
                                return Ok(());
                            }
                        }
                    }
                    Some(Err(error)) => return Err(map_reqwest_error(&error)),
                    None => break,
                }
            }
        }
        for line in decoder.finish()? {
            if process_ollama_line(&line, &sender).await? {
                return Ok(());
            }
        }
        Err(LlmError::InvalidResponse)
    }

    async fn chat_anthropic(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = AnthropicRequest::new(&self.profile.model, request, false);
        let response = self.send(&body).await?;
        let response: AnthropicResponse = response
            .json()
            .await
            .map_err(|_| LlmError::InvalidResponse)?;
        Ok(response.into_chat_response())
    }

    async fn stream_anthropic(
        &self,
        request: ChatRequest,
        sender: Sender<ChatDelta>,
    ) -> Result<(), LlmError> {
        let body = AnthropicRequest::new(&self.profile.model, request, true);
        let response = self.send(&body).await?;
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut terminal = false;

        loop {
            tokio::select! {
                () = sender.closed() => return Err(LlmError::Cancelled),
                next = stream.next() => match next {
                    Some(Ok(chunk)) => {
                        for event in decoder.push(&chunk)? {
                            if process_anthropic_event(&event, &sender).await? {
                                return Ok(());
                            }
                            terminal |= anthropic_event_is_terminal(&event)?;
                        }
                    }
                    Some(Err(error)) => return Err(map_reqwest_error(&error)),
                    None => break,
                }
            }
        }
        for event in decoder.finish()? {
            if process_anthropic_event(&event, &sender).await? {
                return Ok(());
            }
            terminal |= anthropic_event_is_terminal(&event)?;
        }
        if terminal {
            Ok(())
        } else {
            Err(LlmError::InvalidResponse)
        }
    }

    async fn list_models_impl(&self) -> Result<Vec<String>, LlmError> {
        self.validate_profile()?;
        let response = self.send_models().await?;
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| LlmError::InvalidResponse)?;
        let entries = match self.profile.kind {
            ProviderKind::Ollama => body.get("models").and_then(serde_json::Value::as_array),
            ProviderKind::OpenAiCompatible | ProviderKind::Anthropic => {
                body.get("data").and_then(serde_json::Value::as_array)
            }
        };
        let Some(entries) = entries else {
            return Err(LlmError::InvalidResponse);
        };
        let mut models = Vec::new();
        for entry in entries {
            let key = if self.profile.kind == ProviderKind::Ollama {
                "name"
            } else {
                "id"
            };
            if let Some(name) = entry.get(key).and_then(serde_json::Value::as_str) {
                if !name.is_empty() && !models.iter().any(|model: &String| model == name) {
                    models.push(name.to_owned());
                }
            }
        }
        models.sort();
        Ok(models)
    }

    fn validate_profile(&self) -> Result<(), LlmError> {
        if !self.profile.enabled {
            return Err(LlmError::ProviderDisabled);
        }
        Ok(())
    }
}

#[async_trait]
impl LlmProvider for ProviderClient {
    fn id(&self) -> &str {
        &self.profile.id
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming: true,
            json_mode: true,
            tool_calling: matches!(
                self.profile.kind,
                ProviderKind::OpenAiCompatible | ProviderKind::Anthropic
            ),
            embeddings: false,
            context_window: None,
            is_local: self.profile.is_local,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.validate_request(&request)?;
        match self.profile.kind {
            ProviderKind::OpenAiCompatible => self.chat_openai(request).await,
            ProviderKind::Anthropic => self.chat_anthropic(request).await,
            ProviderKind::Ollama => self.chat_ollama(request).await,
        }
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        sender: Sender<ChatDelta>,
    ) -> Result<(), LlmError> {
        self.validate_request(&request)?;
        if sender.is_closed() {
            return Err(LlmError::Cancelled);
        }
        match self.profile.kind {
            ProviderKind::OpenAiCompatible => self.stream_openai(request, sender).await,
            ProviderKind::Anthropic => self.stream_anthropic(request, sender).await,
            ProviderKind::Ollama => self.stream_ollama(request, sender).await,
        }
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        if texts.is_empty() || texts.iter().any(String::is_empty) {
            return Err(LlmError::InvalidRequest);
        }
        if self.profile.kind != ProviderKind::OpenAiCompatible {
            return Err(LlmError::Unsupported);
        }
        let body = EmbeddingRequest {
            model: &self.profile.model,
            input: texts,
        };
        let response = self
            .authorize(self.client.post(self.embeddings_endpoint()?).json(&body))
            .send()
            .await
            .map_err(|error| map_reqwest_error(&error))?;
        let response: EmbeddingResponse = ensure_success(response)?
            .json()
            .await
            .map_err(|_| LlmError::InvalidResponse)?;
        let vectors = response
            .data
            .into_iter()
            .map(|entry| entry.embedding)
            .collect::<Vec<_>>();
        if vectors.len() != texts.len() {
            return Err(LlmError::InvalidResponse);
        }
        Ok(vectors)
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        self.list_models_impl().await
    }
}

fn ensure_success(response: Response) -> Result<Response, LlmError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        // Drop the body unread: compatible services sometimes echo credentials in diagnostics,
        // and a hostile endpoint could otherwise force an unbounded error-body allocation.
        Err(LlmError::HttpStatus(status.as_u16()))
    }
}

async fn send_delta(sender: &Sender<ChatDelta>, delta: ChatDelta) -> Result<(), LlmError> {
    if delta.content.is_empty() && delta.finish_reason.is_none() && delta.usage.is_none() {
        return Ok(());
    }
    sender.send(delta).await.map_err(|_| LlmError::Cancelled)
}

async fn process_openai_event(event: &str, sender: &Sender<ChatDelta>) -> Result<bool, LlmError> {
    if event.trim() == "[DONE]" {
        return Ok(true);
    }
    let chunk: OpenAiChunk = serde_json::from_str(event).map_err(|_| LlmError::InvalidResponse)?;
    let choice = chunk.choices.into_iter().next();
    let delta = ChatDelta {
        content: choice
            .as_ref()
            .and_then(|choice| choice.delta.content.clone())
            .unwrap_or_default(),
        finish_reason: choice.and_then(|choice| choice.finish_reason),
        usage: chunk.usage.map(OpenAiUsage::into_usage),
    };
    send_delta(sender, delta).await?;
    Ok(false)
}

fn openai_event_is_terminal(event: &str) -> Result<bool, LlmError> {
    if event.trim() == "[DONE]" {
        return Ok(true);
    }
    let chunk: OpenAiChunk = serde_json::from_str(event).map_err(|_| LlmError::InvalidResponse)?;
    Ok(chunk
        .choices
        .iter()
        .any(|choice| choice.finish_reason.is_some()))
}

async fn process_ollama_line(line: &str, sender: &Sender<ChatDelta>) -> Result<bool, LlmError> {
    let response: OllamaResponse =
        serde_json::from_str(line).map_err(|_| LlmError::InvalidResponse)?;
    let done = response.done;
    send_delta(sender, response.into_delta()).await?;
    Ok(done)
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingEntry>,
}

#[derive(Deserialize)]
struct EmbeddingEntry {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

impl<'a> OpenAiRequest<'a> {
    fn new(model: &'a str, request: ChatRequest, stream: bool) -> Self {
        Self {
            model,
            messages: request.messages,
            stream,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_format: request.json_mode.then_some(ResponseFormat {
                kind: "json_object",
            }),
        }
    }
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChunk {
    #[serde(default)]
    choices: Vec<OpenAiChunkChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(rename = "prompt_tokens")]
    prompt: u32,
    #[serde(rename = "completion_tokens")]
    completion: u32,
    #[serde(rename = "total_tokens")]
    total: u32,
}

impl OpenAiUsage {
    const fn into_usage(self) -> TokenUsage {
        TokenUsage {
            prompt_tokens: self.prompt,
            completion_tokens: self.completion,
            total_tokens: self.total,
        }
    }
}

#[derive(Deserialize)]
struct OpenAiChunkChoice {
    delta: OpenAiChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiChunkDelta {
    content: Option<String>,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'static str>,
    #[serde(skip_serializing_if = "OllamaOptions::is_empty")]
    options: OllamaOptions,
}

impl<'a> OllamaRequest<'a> {
    fn new(model: &'a str, request: ChatRequest, stream: bool) -> Self {
        Self {
            model,
            messages: request.messages,
            stream,
            format: request.json_mode.then_some("json"),
            options: OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            },
        }
    }
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

impl OllamaOptions {
    const fn is_empty(&self) -> bool {
        self.temperature.is_none() && self.num_predict.is_none()
    }
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    done_reason: Option<String>,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

impl OllamaResponse {
    fn into_chat_response(self) -> ChatResponse {
        let usage = self.usage();
        ChatResponse {
            content: self.message.content,
            finish_reason: self.done_reason,
            usage,
        }
    }

    fn into_delta(self) -> ChatDelta {
        let usage = self.usage();
        ChatDelta {
            content: self.message.content,
            finish_reason: self.done_reason,
            usage,
        }
    }

    fn usage(&self) -> Option<TokenUsage> {
        match (self.prompt_eval_count, self.eval_count) {
            (None, None) => None,
            (prompt, completion) => {
                let prompt_tokens = prompt.unwrap_or_default();
                let completion_tokens = completion.unwrap_or_default();
                Some(TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens: prompt_tokens.saturating_add(completion_tokens),
                })
            }
        }
    }
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

const ANTHROPIC_JSON_MODE_NOTE: &str = "\n只输出合法的 JSON，不要输出任何解释、前缀或前后缀文本。";

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    stream: bool,
}

impl<'a> AnthropicRequest<'a> {
    fn new(model: &'a str, request: ChatRequest, stream: bool) -> Self {
        let mut system = request
            .messages
            .iter()
            .filter(|message| message.role == ChatRole::System)
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        if request.json_mode {
            system.push_str(ANTHROPIC_JSON_MODE_NOTE);
        }
        let messages = request
            .messages
            .into_iter()
            .filter(|message| message.role != ChatRole::System)
            .map(|message| AnthropicMessage {
                role: match message.role {
                    ChatRole::System => unreachable!("system messages were filtered out"),
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                },
                content: message.content,
            })
            .collect();
        Self {
            model,
            // The Anthropic Messages API requires max_tokens and rejects sampling parameters
            // (temperature, top_p) on current models, so neither is forwarded here.
            max_tokens: request.max_tokens.unwrap_or(8192),
            system: (!system.is_empty()).then_some(system),
            messages,
            stream,
        }
    }
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

impl AnthropicResponse {
    fn into_chat_response(self) -> ChatResponse {
        ChatResponse {
            content: self
                .content
                .into_iter()
                .map(|block| block.text)
                .collect::<String>(),
            finish_reason: self.stop_reason,
            usage: self.usage.map(AnthropicUsage::into_usage),
        }
    }
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    #[serde(rename = "input_tokens")]
    input: u32,
    #[serde(rename = "output_tokens")]
    output: u32,
}

impl AnthropicUsage {
    const fn into_usage(self) -> TokenUsage {
        TokenUsage {
            prompt_tokens: self.input,
            completion_tokens: self.output,
            total_tokens: self.input.saturating_add(self.output),
        }
    }
}

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<AnthropicStreamDelta>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    stop_reason: Option<String>,
}

fn anthropic_event_is_terminal(event: &str) -> Result<bool, LlmError> {
    let event: AnthropicStreamEvent =
        serde_json::from_str(event).map_err(|_| LlmError::InvalidResponse)?;
    Ok(event.kind == "message_stop")
}

async fn process_anthropic_event(
    event: &str,
    sender: &Sender<ChatDelta>,
) -> Result<bool, LlmError> {
    let event: AnthropicStreamEvent =
        serde_json::from_str(event).map_err(|_| LlmError::InvalidResponse)?;
    let delta = match event.kind.as_str() {
        "message_stop" => return Ok(true),
        "content_block_delta" => {
            let text = event
                .delta
                .as_ref()
                .filter(|delta| delta.kind.as_deref() == Some("text_delta"))
                .and_then(|delta| delta.text.clone())
                .unwrap_or_default();
            ChatDelta {
                content: text,
                finish_reason: None,
                usage: None,
            }
        }
        "message_delta" => ChatDelta {
            content: String::new(),
            finish_reason: event
                .delta
                .as_ref()
                .and_then(|delta| delta.stop_reason.clone()),
            usage: event.usage.map(AnthropicUsage::into_usage),
        },
        // message_start, content_block_start, content_block_stop, ping, and any future
        // event types carry no user-visible text.
        _ => return Ok(false),
    };
    send_delta(sender, delta).await?;
    Ok(false)
}
