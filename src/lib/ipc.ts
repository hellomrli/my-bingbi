import { Channel, invoke } from "@tauri-apps/api/core";
import { createEmptyDocument as createBrowserDocument } from "./document";
import type { KbConfig, KbHit, KbNote, KbQuery } from "./kb";
import type {
  ChatDelta,
  ChatRequest,
  ChatResponse,
  ConnectionTestResult,
  ProviderProfile,
  ProviderProfileInput,
} from "./llm";
import type { ODoc } from "./odoc";
import type { PackSummary, TemplateSummary } from "./pack";
import type { LintReport } from "./generated/lint";
import type {
  AppConfig,
  CrashEvent,
  DocumentSummary,
  DocumentVersionSummary,
  KbIndexState,
  SaveDocumentResult,
  SessionSummary,
  StageRunRecord,
} from "./store";

const BROWSER_REPOSITORY_KEY = "bingbi-browser-repository-v1";
const BROWSER_CONFIG_KEY = "bingbi-browser-config-v1";
const BROWSER_PROVIDERS_KEY = "bingbi-browser-providers-v1";
const BROWSER_KB_CONFIG_KEY = "bingbi-browser-kb-config-v1";

/**
 * Browser preview stores whole snapshots in localStorage, so it keeps a bounded window of
 * versions. Sequence numbers stay stable (`firstVersionSeq`), which is what history, diff and
 * restore address by.
 */
const MAX_BROWSER_VERSIONS = 50;

/** IPC/browser-boundary failure with a stable machine-readable code. */
export class IpcError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }
}

const builtinProviderProfiles: ProviderProfile[] = [
  {
    id: "openai-compatible",
    kind: "openai_compatible",
    displayName: "OpenAI 兼容",
    baseUrl: "https://api.openai.com",
    model: "gpt-4.1-mini",
    enabled: true,
    isLocal: false,
  },
  {
    id: "deepseek",
    kind: "openai_compatible",
    displayName: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-chat",
    enabled: true,
    isLocal: false,
  },
  {
    id: "anthropic",
    kind: "anthropic",
    displayName: "Claude (Anthropic)",
    baseUrl: "https://api.anthropic.com",
    model: "claude-opus-5",
    enabled: true,
    isLocal: false,
  },
  {
    id: "cline-pass",
    kind: "openai_compatible",
    displayName: "Cline Pass",
    baseUrl: "https://api.cline.bot/api/v1",
    model: "cline-pass/deepseek-v4.1-flash",
    enabled: false,
    isLocal: false,
  },
  {
    id: "ollama",
    kind: "ollama",
    displayName: "Ollama",
    baseUrl: "http://127.0.0.1:11434",
    model: "qwen3:8b",
    enabled: true,
    isLocal: true,
  },
];

interface BrowserVersion {
  document: ODoc;
  stage: string | null;
  createdAt: number;
}

interface BrowserDocumentRecord {
  document: ODoc;
  createdAt: number;
  updatedAt: number;
  versions: BrowserVersion[];
  /** Sequence number of `versions[0]`; older snapshots are dropped once the window is full. */
  firstVersionSeq?: number;
}

interface BrowserRepository {
  documents: Record<string, BrowserDocumentRecord>;
}

export interface KbIndexProgress {
  status: "running" | "ready" | "error" | string;
  processed: number;
  total: number;
  path: string | null;
  message: string | null;
}

export interface ArchiveResult {
  path: string;
  vault: string;
  overwritten: boolean;
}

export interface VersionDiff {
  documentId: string;
  leftSeq: number;
  rightSeq: number;
  leftText: string;
  rightText: string;
  addedLines: number;
  removedLines: number;
}

export interface RestoreVersionResult {
  document: ODoc;
  saved: SaveDocumentResult;
}

export interface DocxExportResult {
  bytes: number[];
  usedTemplate: boolean;
  fallbackReason: string | null;
}

export type PdfExportResult =
  | { kind: "pdf"; bytes: number[] }
  | { kind: "print_preview"; html: string };

export interface ExportCapabilities {
  customTemplateConfigured: boolean;
}

export const defaultAppConfig: AppConfig = {
  schemaVersion: 1,
  theme: "system",
  autosaveEnabled: true,
  fontMappings: {},
  firstRunCompleted: false,
  autoUpdateEnabled: true,
  crashReportingEnabled: false,
  activePackIds: [],
};

export const defaultKbConfig: KbConfig = {
  schemaVersion: 1,
  enabled: false,
  providerKind: "fns",
  baseUrl: "http://127.0.0.1:9000",
  vault: "",
  localPath: "",
  indexEnabled: true,
  autoTaggingEnabled: false,
  syncEnabled: true,
  mcpUrl: "",
};

export function isDesktopRuntime(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

export async function createEmptyDocument(): Promise<ODoc> {
  if (!isDesktopRuntime()) return createBrowserDocument();
  return invoke<ODoc>("create_empty_document");
}

export async function backendVersion(): Promise<string> {
    if (!isDesktopRuntime()) return "browser-preview";
    return invoke<string>("backend_version");
}

export async function listPackSummaries(): Promise<PackSummary[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<PackSummary[]>("list_pack_summaries");
}

/** Lists the knowledge-pack document templates; browser preview has no pack registry. */
export async function listTemplates(): Promise<TemplateSummary[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<TemplateSummary[]>("list_templates");
}

export async function saveDocument(document: ODoc, stage: string): Promise<SaveDocumentResult> {
  if (isDesktopRuntime()) return invoke<SaveDocumentResult>("save_document", { document, stage });

  const repository = readBrowserRepository();
  const existing = repository.documents[document.id];
  const now = Date.now();
  if (existing && document.revision < existing.document.revision && JSON.stringify(existing.document) !== JSON.stringify(document)) {
    throw browserError("stale_revision", "检测到较新的已保存版本，请重新打开公文后再修改。");
  }
  const versionCreated = !existing || JSON.stringify(existing.document) !== JSON.stringify(document);
  const versions = existing?.versions ?? [];
  let firstSeq = existing?.firstVersionSeq ?? 1;
  if (versionCreated) {
    versions.push({ document: structuredClone(document), stage: stage || null, createdAt: now });
  }
  while (versions.length > MAX_BROWSER_VERSIONS) {
    versions.shift();
    firstSeq += 1;
  }
  repository.documents[document.id] = {
    document: structuredClone(document),
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    versions,
    firstVersionSeq: firstSeq,
  };
  writeBrowserRepository(repository);
  return {
    documentId: document.id,
    versionSeq: firstSeq + versions.length - 1,
    created: !existing,
    versionCreated,
    savedAt: now,
  };
}

/** Generates a styled DOCX in the desktop kernel, optionally through the configured template. */
export async function exportDocx(documentId: string): Promise<DocxExportResult> {
  if (!isDesktopRuntime()) {
    throw browserError("export_unavailable", "DOCX 导出仅在桌面应用中可用，浏览器版可导出 ODoc JSON。");
  }
  return invoke<DocxExportResult>("export_docx", { documentId });
}

export async function importDocx(bytes: Uint8Array): Promise<RestoreVersionResult> {
  if (!isDesktopRuntime()) {
    throw browserError("import_unavailable", "DOCX 导入仅在桌面应用中可用。");
  }
  return invoke<RestoreVersionResult>("import_docx", { bytes: Array.from(bytes) });
}

/** Extracts a DOCX reference as plain text without importing it into the document library. */
export async function extractDocxText(bytes: Uint8Array): Promise<string> {
  if (!isDesktopRuntime()) {
    throw browserError("reference_upload_unavailable", "DOCX 参考文件仅可在桌面应用中读取。");
  }
  return invoke<string>("extract_docx_text", { bytes: Array.from(bytes) });
}

/** Extracts a supported binary reference as plain text without importing it into the library. */
export async function extractReferenceText(fileName: string, bytes: Uint8Array): Promise<string> {
  if (!isDesktopRuntime()) {
    throw browserError(
      "reference_upload_unavailable",
      "PDF、OFD、DOC、DOCX、XLSX、XLS 和 WPS 参考文件仅可在桌面应用中读取。",
    );
  }
  return invoke<string>("extract_reference_text", { fileName, bytes: Array.from(bytes) });
}

export interface DroppedReference {
  fileName: string;
  text: string;
}

/**
 * Reads a path delivered by Tauri's native file-drop event.
 *
 * The WebView cannot open arbitrary native paths, so text/Office extraction happens in the
 * kernel; only the bounded extracted text crosses back into the composer state.
 */
export async function readDroppedReference(path: string): Promise<DroppedReference> {
  if (!isDesktopRuntime()) {
    throw browserError("reference_upload_unavailable", "参考文件拖拽仅在桌面应用中可用。");
  }
  return invoke<DroppedReference>("read_dropped_reference", { path });
}

export async function exportPdf(documentId: string): Promise<PdfExportResult> {
  if (!isDesktopRuntime()) {
    throw browserError("export_unavailable", "PDF 导出仅在桌面应用中可用。");
  }
  return invoke<PdfExportResult>("export_pdf", { documentId });
}

export async function exportCapabilities(): Promise<ExportCapabilities> {
  if (!isDesktopRuntime()) {
    return { customTemplateConfigured: false };
  }
  return invoke<ExportCapabilities>("export_capabilities");
}

export async function saveDocxTemplate(bytes: Uint8Array): Promise<void> {
  if (!isDesktopRuntime()) {
    throw browserError("template_unavailable", "自定义 DOCX 模板仅在桌面应用中可用。");
  }
  await invoke("save_docx_template", { bytes: Array.from(bytes) });
}

export async function deleteDocxTemplate(): Promise<void> {
  if (!isDesktopRuntime()) {
    throw browserError("template_unavailable", "自定义 DOCX 模板仅在桌面应用中可用。");
  }
  await invoke("delete_docx_template");
}

export async function loadDocument(id: string): Promise<ODoc> {
  if (isDesktopRuntime()) return invoke<ODoc>("load_document", { id });
  const record = readBrowserRepository().documents[id];
  if (!record) throw browserError("document_not_found", "没有找到指定的公文。");
  return structuredClone(record.document);
}

/** Runs the pack-driven rule engine against the in-memory document (desktop only). */
export async function lintDocumentNow(document: ODoc): Promise<LintReport> {
  if (!isDesktopRuntime()) {
    throw browserError("lint_unavailable", "规则校验仅在桌面应用中可用。");
  }
  return invoke<LintReport>("lint_document_now", { document });
}

/** Lists recently updated documents (newest first). */
export async function listDocuments(limit = 30): Promise<DocumentSummary[]> {  if (isDesktopRuntime()) return invoke<DocumentSummary[]>("list_documents", { limit });
  return Object.entries(readBrowserRepository().documents)
    .map(([id, record]) => ({
      id,
      title: record.document.body.title.text.trim() || "未命名公文",
      docType: record.document.meta.docType,
      revision: record.document.revision,
      status: record.document.meta.status,
      versionCount: record.versions.length,
      createdAt: record.createdAt,
      updatedAt: record.updatedAt,
    }))
    .sort((left, right) => right.updatedAt - left.updatedAt || left.id.localeCompare(right.id))
    .slice(0, Math.max(1, Math.min(limit, 200)));
}

/** Lists drafting sessions (newest first); browser preview has no desktop sessions. */
export async function listSessions(
  documentId?: string | null,
  limit = 20,
): Promise<SessionSummary[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<SessionSummary[]>("list_sessions", {
    documentId: documentId ?? null,
    limit,
  });
}

/** Lists the recorded stage runs of one drafting session for the cost panel. */
export async function listStageRuns(sessionId: string): Promise<StageRunRecord[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<StageRunRecord[]>("list_stage_runs", { sessionId });
}

export async function deleteDocument(id: string): Promise<boolean> {
  if (isDesktopRuntime()) return invoke<boolean>("delete_document", { id });
  const repository = readBrowserRepository();
  const existed = Boolean(repository.documents[id]);
  delete repository.documents[id];
  writeBrowserRepository(repository);
  return existed;
}

export async function listDocumentVersions(documentId: string): Promise<DocumentVersionSummary[]> {
  if (isDesktopRuntime()) {
    return invoke<DocumentVersionSummary[]>("list_document_versions", { documentId });
  }
  const record = readBrowserRepository().documents[documentId];
  if (!record) throw browserError("document_not_found", "没有找到指定的公文。");
  const firstSeq = record.firstVersionSeq ?? 1;
  return record.versions
    .map((version, index) => ({
      seq: firstSeq + index,
      odocRevision: version.document.revision,
      stage: version.stage,
      createdAt: version.createdAt,
    }))
    .reverse();
}

export async function loadDocumentVersion(documentId: string, seq: number): Promise<ODoc> {
  if (isDesktopRuntime()) {
    return invoke<ODoc>("load_document_version", { documentId, seq });
  }
  const record = readBrowserRepository().documents[documentId];
  if (!record) throw browserError("version_not_found", "没有找到指定的历史版本。");
  const version = record.versions[seq - (record.firstVersionSeq ?? 1)];
  if (!version) throw browserError("version_not_found", "没有找到指定的历史版本。");
  return structuredClone(version.document);
}

export async function restoreDocumentVersion(documentId: string, seq: number): Promise<RestoreVersionResult> {
  if (isDesktopRuntime()) {
    return invoke<RestoreVersionResult>("restore_document_version", { documentId, seq });
  }
  const repository = readBrowserRepository();
  const record = repository.documents[documentId];
  if (!record) throw browserError("version_not_found", "没有找到指定的历史版本。");
  const version = record.versions[seq - (record.firstVersionSeq ?? 1)];
  if (!version) throw browserError("version_not_found", "没有找到指定的历史版本。");
  const restored = structuredClone(version.document);
  restored.id = record.document.id;
  restored.revision = record.document.revision + 1;
  const saved = await saveDocument(restored, `restore:${seq}`);
  return { document: restored, saved };
}

export async function diffDocumentVersions(
  documentId: string,
  leftSeq: number,
  rightSeq: number,
): Promise<VersionDiff> {
  if (isDesktopRuntime()) {
    return invoke<VersionDiff>("diff_document_versions", { documentId, leftSeq, rightSeq });
  }
  const [left, right] = await Promise.all([
    loadDocumentVersion(documentId, leftSeq),
    loadDocumentVersion(documentId, rightSeq),
  ]);
  const leftText = documentPlainText(left);
  const rightText = documentPlainText(right);
  const [removedLines, addedLines] = changedLineCounts(leftText, rightText);
  return { documentId, leftSeq, rightSeq, leftText, rightText, addedLines, removedLines };
}

export async function loadAppConfig(): Promise<AppConfig> {
  if (isDesktopRuntime()) return invoke<AppConfig>("load_app_config");
  const raw = localStorage.getItem(BROWSER_CONFIG_KEY);
  if (!raw) return structuredClone(defaultAppConfig);
  try {
    const parsed = JSON.parse(raw) as Partial<AppConfig>;
    return {
      ...defaultAppConfig,
      ...parsed,
      fontMappings: parsed.fontMappings ?? {},
    };
  } catch {
    return structuredClone(defaultAppConfig);
  }
}

export async function saveAppConfig(config: AppConfig): Promise<void> {
  if (isDesktopRuntime()) {
    await invoke("save_app_config", { config });
    return;
  }
  localStorage.setItem(BROWSER_CONFIG_KEY, JSON.stringify(config));
}

export async function loadKbConfig(): Promise<KbConfig> {
  if (isDesktopRuntime()) return invoke<KbConfig>("load_kb_config");
  const raw = localStorage.getItem(BROWSER_KB_CONFIG_KEY);
  if (!raw) return structuredClone(defaultKbConfig);
  try {
    return normalizeBrowserKbConfig(JSON.parse(raw) as KbConfig);
  } catch {
    return structuredClone(defaultKbConfig);
  }
}

export async function saveKbConfig(config: KbConfig): Promise<KbConfig> {
  if (isDesktopRuntime()) return invoke<KbConfig>("save_kb_config", { config });
  const normalized = normalizeBrowserKbConfig(config);
  localStorage.setItem(BROWSER_KB_CONFIG_KEY, JSON.stringify(normalized));
  return structuredClone(normalized);
}

export async function setKbFnsToken(token: string): Promise<void> {
  requireDesktopSecretStore();
  await invoke("set_kb_fns_token", { token });
}

export async function hasKbFnsToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("has_kb_fns_token");
}

export async function deleteKbFnsToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("delete_kb_fns_token");
}

export async function setKbFnsWsToken(token: string): Promise<void> {
  requireDesktopSecretStore();
  await invoke("set_kb_fns_ws_token", { token });
}

export async function hasKbFnsWsToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("has_kb_fns_ws_token");
}

export async function deleteKbFnsWsToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("delete_kb_fns_ws_token");
}

export async function setKbMcpToken(token: string): Promise<void> {
  requireDesktopSecretStore();
  await invoke("set_kb_mcp_token", { token });
}

export async function hasKbMcpToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("has_kb_mcp_token");
}

export async function deleteKbMcpToken(): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("delete_kb_mcp_token");
}

export async function kbSearch(query: KbQuery, documentId?: string): Promise<KbHit[]> {
  requireDesktopKbRuntime();
  return invoke<KbHit[]>("kb_search", { query, documentId });
}

export async function kbGet(path: string, documentId?: string): Promise<KbNote> {
  requireDesktopKbRuntime();
  return invoke<KbNote>("kb_get", { path, documentId });
}

export async function kbIndexState(vault?: string): Promise<KbIndexState | null> {
  requireDesktopKbRuntime();
  return invoke<KbIndexState | null>("kb_index_state", { vault });
}

export async function kbRebuildIndex(
  documentId: string | undefined,
  onProgress: (progress: KbIndexProgress) => void,
): Promise<KbIndexState> {
  requireDesktopKbRuntime();
  const channel = new Channel<KbIndexProgress>();
  channel.onmessage = onProgress;
  return invoke<KbIndexState>("kb_rebuild_index", { documentId, onProgress: channel });
}

export async function kbClearIndex(vault?: string): Promise<void> {
  requireDesktopKbRuntime();
  await invoke("kb_clear_index", { vault });
}

export async function kbSyncOnce(documentId?: string): Promise<KbIndexState> {
  requireDesktopKbRuntime();
  return invoke<KbIndexState>("kb_sync_once", { documentId });
}

export interface CreatedKbNote {
  path: string;
  title: string;
}

/** Creates the managed local Markdown vault (or repairs missing folders) and returns its path. */
export async function kbCreateLocalVault(path?: string): Promise<string> {
  requireDesktopKbRuntime();
  return invoke<string>("kb_create_local_vault", { path: path?.trim() || null });
}

/** Writes one Markdown note into the configured knowledge base. */
export async function kbCreateNote(input: {
  documentId?: string | null;
  title: string;
  content: string;
  docType?: string | null;
}): Promise<CreatedKbNote> {
  requireDesktopKbRuntime();
  return invoke<CreatedKbNote>("kb_create_note", {
    documentId: input.documentId ?? null,
    title: input.title,
    content: input.content,
    docType: input.docType ?? null,
  });
}

export async function archiveDocumentToKb(
  documentId: string,
  path: string,
  overwrite = false,
): Promise<ArchiveResult> {
  requireDesktopKbRuntime();
  return invoke<ArchiveResult>("archive_document_to_kb", { documentId, path, overwrite });
}

export interface UpdateCheckResult {
  available: boolean;
  version: string | null;
  currentVersion: string;
  installed: boolean;
}

/** Checks the configured update endpoint; only the desktop runtime has update services. */
export async function checkForUpdate(): Promise<UpdateCheckResult> {
  if (!isDesktopRuntime()) {
    throw browserError("update_unavailable", "更新检查仅在桌面应用中可用。");
  }
  return invoke<UpdateCheckResult>("check_for_update");
}

/** Downloads and installs the pending update, then restarts the application. */
export async function applyUpdate(): Promise<void> {
  if (!isDesktopRuntime()) {
    throw browserError("update_unavailable", "更新检查仅在桌面应用中可用。");
  }
  return invoke<void>("apply_update");
}

/** The user pack directory; browser preview has none. */
export async function userPackRoot(): Promise<string | null> {
  if (!isDesktopRuntime()) return null;
  return invoke<string>("user_pack_root");
}

/** Pack-loading issues from the user pack directory (broken manifests, id clashes). */
export async function packLoadIssues(): Promise<string[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<string[]>("pack_load_issues");
}

/** Lists the newest opt-in Rust panic diagnostics stored on this device. */
export async function listCrashEvents(limit = 20): Promise<CrashEvent[]> {
  if (!isDesktopRuntime()) return [];
  return invoke<CrashEvent[]>("list_crash_events", { limit });
}

/** Clears all local panic diagnostics and returns the number removed. */
export async function clearCrashEvents(): Promise<number> {
  if (!isDesktopRuntime()) {
    throw browserError("crash_diagnostics_unavailable", "崩溃诊断仅在桌面应用中可用。");
  }
  return invoke<number>("clear_crash_events");
}

export async function listProviderProfiles(): Promise<ProviderProfile[]> {
  if (isDesktopRuntime()) return invoke<ProviderProfile[]>("list_provider_profiles");
  const profiles = new Map(builtinProviderProfiles.map((profile) => [profile.id, structuredClone(profile)]));
  for (const profile of readBrowserProviders()) profiles.set(profile.id, profile);
  return [...profiles.values()].sort(
    (left, right) => left.displayName.localeCompare(right.displayName, "zh-CN") || left.id.localeCompare(right.id),
  );
}

export async function saveProviderProfile(input: ProviderProfileInput): Promise<ProviderProfile> {
  if (isDesktopRuntime()) return invoke<ProviderProfile>("save_provider_profile", { profile: input });
  const profile = normalizeBrowserProvider(input);
  const profiles = readBrowserProviders().filter((item) => item.id !== profile.id);
  profiles.push(profile);
  localStorage.setItem(BROWSER_PROVIDERS_KEY, JSON.stringify(profiles));
  return structuredClone(profile);
}

export async function testProviderConnection(providerId: string): Promise<ConnectionTestResult> {
  requireDesktopModelRuntime();
  return invoke<ConnectionTestResult>("test_provider_connection", { providerId });
}

export async function listProviderModels(providerId: string): Promise<string[]> {
  requireDesktopModelRuntime();
  return invoke<string[]>("list_provider_models", { providerId });
}

export async function chatWithProvider(
  providerId: string,
  documentId: string,
  request: ChatRequest,
): Promise<ChatResponse> {
  requireDesktopModelRuntime();
  return invoke<ChatResponse>("chat_with_provider", { providerId, documentId, request });
}

export async function chatStreamWithProvider(
  providerId: string,
  documentId: string,
  request: ChatRequest,
  onDelta: (delta: ChatDelta) => void,
): Promise<void> {
  requireDesktopModelRuntime();
  const channel = new Channel<ChatDelta>();
  channel.onmessage = onDelta;
  await invoke("chat_stream_with_provider", { providerId, documentId, request, onDelta: channel });
}

export async function setProviderSecret(providerId: string, secret: string): Promise<void> {
  requireDesktopSecretStore();
  await invoke("set_provider_secret", { providerId, secret });
}

export async function hasProviderSecret(providerId: string): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("has_provider_secret", { providerId });
}

export async function deleteProviderSecret(providerId: string): Promise<boolean> {
  requireDesktopSecretStore();
  return invoke<boolean>("delete_provider_secret", { providerId });
}

export function ipcErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  if (typeof error === "string") return error;
  return "操作未完成，请稍后重试。";
}

function readBrowserRepository(): BrowserRepository {
  const raw = localStorage.getItem(BROWSER_REPOSITORY_KEY);
  if (!raw) return { documents: {} };
  try {
    const parsed = JSON.parse(raw) as BrowserRepository;
    if (!parsed || typeof parsed.documents !== "object" || parsed.documents === null) return { documents: {} };
    return parsed;
  } catch {
    return { documents: {} };
  }
}

function writeBrowserRepository(repository: BrowserRepository): void {
  try {
    localStorage.setItem(BROWSER_REPOSITORY_KEY, JSON.stringify(repository));
  } catch (error) {
    // The quota is shared by every origin key; a long document plus its snapshots can fill it.
    if (error instanceof DOMException && (error.name === "QuotaExceededError" || error.name === "NS_ERROR_DOM_QUOTA_REACHED")) {
      throw browserError(
        "storage_full",
        "浏览器本地存储已满，请先在「最近文档」删除不再需要的公文，或改用桌面版保存。",
      );
    }
    throw error;
  }
}

function readBrowserProviders(): ProviderProfile[] {
  const raw = localStorage.getItem(BROWSER_PROVIDERS_KEY);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed)
      ? parsed.filter(isProviderProfile).map((profile) => structuredClone(profile))
      : [];
  } catch {
    return [];
  }
}

function normalizeBrowserProvider(input: ProviderProfileInput): ProviderProfile {
  const idValid = input.id.length > 0
    && input.id.length <= 64
    && /^[A-Za-z0-9._-]+$/.test(input.id);
  const displayName = input.displayName.trim();
  const model = input.model.trim();
  let url: URL;
  try {
    url = new URL(input.baseUrl.trim());
  } catch {
    throw browserError("invalid_input", "模型服务配置无效。");
  }
  const hostname = url.hostname.toLowerCase();
  const isLocal = hostname === "localhost"
    || hostname.endsWith(".localhost")
    || hostname === "127.0.0.1"
    || hostname === "[::1]";
  const valid = idValid
    && displayName.length > 0
    && model.length > 0
    && (url.protocol === "https:" || (url.protocol === "http:" && isLocal))
    && !url.username
    && !url.password
    && !url.search
    && !url.hash;
  if (!valid) throw browserError("invalid_input", "模型服务配置无效。");
  return {
    ...input,
    displayName,
    model,
    baseUrl: url.toString().replace(/\/$/, ""),
    isLocal,
  };
}

function normalizeBrowserKbConfig(input: KbConfig): KbConfig {
  const config: KbConfig = {
    schemaVersion: input.schemaVersion,
    enabled: input.enabled,
    providerKind: input.providerKind,
    baseUrl: input.baseUrl.trim().replace(/\/+$/, ""),
    vault: input.vault.trim(),
    localPath: input.localPath.trim(),
    indexEnabled: input.indexEnabled ?? true,
    autoTaggingEnabled: input.autoTaggingEnabled ?? false,
    syncEnabled: input.syncEnabled ?? true,
    mcpUrl: (input.mcpUrl ?? "").trim().replace(/\/+$/, ""),
  };
  const commonValid = config.schemaVersion === 1
    && typeof config.enabled === "boolean"
    && typeof config.indexEnabled === "boolean"
    && typeof config.autoTaggingEnabled === "boolean"
    && typeof config.syncEnabled === "boolean"
    && (config.providerKind === "fns" || config.providerKind === "local_vault" || config.providerKind === "mcp")
    && typeof config.mcpUrl === "string"
    // eslint-disable-next-line no-control-regex -- rejecting control characters is the point
    && !/[\u0000-\u001f\u007f]/.test(`${config.baseUrl}${config.vault}${config.localPath}${config.mcpUrl}`);
  if (!commonValid) throw browserError("invalid_input", "知识库配置无效。");
  if (!config.enabled) return config;
  if (!config.vault) throw browserError("invalid_input", "知识库名称不能为空。");
  if (config.providerKind === "local_vault") {
    const absolute = config.localPath.startsWith("/")
      || /^[A-Za-z]:[\\/]/.test(config.localPath)
      || config.localPath.startsWith("\\\\");
    if (!absolute) throw browserError("invalid_input", "本地 Vault 路径必须是绝对路径。");
    return config;
  }
  if (config.providerKind === "mcp") {
    let mcpUrl: URL;
    try { mcpUrl = new URL(config.mcpUrl); } catch { throw browserError("invalid_input", "MCP 地址无效。"); }
    const hostname = mcpUrl.hostname.toLowerCase();
    const local = hostname === "localhost"
      || hostname.endsWith(".localhost")
      || hostname === "127.0.0.1"
      || hostname === "[::1]";
    if ((mcpUrl.protocol !== "https:" && !(mcpUrl.protocol === "http:" && local))
      || mcpUrl.username || mcpUrl.password || mcpUrl.search || mcpUrl.hash) {
      throw browserError("invalid_input", "MCP 地址无效。");
    }
    return config;
  }
  let url: URL;
  try {
    url = new URL(config.baseUrl);
  } catch {
    throw browserError("invalid_input", "FNS 服务地址无效。");
  }
  const hostname = url.hostname.toLowerCase();
  const local = hostname === "localhost"
    || hostname.endsWith(".localhost")
    || hostname === "127.0.0.1"
    || hostname === "[::1]";
  if ((url.protocol !== "https:" && !(url.protocol === "http:" && local))
    || Boolean(url.username || url.password || url.search || url.hash)) {
    throw browserError("invalid_input", "FNS 服务地址无效。");
  }
  return config;
}

function documentPlainText(document: ODoc): string {
  const blockLines = document.body.blocks.flatMap((block) => {
    if (block.type === "list") return block.items.map((item) => item.content);
    if (block.type === "table") return block.rows.flatMap((row) => row.cells.map((cell) => cell.content));
    return [block.content];
  });
  return [
    document.header?.issuingAuthorityMark,
    document.header?.documentNumber,
    document.body.title.text,
    ...document.body.mainRecipients,
    ...blockLines,
    ...document.body.attachments.map((attachment) => attachment.title),
    document.body.signer?.authority,
    document.body.date?.value,
    document.body.note,
    ...(document.footer?.copyRecipients ?? []),
    document.footer?.printingAuthority,
    document.footer?.printingDate,
  ].filter((value): value is string => Boolean(value)).join("\n");
}

function changedLineCounts(leftText: string, rightText: string): [number, number] {
  const left = leftText.split("\n");
  const right = rightText.split("\n");
  const previous = new Uint32Array(right.length + 1);
  const current = new Uint32Array(right.length + 1);
  for (const leftLine of left) {
    current.fill(0);
    for (let column = 1; column <= right.length; column += 1) {
      current[column] = leftLine === right[column - 1]
        ? previous[column - 1] + 1
        : Math.max(previous[column], current[column - 1]);
    }
    previous.set(current);
  }
  const common = previous[right.length];
  return [left.length - common, right.length - common];
}

function isProviderProfile(value: unknown): value is ProviderProfile {
  if (typeof value !== "object" || value === null) return false;
  const profile = value as Partial<ProviderProfile>;
  return typeof profile.id === "string"
    && (profile.kind === "openai_compatible" || profile.kind === "anthropic" || profile.kind === "ollama")
    && typeof profile.displayName === "string"
    && typeof profile.baseUrl === "string"
    && typeof profile.model === "string"
    && typeof profile.enabled === "boolean"
    && typeof profile.isLocal === "boolean";
}

function requireDesktopSecretStore(): void {
  if (!isDesktopRuntime()) {
    throw browserError("secret_store_unavailable", "系统凭据库仅在桌面应用中可用。");
  }
}

function requireDesktopModelRuntime(): void {
  if (!isDesktopRuntime()) {
    throw browserError("model_runtime_unavailable", "模型调用仅在桌面应用中可用。");
  }
}

function requireDesktopKbRuntime(): void {
  if (!isDesktopRuntime()) {
    throw browserError("kb_unavailable", "知识库检索仅在桌面应用中可用。");
  }
}

function browserError(code: string, message: string): IpcError {
  return new IpcError(code, message);
}
