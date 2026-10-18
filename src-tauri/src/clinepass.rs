//! Cline Pass subscription overview: quota windows, model catalog, and the subscription page.
//!
//! Cline Pass is an OpenAI-compatible gateway (`https://api.cline.bot/api/v1`) that fronts a
//! large set of Chinese models. The API key stays in the system keyring; the webview only ever
//! receives plan quotas and model metadata.

use crate::error::IpcError;
use crate::state::AppState;
use crate::util::provider_secret_account;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::State;

const PROVIDER_ID: &str = "cline-pass";
const BASE_URL: &str = "https://api.cline.bot/api/v1";
const RECOMMENDED_URL: &str = "https://api.cline.bot/api/v1/ai/cline/recommended-models";
const SUBSCRIPTION_URL: &str = "https://cline.bot/cline-pass";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// One quota window reported by the gateway (`five_hour`, `weekly`, `monthly`, ...).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaLimit {
    pub(crate) kind: String,
    pub(crate) percent_used: f64,
    pub(crate) resets_at: Option<String>,
}

/// One selectable Cline Pass model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClinePassModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) context_window: Option<u64>,
    /// Offered by the official subscription list (public endpoint).
    pub(crate) recommended: bool,
    /// Present in the gateway's authenticated `/models` response.
    pub(crate) gateway: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClinePassOverview {
    pub(crate) configured: bool,
    pub(crate) limits: Vec<QuotaLimit>,
    pub(crate) models: Vec<ClinePassModel>,
    /// Safe, user-facing summary of the first failed call; never contains the key.
    pub(crate) error: Option<String>,
}

#[tauri::command]
pub(crate) async fn clinepass_overview(
    state: State<'_, AppState>,
) -> Result<ClinePassOverview, IpcError> {
    let key = state
        .secrets
        .get_secret(&provider_secret_account(PROVIDER_ID)?)
        .map_err(|_| IpcError {
            code: "secret_store_unavailable",
            message: "系统凭据库不可用，无法读取 Cline Pass Key。",
        })?
        .filter(|value| !value.trim().is_empty());

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| IpcError {
            code: "network_unavailable",
            message: "无法初始化网络请求。",
        })?;

    let mut errors = Vec::new();
    let recommended = match fetch_recommended(&client).await {
        Ok(models) => models,
        Err(()) => {
            errors.push("订阅模型列表暂不可用，已显示内置目录。".to_owned());
            Vec::new()
        }
    };

    let mut gateway = Vec::new();
    let mut limits = Vec::new();
    if let Some(key) = key.as_deref() {
        match fetch_gateway_models(&client, key).await {
            Ok(models) => gateway = models,
            Err(()) => errors.push("网关模型列表暂不可用。".to_owned()),
        }
        match fetch_limits(&client, key).await {
            Ok(parsed) => limits = parsed,
            Err(()) => errors.push("额度信息暂不可用，请检查 Key 或稍后重试。".to_owned()),
        }
    }

    Ok(ClinePassOverview {
        configured: key.is_some(),
        limits,
        models: merge_models(&recommended, &gateway),
        error: errors.into_iter().next(),
    })
}

/// Opens the official Cline Pass subscription page in the system browser.
#[tauri::command]
pub(crate) fn clinepass_open_subscription() -> Result<(), IpcError> {
    open::that_detached(SUBSCRIPTION_URL).map_err(|_| IpcError {
        code: "open_failed",
        message: "无法自动打开订阅页，请手动访问 cline.bot/cline-pass。",
    })
}

async fn fetch_recommended(client: &reqwest::Client) -> Result<Vec<ClinePassModel>, ()> {
    let response = client.get(RECOMMENDED_URL).send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let value: Value = response.json().await.map_err(|_| ())?;
    Ok(parse_recommended(&value))
}

async fn fetch_gateway_models(client: &reqwest::Client, key: &str) -> Result<Vec<String>, ()> {
    let response = client
        .get(format!("{BASE_URL}/models"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let value: Value = response.json().await.map_err(|_| ())?;
    Ok(parse_gateway_ids(&value))
}

async fn fetch_limits(client: &reqwest::Client, key: &str) -> Result<Vec<QuotaLimit>, ()> {
    let response = client
        .get(format!("{BASE_URL}/users/me/plan/usage-limits"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let value: Value = response.json().await.map_err(|_| ())?;
    Ok(parse_limits(&value))
}

/// The official list is `{ clinePass: [ "id" | { id, name, ... } ] }`, occasionally wrapped in
/// `data`. Entries without a `cline-pass/` prefix are rejected rather than guessed at.
fn parse_recommended(value: &Value) -> Vec<ClinePassModel> {
    let payload = value.get("data").unwrap_or(value);
    let list = payload
        .get("clinePass")
        .or_else(|| value.get("clinePass"))
        .and_then(Value::as_array);
    let Some(list) = list else {
        return Vec::new();
    };
    let mut models = Vec::new();
    for entry in list {
        let (id, name) = match entry {
            Value::String(id) => (id.clone(), None),
            Value::Object(map) => {
                let Some(id) = map.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let name = map.get("name").and_then(Value::as_str);
                (id.to_owned(), name)
            }
            _ => continue,
        };
        let Some(stripped) = id.strip_prefix("cline-pass/") else {
            continue;
        };
        if stripped.is_empty() || !stripped.bytes().all(valid_model_byte) {
            continue;
        }
        let model_name = match name {
            Some(value) if value != id => value.to_owned(),
            _ => shipped_name(&id),
        };
        models.push(ClinePassModel {
            name: model_name,
            context_window: shipped_context(&id),
            id,
            recommended: true,
            gateway: false,
        });
    }
    models
}

fn parse_gateway_ids(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .filter(|id| id.starts_with("cline-pass/"))
        .map(str::to_owned)
        .collect()
}

/// Tolerates both `{ data: { limits: [...] } }` and a bare `{ limits: [...] }` response.
fn parse_limits(value: &Value) -> Vec<QuotaLimit> {
    let payload = value.get("data").unwrap_or(value);
    let Some(list) = payload.get("limits").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let kind = entry.get("type").and_then(Value::as_str)?.trim();
            if kind.is_empty() {
                return None;
            }
            let percent = entry
                .get("percentUsed")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 100.0);
            let resets_at = entry
                .get("resetsAt")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned);
            Some(QuotaLimit {
                kind: kind.to_owned(),
                percent_used: percent,
                resets_at,
            })
        })
        .collect()
}

fn merge_models(recommended: &[ClinePassModel], gateway: &[String]) -> Vec<ClinePassModel> {
    let mut models = SHIPPED_MODELS
        .iter()
        .map(|(id, name, context)| ClinePassModel {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            context_window: Some(*context),
            recommended: false,
            gateway: false,
        })
        .collect::<Vec<_>>();

    for model in recommended {
        match models.iter_mut().find(|candidate| candidate.id == model.id) {
            Some(existing) => {
                existing.recommended = true;
                if existing.name == existing.id && model.name != model.id {
                    existing.name.clone_from(&model.name);
                }
                if existing.context_window.is_none() {
                    existing.context_window = model.context_window;
                }
            }
            None => models.push(model.clone()),
        }
    }
    for id in gateway {
        match models.iter_mut().find(|candidate| candidate.id == *id) {
            Some(existing) => existing.gateway = true,
            None => models.push(ClinePassModel {
                id: id.clone(),
                name: shipped_name(id),
                context_window: shipped_context(id),
                recommended: false,
                gateway: true,
            }),
        }
    }
    models
}

fn valid_model_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
}

fn shipped_name(id: &str) -> String {
    SHIPPED_MODELS
        .iter()
        .find(|(candidate, _, _)| *candidate == id)
        .map_or_else(
            || id.strip_prefix("cline-pass/").unwrap_or(id).to_owned(),
            |(_, name, _)| (*name).to_owned(),
        )
}

fn shipped_context(id: &str) -> Option<u64> {
    SHIPPED_MODELS
        .iter()
        .find(|(candidate, _, _)| *candidate == id)
        .map(|(_, _, context)| *context)
}

/// Curated fallback for the first paint and for offline gateways, mirroring the dsh plugin's
/// published catalog.
const SHIPPED_MODELS: &[(&str, &str, u64)] = &[
    (
        "cline-pass/deepseek-v4.1-flash",
        "DeepSeek V4.1 Flash",
        1_000_000,
    ),
    (
        "cline-pass/deepseek-v4-flash",
        "DeepSeek V4 Flash",
        1_000_000,
    ),
    ("cline-pass/deepseek-v4-pro", "DeepSeek V4 Pro", 1_000_000),
    ("cline-pass/glm-5.3", "GLM-5.3", 1_000_000),
    ("cline-pass/glm-5.3-flash", "GLM-5.3 Flash", 1_000_000),
    ("cline-pass/glm-5.2", "GLM-5.2", 1_000_000),
    ("cline-pass/kimi-k3", "Kimi K3", 1_048_576),
    ("cline-pass/kimi-k2.7-code", "Kimi K2.7 Code", 262_144),
    ("cline-pass/kimi-k2.6", "Kimi K2.6", 262_144),
    ("cline-pass/minimax-m3", "MiniMax-M3", 1_048_576),
    ("cline-pass/qwen3.8-max", "Qwen3.8 Max", 1_000_000),
    ("cline-pass/qwen3.7-max", "Qwen3.7 Max", 1_000_000),
    ("cline-pass/qwen3.7-plus", "Qwen3.7 Plus", 1_000_000),
    ("cline-pass/mimo-v2.5-pro", "MiMo-V2.5-Pro", 1_048_576),
    ("cline-pass/mimo-v2.5", "MiMo-V2.5", 1_048_576),
];

#[cfg(test)]
mod tests {
    use super::{merge_models, parse_gateway_ids, parse_limits, parse_recommended};
    use serde_json::json;

    #[test]
    fn parses_recommended_models_from_both_envelopes() {
        let wrapped = json!({
            "data": { "clinePass": [
                "cline-pass/glm-5.3",
                { "id": "cline-pass/kimi-k3", "name": "Kimi K3 Custom" },
                "other/ignored"
            ]}
        });
        let models = parse_recommended(&wrapped);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "cline-pass/glm-5.3");
        assert_eq!(models[0].name, "GLM-5.3");
        assert!(models[0].recommended);
        assert_eq!(models[1].name, "Kimi K3 Custom");
    }

    #[test]
    fn parses_quota_limits_and_clamps_percentages() {
        let value = json!({
            "success": true,
            "data": { "limits": [
                { "type": "five_hour", "percentUsed": 42.5, "resetsAt": "2026-10-18T12:00:00Z" },
                { "type": "weekly", "percentUsed": 130 },
                { "type": "" }
            ]}
        });
        let limits = parse_limits(&value);
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0].kind, "five_hour");
        assert_eq!(limits[0].percent_used, 42.5);
        assert_eq!(limits[1].percent_used, 100.0);
    }

    #[test]
    fn merges_shipped_recommended_and_gateway_models() {
        let recommended = parse_recommended(&json!({
            "clinePass": ["cline-pass/glm-5.3", "cline-pass/new-model"]
        }));
        let gateway = parse_gateway_ids(&json!({
            "data": [{ "id": "cline-pass/kimi-k3" }, { "id": "other/model" }]
        }));
        let models = merge_models(&recommended, &gateway);
        let glm = models
            .iter()
            .find(|model| model.id == "cline-pass/glm-5.3")
            .expect("shipped model exists");
        assert!(glm.recommended);
        let kimi = models
            .iter()
            .find(|model| model.id == "cline-pass/kimi-k3")
            .expect("shipped model exists");
        assert!(kimi.gateway);
        let new_model = models
            .iter()
            .find(|model| model.id == "cline-pass/new-model")
            .expect("recommended model is added");
        assert_eq!(new_model.name, "new-model");
    }
}
