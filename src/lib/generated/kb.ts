// This file is generated from src-tauri/crates/odoc-kb. Do not edit by hand.

export type KbProviderKind = "fns" | "local_vault" | "mcp";

export type KbConfig = { schemaVersion: number, enabled: boolean, providerKind: KbProviderKind, baseUrl: string, vault: string, localPath: string, indexEnabled: boolean, autoTaggingEnabled: boolean, syncEnabled: boolean, mcpUrl: string, };

export type SearchMode = "path" | "content" | "regex";

export type KbQuery = { vault: string | null, keyword: string, searchContent: boolean, searchMode: SearchMode, pathPrefix: string | null, filters: { [key in string]?: string }, limit: number, };

export type KbHit = { vault: string, path: string, title: string, snippet: string, score: number, frontmatter: { [key in string]?: string }, };

export type KbNote = { vault: string, path: string, title: string, content: string, frontmatter: { [key in string]?: string }, tags: Array<string>, links: Array<string>, modifiedAt: number | null, };

export type ChangeKind = "added" | "modified" | "deleted";

export type ChangeEvent = { kind: ChangeKind, vault: string, path: string, oldPath: string | null, cursor: number | null, };
