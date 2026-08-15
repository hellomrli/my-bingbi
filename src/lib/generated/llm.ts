// This file is generated from src-tauri/crates/odoc-llm. Do not edit by hand.

import type { SecurityLevel } from "./bindings";

export type ProviderKind = "openai_compatible" | "anthropic" | "ollama";

export type ProviderProfileInput = { id: string, kind: ProviderKind, displayName: string, baseUrl: string, model: string, enabled: boolean, };

export type ProviderProfile = { id: string, kind: ProviderKind, displayName: string, baseUrl: string, model: string, enabled: boolean, isLocal: boolean, };

export type ChatRole = "system" | "user" | "assistant";

export type ChatMessage = { role: ChatRole, content: string, };

export type ChatRequest = { messages: Array<ChatMessage>, temperature: number | null, maxTokens: number | null, jsonMode: boolean, securityLevel: SecurityLevel, };

export type TokenUsage = { promptTokens: number, completionTokens: number, totalTokens: number, };

export type ChatResponse = { content: string, finishReason: string | null, usage: TokenUsage | null, };

export type ChatDelta = { content: string, finishReason: string | null, usage: TokenUsage | null, };

export type Capabilities = { streaming: boolean, jsonMode: boolean, toolCalling: boolean, embeddings: boolean, contextWindow: number | null, isLocal: boolean, };

export type ConnectionTestResult = { providerId: string, model: string, latencyMs: number, capabilities: Capabilities, };
