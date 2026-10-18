import type { ProviderKind, ProviderProfileInput } from "./llm";

export interface ProviderPreset {
  id: string;
  displayName: string;
  kind: ProviderKind;
  baseUrl: string;
  /** Starting model; every vendor renames models, so the form lets the user edit it. */
  model: string;
  group: "cloud" | "local";
  /** Shown after the preset is applied, e.g. where to create the key. */
  note: string;
}

/**
 * Common chat-completion endpoints that speak the OpenAI-compatible protocol (or Anthropic /
 * Ollama natively). Presets are templates, not seeded profiles: nothing is persisted until the
 * user saves, so the model-service list stays as short as the user's actual subscriptions.
 */
export const PROVIDER_PRESETS: readonly ProviderPreset[] = [
  {
    id: "qwen",
    displayName: "通义千问（DashScope）",
    kind: "openai_compatible",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
    group: "cloud",
    note: "阿里云百炼控制台创建 API Key；模型可换 qwen-max / qwen-turbo。",
  },
  {
    id: "moonshot",
    displayName: "Kimi（Moonshot）",
    kind: "openai_compatible",
    baseUrl: "https://api.moonshot.cn/v1",
    model: "moonshot-v1-32k",
    group: "cloud",
    note: "Moonshot 控制台创建 API Key；模型可换 kimi-k2 系列。",
  },
  {
    id: "zhipu",
    displayName: "智谱 GLM",
    kind: "openai_compatible",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-plus",
    group: "cloud",
    note: "BigModel 控制台创建 API Key；模型可换 glm-4.5 / glm-4-flash。",
  },
  {
    id: "hunyuan",
    displayName: "腾讯混元",
    kind: "openai_compatible",
    baseUrl: "https://api.hunyuan.cloud.tencent.com/v1",
    model: "hunyuan-turbos-latest",
    group: "cloud",
    note: "腾讯云控制台开通混元并创建 API Key；模型可换 hunyuan-pro。",
  },
  {
    id: "qianfan",
    displayName: "百度千帆（文心）",
    kind: "openai_compatible",
    baseUrl: "https://qianfan.baidubce.com/v2",
    model: "ernie-4.5-turbo-128k",
    group: "cloud",
    note: "千帆控制台创建 API Key；模型名以控制台当前可用的 ERNIE 型号为准。",
  },
  {
    id: "siliconflow",
    displayName: "硅基流动 SiliconFlow",
    kind: "openai_compatible",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "deepseek-ai/DeepSeek-V3",
    group: "cloud",
    note: "硅基流动控制台创建 API Key；模型可换 Qwen/Qwen2.5-72B-Instruct 等。",
  },
  {
    id: "spark",
    displayName: "讯飞星火",
    kind: "openai_compatible",
    baseUrl: "https://spark-api-open.xf-yun.com/v1",
    model: "generalv3.5",
    group: "cloud",
    note: "星火控制台创建 APIPassword；模型名按控制台当前版本调整。",
  },
  {
    id: "gemini",
    displayName: "Google Gemini",
    kind: "openai_compatible",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-2.5-flash",
    group: "cloud",
    note: "Google AI Studio 创建 API Key；走 Gemini 的 OpenAI 兼容端点。",
  },
  {
    id: "openrouter",
    displayName: "OpenRouter",
    kind: "openai_compatible",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o-mini",
    group: "cloud",
    note: "一个 Key 访问多家模型；模型名形如 vendor/model。",
  },
  {
    id: "groq",
    displayName: "Groq",
    kind: "openai_compatible",
    baseUrl: "https://api.groq.com/openai/v1",
    model: "llama-3.3-70b-versatile",
    group: "cloud",
    note: "Groq 控制台创建 API Key；适合低延迟的英文/多语任务。",
  },
  {
    id: "xai",
    displayName: "xAI Grok",
    kind: "openai_compatible",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-3-mini",
    group: "cloud",
    note: "xAI 控制台创建 API Key；模型名按控制台调整。",
  },
  {
    id: "mistral",
    displayName: "Mistral",
    kind: "openai_compatible",
    baseUrl: "https://api.mistral.ai/v1",
    model: "mistral-small-latest",
    group: "cloud",
    note: "Mistral 控制台创建 API Key；mistral-small-latest 会跟随稳定版更新。",
  },
  {
    id: "lmstudio",
    displayName: "LM Studio（本地）",
    kind: "openai_compatible",
    baseUrl: "http://127.0.0.1:1234/v1",
    model: "local-model",
    group: "local",
    note: "在 LM Studio 中启动本地服务器（默认 1234），模型名可与已加载模型一致。",
  },
  {
    id: "vllm",
    displayName: "vLLM / 自建（本地）",
    kind: "openai_compatible",
    baseUrl: "http://127.0.0.1:8000/v1",
    model: "qwen2.5-7b-instruct",
    group: "local",
    note: "自建 vLLM、SGLang 等 OpenAI 兼容服务；模型名填部署时指定的名字。",
  },
];

/** Builds an unsaved draft from a preset, keeping IDs unique against the existing profiles. */
export function providerPresetInput(
  preset: ProviderPreset,
  existingIds: readonly string[] = [],
): ProviderProfileInput {
  let id = preset.id;
  let suffix = 2;
  while (existingIds.includes(id)) {
    id = `${preset.id}-${suffix}`;
    suffix += 1;
  }
  return {
    id,
    kind: preset.kind,
    displayName: preset.displayName,
    baseUrl: preset.baseUrl,
    model: preset.model,
    enabled: true,
  };
}
