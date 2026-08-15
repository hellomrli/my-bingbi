// This file is generated from src-tauri/crates/odoc-store. Do not edit by hand.

import type { DocumentStatus, DocumentType } from "./bindings";

export type AppTheme = "system" | "light" | "dark";

export type AppConfig = { schemaVersion: number, theme: AppTheme, autosaveEnabled: boolean, fontMappings: { [key in string]?: string }, firstRunCompleted: boolean, autoUpdateEnabled: boolean, crashReportingEnabled: boolean, activePackIds: Array<string>, };

export type SaveDocumentResult = { documentId: string, versionSeq: number, created: boolean, versionCreated: boolean, savedAt: number, };

export type DocumentSummary = { id: string, title: string, docType: DocumentType, revision: number, status: DocumentStatus, versionCount: number, createdAt: number, updatedAt: number, };

export type DocumentVersionSummary = { seq: number, odocRevision: number, stage: string | null, createdAt: number, };

export type SessionSummary = { id: string, documentId: string, pipeline: string, stateJson: string, costTokens: number, createdAt: number, updatedAt: number, };

export type StageRunRecord = { id: number, sessionId: string, stageId: string, inputJson: string, outputJson: string | null, provider: string | null, tokens: number, durationMs: number, createdAt: number, };

export type KbCacheHit = { vault: string, path: string, title: string, snippet: string, score: number, frontmatter: { [key in string]?: string }, };

export type KbIndexState = { vault: string, providerKind: string, lastCursor: number, noteCount: number, chunkCount: number, status: string, errorMessage: string | null, updatedAt: number, };

export type CrashEvent = { id: number, kind: string, message: string, contextJson: string, createdAt: number, };

export type UpdateState = { currentVersion: string, availableVersion: string | null, downloadUrl: string | null, checkedAt: number | null, status: string, };

export type PackInstallRecord = { packId: string, sourcePath: string, enabled: boolean, installedAt: number, updatedAt: number, };

export type CostSummary = { totalTokens: number, totalMoney: number, sessionCount: number, };
