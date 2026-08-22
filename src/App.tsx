import { Code2, FileJson2, LayoutTemplate, LockKeyhole, Save, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArchiveDialog } from "./features/documents/ArchiveDialog";
import { DocumentLibrary } from "./features/documents/DocumentLibrary";
import { VersionHistoryDialog } from "./features/documents/VersionHistoryDialog";
import { StructureEditor } from "./features/editor/StructureEditor";
import { IntakeSidebar } from "./features/intake/IntakeSidebar";
import { LintPanel } from "./features/lint-panel/LintPanel";
import { WelcomeGuide } from "./features/onboarding/WelcomeGuide";
import { DocumentPreview } from "./features/preview/DocumentPreview";
import { ReviewReportDialog } from "./features/review/ReviewReportDialog";
import { SessionHistoryDialog } from "./features/documents/SessionHistoryDialog";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { Toolbar } from "./features/workbench/Toolbar";
import { countChineseCharacters, documentTypeLabels } from "./lib/document";
import { draftStageLabels, draftStages, imitateStages, polishStages, rerunStage, shouldApplyDraftOutcome, shouldApplyRerunOutcome, startDraft, startImitate, startPolish, type DraftStageId } from "./lib/engine";
import { isReviewStage, startReview, type ReviewStageId } from "./lib/review";
import {
  backendVersion,
  archiveDocumentToKb,
  createEmptyDocument,
  defaultAppConfig,
  defaultKbConfig,
  deleteDocument,
  exportDocx,
  exportPdf,
  importDocx,
  ipcErrorMessage,
  isDesktopRuntime,
  listDocuments,
  loadKbConfig,
  listProviderProfiles,
  loadAppConfig,
  loadDocument,
  saveAppConfig,
  saveDocument,
  type ArchiveResult,
  type RestoreVersionResult,
} from "./lib/ipc";
import type { DraftEvent, DraftOutcome, RerunOutcome, ReviewEvent, ReviewOutcome, TypeDecision } from "./lib/generated/engine";
import type { FactCheckReport, LintReport } from "./lib/generated/lint";
import type { ProviderProfile } from "./lib/llm";
import type { KbConfig } from "./lib/kb";
import type { DocumentType } from "./lib/odoc";
import { lintDocument, unverifiedTokenMap, type Finding } from "./lib/lint";
import type { AppConfig, DocumentSummary } from "./lib/store";
import { useWorkbenchStore } from "./store/workbench";

function downloadDocument(filename: string, contents: string) {
  const blob = new Blob([contents], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = window.document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function downloadBytes(filename: string, bytes: Uint8Array, mimeType: string) {
  const blob = new Blob([bytes], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = window.document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function errorCode(error: unknown): string | null {
  return typeof error === "object" && error !== null && "code" in error && typeof error.code === "string"
    ? error.code
    : null;
}

function isDraftStage(value: string): value is DraftStageId {
  return value in draftStageLabels;
}

function App() {
  const document = useWorkbenchStore((state) => state.document);
  const view = useWorkbenchStore((state) => state.view);
  const theme = useWorkbenchStore((state) => state.theme);
  const dirty = useWorkbenchStore((state) => state.dirty);
  const savedAt = useWorkbenchStore((state) => state.savedAt);
  const setDocument = useWorkbenchStore((state) => state.setDocument);
  const setUnverifiedTokens = useWorkbenchStore((state) => state.setUnverifiedTokens);
  const setView = useWorkbenchStore((state) => state.setView);
  const setTheme = useWorkbenchStore((state) => state.setTheme);
  const markSaved = useWorkbenchStore((state) => state.markSaved);
  const importInputRef = useRef<HTMLInputElement>(null);
  const [runtime, setRuntime] = useState("正在连接内核");
  const [notice, setNotice] = useState<string | null>(null);
  const [config, setConfig] = useState<AppConfig>(defaultAppConfig);
  const [kbConfig, setKbConfig] = useState<KbConfig>(defaultKbConfig);
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [guideOpen, setGuideOpen] = useState(false);
  const [archiveOpen, setArchiveOpen] = useState(false);
  const [archiveBusy, setArchiveBusy] = useState(false);
  const [historyTarget, setHistoryTarget] = useState<{ id: string; title: string } | null>(null);
  const [sessionTarget, setSessionTarget] = useState<{ id: string; title: string } | null>(null);
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [savedVersion, setSavedVersion] = useState<number | null>(null);
  const [providers, setProviders] = useState<ProviderProfile[]>([]);
  const [providerId, setProviderId] = useState("");
  const [material, setMaterial] = useState("");
  const [modelText, setModelText] = useState("");
  const [drafting, setDrafting] = useState(false);
  const [draftStage, setDraftStage] = useState<DraftStageId | null>(null);
  const [completedStages, setCompletedStages] = useState<DraftStageId[]>([]);
  const [failedStage, setFailedStage] = useState<DraftStageId | null>(null);
  // The session id returned by the last completed draft; single stages can be re-run from it.
  const [draftSessionId, setDraftSessionId] = useState<string | null>(null);
  const [draftText, setDraftText] = useState("");
  const [draftError, setDraftError] = useState<string | null>(null);
  const [typeDecision, setTypeDecision] = useState<TypeDecision | null>(null);
  const [confirmedDocType, setConfirmedDocType] = useState<DocumentType | null>(null);
  const [engineLintReport, setEngineLintReport] = useState<LintReport | null>(null);
  const [engineFactCheckReport, setEngineFactCheckReport] = useState<FactCheckReport | null>(null);
  const [engineReportRevision, setEngineReportRevision] = useState<number | null>(null);
  const [pipeline, setPipeline] = useState<"draft" | "review" | "imitate" | "polish">("draft");
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [reviewStage, setReviewStage] = useState<ReviewStageId | null>(null);
  const [completedReviewStages, setCompletedReviewStages] = useState<ReviewStageId[]>([]);
  const [failedReviewStage, setFailedReviewStage] = useState<ReviewStageId | null>(null);
  const [reviewOutcome, setReviewOutcome] = useState<ReviewOutcome | null>(null);
  const [reviewError, setReviewError] = useState<string | null>(null);
  // Tracks the stage a failed review died on: the state value read inside the catch block would
  // still be whatever it was when the run started.
  const reviewStageRef = useRef<ReviewStageId | null>(null);
  // The draft pipelines have the same catch-block staleness problem, so the stage a failure
  // occurred on is tracked in a ref too, not in the render-time `draftStage` state.
  const draftStageRef = useRef<DraftStageId | null>(null);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const findings = useMemo(() => lintDocument(document), [document]);
  const displayedFindings = useMemo<Finding[]>(() => {
    if (engineReportRevision !== document.revision || (!engineLintReport && !engineFactCheckReport)) {
      return findings;
    }
    const engineFindings: Finding[] = [
      ...(engineLintReport?.findings ?? []),
      ...(engineFactCheckReport?.findings ?? []),
    ].map((finding) => ({
      id: finding.id,
      severity: finding.severity,
      nodeId: finding.nodeId,
      message: finding.message,
      location: finding.location,
      ...(finding.suggestion ? { suggestion: finding.suggestion } : {}),
      ...(finding.token ? { token: finding.token } : {}),
    }));
    return engineFindings;
  }, [document.revision, engineFactCheckReport, engineLintReport, engineReportRevision, findings]);
  const errorCount = displayedFindings.filter((finding) => finding.severity === "error").length;
  const desktopRuntime = isDesktopRuntime();

  useEffect(() => {
    const root = window.document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    backendVersion().then(setRuntime).catch(() => setRuntime("内核连接失败"));
    loadAppConfig()
      .then((loaded) => {
        setConfig(loaded);
        setTheme(loaded.theme);
        // The guide is a desktop first-run flow; browser preview stays quiet.
        if (isDesktopRuntime() && !loaded.firstRunCompleted) setGuideOpen(true);
      })
      .catch((error) => setNotice(ipcErrorMessage(error)));
    loadKbConfig()
      .then(setKbConfig)
      .catch((error) => setNotice(ipcErrorMessage(error)));
  }, [setTheme]);

  // Desktop: reopen the document the user last had open. The first-run guide, if any, sits on
  // top; a missing or deleted document falls back to the demo document without interrupting it.
  useEffect(() => {
    if (!isDesktopRuntime()) return;
    const lastId = useWorkbenchStore.getState().lastDocumentId;
    if (!lastId) return;
    loadDocument(lastId)
      .then((loaded) => {
        const current = useWorkbenchStore.getState().document;
        if (current.id === loaded.id) return;
        setDocument(loaded, Date.now());
        setSavedVersion(null);
        setNotice("已恢复上次打开的公文");
      })
      .catch(() => {
        // The last document may no longer exist; keep the demo document.
      });
  }, [setDocument, setNotice]);

  useEffect(() => {
    listProviderProfiles()
      .then((loaded) => {
        setProviders(loaded);
        setProviderId((current) => {
          const existing = loaded.find((provider) => provider.id === current && provider.enabled);
          if (existing) return existing.id;
          return loaded.find((provider) => provider.enabled && provider.isLocal)?.id
            ?? loaded.find((provider) => provider.enabled)?.id
            ?? loaded[0]?.id
            ?? "";
        });
      })
      .catch((error) => setNotice(ipcErrorMessage(error)));
  }, []);

  useEffect(() => {
    const restricted = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
    if (!restricted) return;
    const selected = providers.find((provider) => provider.id === providerId);
    if (selected?.isLocal && selected.enabled) return;
    const local = providers.find((provider) => provider.enabled && provider.isLocal);
    if (local) setProviderId(local.id);
  }, [document.meta.securityLevel, providerId, providers]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 2800);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  const refreshDocuments = useCallback(async (): Promise<DocumentSummary[]> => {
    setLibraryLoading(true);
    try {
      const loaded = await listDocuments();
      setDocuments(loaded);
      return loaded;
    } catch (error) {
      setNotice(ipcErrorMessage(error));
      return [];
    } finally {
      setLibraryLoading(false);
    }
  }, []);

  useEffect(() => {
    if (libraryOpen) void refreshDocuments();
  }, [libraryOpen, refreshDocuments]);

  const persistDocument = useCallback(
    async (stage: "user_save" | "autosave", quiet = false) => {
      const snapshot = useWorkbenchStore.getState().document;
      setSaving(true);
      try {
        const result = await saveDocument(snapshot, stage);
        const current = useWorkbenchStore.getState().document;
        if (current.id === snapshot.id && current.revision === snapshot.revision) {
          markSaved(result.savedAt);
          setSavedVersion(result.versionSeq);
        }
        if (!quiet) {
          setNotice(result.versionCreated ? `已保存版本 ${result.versionSeq}` : "文档内容已是最新");
        }
        if (libraryOpen) void refreshDocuments();
      } catch (error) {
        setNotice(ipcErrorMessage(error));
      } finally {
        setSaving(false);
      }
    },
    [libraryOpen, markSaved, refreshDocuments],
  );

  useEffect(() => {
    if (!dirty || !config.autosaveEnabled) return;
    const timeout = window.setTimeout(() => void persistDocument("autosave", true), 1200);
    return () => window.clearTimeout(timeout);
  }, [config.autosaveEnabled, dirty, document, persistDocument]);

  useEffect(() => {
    function handleShortcut(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void persistDocument("user_save");
      }
    }
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [persistDocument]);

  async function handleNew() {
    if (dirty && !window.confirm("当前修改尚未保存，仍要新建公文吗？")) return;
    try {
      const next = await createEmptyDocument();
      setDocument(next, null);
      setEngineLintReport(null);
      setEngineFactCheckReport(null);
      setEngineReportRevision(null);
      setSavedVersion(null);
      setView("structure");
      setNotice("已创建空白 ODoc");
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    }
  }

  async function handleImportDocx(file: File | null) {
    if (!file) return;
    if (dirty && !window.confirm("当前修改尚未保存，仍要导入其他公文吗？")) return;
    setSaving(true);
    try {
      const result = await importDocx(new Uint8Array(await file.arrayBuffer()));
      setDocument(result.document, result.saved.savedAt);
      setEngineLintReport(null);
      setEngineFactCheckReport(null);
      setEngineReportRevision(null);
      setSavedVersion(result.saved.versionSeq);
      setView("structure");
      setNotice("DOCX 已导入，复杂版式请在结构视图中复核");
      void refreshDocuments();
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    } finally {
      setSaving(false);
      if (importInputRef.current) importInputRef.current.value = "";
    }
  }

  async function handleOpenDocument(id: string) {
    if (id === document.id) {
      setLibraryOpen(false);
      return;
    }
    if (dirty && !window.confirm("当前修改尚未保存，仍要打开其他公文吗？")) return;
    setLibraryLoading(true);
    try {
      const loaded = await loadDocument(id);
      const summary = documents.find((item) => item.id === id);
      setDocument(loaded, summary?.updatedAt ?? Date.now());
      setEngineLintReport(null);
      setEngineFactCheckReport(null);
      setEngineReportRevision(null);
      setSavedVersion(summary?.versionCount ?? null);
      setLibraryOpen(false);
      setNotice("公文已打开");
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    } finally {
      setLibraryLoading(false);
    }
  }

  async function handleDeleteDocument(id: string) {
    const item = documents.find((candidate) => candidate.id === id);
    if (!window.confirm(`确定删除“${item?.title ?? "未命名公文"}”及其全部版本吗？此操作不可恢复。`)) return;
    try {
      await deleteDocument(id);
      if (id === document.id) {
        const next = await createEmptyDocument();
        setDocument(next, null);
        setEngineLintReport(null);
        setEngineFactCheckReport(null);
        setEngineReportRevision(null);
        setSavedVersion(null);
        setView("structure");
      }
      await refreshDocuments();
      setNotice("公文及版本已删除");
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    }
  }

  async function handleArchive(path: string, overwrite: boolean): Promise<ArchiveResult> {
    const snapshot = useWorkbenchStore.getState().document;
    setArchiveBusy(true);
    try {
      const saved = await saveDocument(snapshot, "archive");
      const current = useWorkbenchStore.getState().document;
      if (current.id === snapshot.id && current.revision === snapshot.revision) {
        markSaved(saved.savedAt);
        setSavedVersion(saved.versionSeq);
      }
      const result = await archiveDocumentToKb(snapshot.id, path, overwrite);
      setNotice(`已归档到 ${result.vault}/${result.path}`);
      return result;
    } finally {
      setArchiveBusy(false);
    }
  }

  function handleVersionRestored(result: RestoreVersionResult) {
    setDocument(result.document, result.saved.savedAt);
    setEngineLintReport(null);
    setEngineFactCheckReport(null);
    setEngineReportRevision(null);
    setSavedVersion(result.saved.versionSeq);
    setView("structure");
    setHistoryTarget(null);
    setLibraryOpen(false);
    setNotice(`已恢复为版本 ${result.saved.versionSeq}`);
    void refreshDocuments();
  }

  async function handleSaveConfig(next: AppConfig) {
    try {
      const normalized = {
        ...next,
        fontMappings: Object.fromEntries(
          Object.entries(next.fontMappings).filter(([, value]) => value?.trim()),
        ),
      } satisfies AppConfig;
      await saveAppConfig(normalized);
      setConfig(normalized);
      setTheme(normalized.theme);
      setNotice("设置已保存");
    } catch (error) {
      setNotice(ipcErrorMessage(error));
      throw error;
    }
  }

  async function saveBeforeDraft(): Promise<void> {
    const snapshot = useWorkbenchStore.getState().document;
    setSaving(true);
    try {
      const result = await saveDocument(snapshot, "autosave");
      const current = useWorkbenchStore.getState().document;
      if (current.id === snapshot.id && current.revision === snapshot.revision) {
        markSaved(result.savedAt);
        setSavedVersion(result.versionSeq);
      }
      if (libraryOpen) await refreshDocuments();
    } finally {
      setSaving(false);
    }
  }

  function applyDraftOutcome(outcome: DraftOutcome) {
    setDraftSessionId(outcome.sessionId);
    setDocument(outcome.document, Date.now());
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
    setEngineLintReport(outcome.lintReport);
    setEngineFactCheckReport(outcome.factCheckReport);
    setEngineReportRevision(outcome.document.revision);
    setTypeDecision(null);
    setConfirmedDocType(null);
    setDraftText("");
    setDraftError(null);
    setDraftStage(null);
    draftStageRef.current = null;
    setFailedStage(null);
    void refreshDocuments().then((loaded) => {
      setSavedVersion(loaded.find((item) => item.id === outcome.document.id)?.versionCount ?? null);
    });
  }

  function applyRerunOutcome(outcome: RerunOutcome, rerunStageId: DraftStageId) {
    setDocument(outcome.document, Date.now());
    // Only lint and factcheck report fresh results from a rerun; the panel falls back to live
    // deterministic findings while their reports are null, which also drops stale highlights.
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
    setEngineLintReport(outcome.lintReport);
    setEngineFactCheckReport(outcome.factCheckReport);
    setEngineReportRevision(outcome.document.revision);
    setTypeDecision(null);
    setConfirmedDocType(null);
    setDraftText("");
    setDraftError(null);
    setDraftStage(null);
    draftStageRef.current = null;
    setFailedStage(null);
    // Downstream stages ran against the previous output, so their records no longer hold.
    // The stage order depends on the session's pipeline kind, which the closure captured.
    const stageOrder: readonly DraftStageId[] = pipeline === "imitate"
      ? imitateStages
      : pipeline === "polish"
        ? polishStages
        : draftStages;
    const rerunIndex = stageOrder.indexOf(rerunStageId);
    setCompletedStages((current) =>
      current.filter((stage) => stageOrder.indexOf(stage) <= rerunIndex),
    );
    void refreshDocuments().then((loaded) => {
      setSavedVersion(loaded.find((item) => item.id === outcome.document.id)?.versionCount ?? null);
    });
  }

  function handleDraftEvent(event: DraftEvent) {
    if (event.type === "stage_started") {
      if (isDraftStage(event.stageId)) {
        draftStageRef.current = event.stageId;
        setDraftStage(event.stageId);
        setFailedStage(null);
        if (event.stageId === "draft" || event.stageId === "deai") setDraftText("");
      }
      return;
    }
    if (event.type === "token") {
      if (event.stageId === "draft" || event.stageId === "deai") setDraftText((current) => current + event.content);
      return;
    }
    if (event.type === "needs_document_type") {
      setTypeDecision(event.decision);
      setConfirmedDocType(event.decision.value);
      return;
    }
    if (event.type === "stage_completed") {
      const stageId = event.output.stageId;
      if (isDraftStage(stageId)) {
        setCompletedStages((current) => current.includes(stageId) ? current : [...current, stageId]);
        setDraftStage(stageId);
        if ((stageId === "draft" || stageId === "deai") && event.output.content) {
          setDraftText(event.output.content);
        }
      }
      return;
    }
    if (event.type === "completed") applyDraftOutcome(event.outcome);
  }

  async function runDraft(nextConfirmedDocType: DocumentType | null = null) {
    if (drafting || !material.trim() || !providerId) return;
    const selected = providers.find((provider) => provider.id === providerId);
    const restricted = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
    if (!selected?.enabled || (restricted && !selected.isLocal)) {
      setDraftError(restricted ? "当前密级仅允许本地模型，请先选择本地服务。" : "模型服务不可用，请先在设置中启用服务。");
      return;
    }
    setDrafting(true);
    setPipeline("draft");
    setDraftStage(null);
    draftStageRef.current = null;
    setCompletedStages([]);
    setFailedStage(null);
    setDraftText("");
    setDraftError(null);
    setDraftSessionId(null);
    if (nextConfirmedDocType === null) {
      setTypeDecision(null);
      setConfirmedDocType(null);
    }
    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await startDraft(
        {
          documentId: snapshot.id,
          material: material.trim(),
          providerId,
          confirmedDocType: nextConfirmedDocType,
        },
        handleDraftEvent,
      );
      // Older runtimes may return the outcome without delivering the final channel event. Apply
      // it only for the document that was drafted: if the user opened a different document while
      // this ran, its unsaved edits must not be clobbered by the finished draft.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyDraftOutcome(current.id, current.revision, outcome)) {
        applyDraftOutcome(outcome);
      }
    } catch (error) {
      const code = errorCode(error);
      if (code === "document_type_confirmation_required") {
        setDraftError(null);
      } else {
        setDraftError(ipcErrorMessage(error));
        if (isDraftStage(draftStageRef.current ?? "")) setFailedStage(draftStageRef.current);
      }
    } finally {
      setDrafting(false);
    }
  }

  async function runImitate() {
    if (drafting || !material.trim() || !modelText.trim() || !providerId) return;
    if (!desktopRuntime) {
      setDraftError("模型调用仅在桌面应用中可用。");
      return;
    }
    const selected = providers.find((provider) => provider.id === providerId);
    const restricted = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
    if (!selected?.enabled || (restricted && !selected.isLocal)) {
      setDraftError(restricted ? "当前密级仅允许本地模型，请先选择本地服务。" : "模型服务不可用，请先在设置中启用服务。");
      return;
    }
    setDrafting(true);
    setPipeline("imitate");
    setDraftStage(null);
    draftStageRef.current = null;
    setCompletedStages([]);
    setFailedStage(null);
    setDraftText("");
    setDraftError(null);
    setDraftSessionId(null);
    setTypeDecision(null);
    setConfirmedDocType(null);
    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await startImitate(
        {
          documentId: snapshot.id,
          material: material.trim(),
          providerId,
          modelText: modelText.trim(),
          confirmedDocType: null,
        },
        handleDraftEvent,
      );
      // Older runtimes may return the outcome without delivering the final channel event. Apply
      // it only for the document that was drafted: if the user opened a different document while
      // this ran, its unsaved edits must not be clobbered by the finished draft.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyDraftOutcome(current.id, current.revision, outcome)) {
        applyDraftOutcome(outcome);
      }
    } catch (error) {
      const code = errorCode(error);
      if (code === "document_type_confirmation_required") {
        setDraftError(null);
      } else {
        setDraftError(ipcErrorMessage(error));
        if (isDraftStage(draftStageRef.current ?? "")) setFailedStage(draftStageRef.current);
      }
    } finally {
      setDrafting(false);
    }
  }

  async function runPolish() {
    if (drafting || !material.trim() || !providerId) return;
    if (!desktopRuntime) {
      setDraftError("模型调用仅在桌面应用中可用。");
      return;
    }
    const selected = providers.find((provider) => provider.id === providerId);
    const restricted = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
    if (!selected?.enabled || (restricted && !selected.isLocal)) {
      setDraftError(restricted ? "当前密级仅允许本地模型，请先选择本地服务。" : "模型服务不可用，请先在设置中启用服务。");
      return;
    }
    setDrafting(true);
    setPipeline("polish");
    setDraftStage(null);
    draftStageRef.current = null;
    setCompletedStages([]);
    setFailedStage(null);
    setDraftText("");
    setDraftError(null);
    setDraftSessionId(null);
    setTypeDecision(null);
    setConfirmedDocType(null);
    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await startPolish(
        {
          documentId: snapshot.id,
          material: material.trim(),
          providerId,
          confirmedDocType: null,
        },
        handleDraftEvent,
      );
      // Older runtimes may return the outcome without delivering the final channel event. Apply
      // it only for the document that was drafted: if the user opened a different document while
      // this ran, its unsaved edits must not be clobbered by the finished draft.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyDraftOutcome(current.id, current.revision, outcome)) {
        applyDraftOutcome(outcome);
      }
    } catch (error) {
      const code = errorCode(error);
      if (code === "document_type_confirmation_required") {
        setDraftError(null);
      } else {
        setDraftError(ipcErrorMessage(error));
        if (isDraftStage(draftStageRef.current ?? "")) setFailedStage(draftStageRef.current);
      }
    } finally {
      setDrafting(false);
    }
  }

  async function rerunPipelineStage(stageId: DraftStageId) {
    if (drafting || reviewing || !draftSessionId) return;
    if (!desktopRuntime) {
      setDraftError("模型调用仅在桌面应用中可用。");
      return;
    }
    setDrafting(true);
    setDraftStage(stageId);
    draftStageRef.current = stageId;
    setFailedStage(null);
    setDraftError(null);
    if (stageId === "draft" || stageId === "deai") setDraftText("");
    try {
      const outcome = await rerunStage(draftSessionId, stageId, handleDraftEvent);
      // The rerun streams only stage events, so the returned document is applied here; the
      // content guard keeps this idempotent if a future runtime also delivers a final event.
      // Like the full runs, never write back into a document the user has since switched away
      // from — the rerun's snapshot belongs to the session's document, not the active one.
      const current = useWorkbenchStore.getState().document;
      if (shouldApplyRerunOutcome(current, outcome)) {
        applyRerunOutcome(outcome, stageId);
      }
    } catch (error) {
      const code = errorCode(error);
      if (code === "document_type_confirmation_required") {
        setDraftError(null);
      } else {
        setDraftError(ipcErrorMessage(error));
        if (isDraftStage(draftStageRef.current ?? "")) setFailedStage(draftStageRef.current);
      }
    } finally {
      setDrafting(false);
    }
  }

  function handleReviewEvent(event: ReviewEvent) {
    if (event.type === "stage_started") {
      if (isReviewStage(event.stageId)) {
        reviewStageRef.current = event.stageId;
        setReviewStage(event.stageId);
        setFailedReviewStage(null);
      }
      return;
    }
    if (event.type === "stage_completed") {
      const stageId = event.output.stageId;
      if (isReviewStage(stageId)) {
        setCompletedReviewStages((current) => current.includes(stageId) ? current : [...current, stageId]);
        setReviewStage(stageId);
      }
      return;
    }
    if (event.type === "completed") applyReviewOutcome(event.outcome);
  }

  function applyReviewOutcome(outcome: ReviewOutcome) {
    setReviewOutcome(outcome);
    reviewStageRef.current = null;
    setReviewStage(null);
    setFailedReviewStage(null);
    // The kernel reports on the stored revision, so its findings also drive the lint panel.
    setEngineLintReport(outcome.lintReport);
    setEngineFactCheckReport(outcome.factCheckReport);
    setEngineReportRevision(outcome.revision);
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
  }

  async function runReview() {
    if (reviewing) return;
    setReviewOpen(true);
    setPipeline("review");
    if (!desktopRuntime) {
      setReviewError("公文审核仅在桌面应用中可用。");
      return;
    }
    const selected = providers.find((provider) => provider.id === providerId);
    const restricted = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
    if (!selected?.enabled || (restricted && !selected.isLocal)) {
      setReviewError(restricted ? "当前密级仅允许本地模型，请先选择本地服务。" : "模型服务不可用，请先在设置中启用服务。");
      return;
    }
    setReviewing(true);
    setReviewError(null);
    setReviewOutcome(null);
    reviewStageRef.current = null;
    setReviewStage(null);
    setCompletedReviewStages([]);
    setFailedReviewStage(null);
    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await startReview({ documentId: snapshot.id, providerId }, handleReviewEvent);
      // Older runtimes may return the outcome without delivering the final channel event.
      applyReviewOutcome(outcome);
    } catch (error) {
      setReviewError(ipcErrorMessage(error));
      setFailedReviewStage(reviewStageRef.current);
    } finally {
      setReviewing(false);
    }
  }

  function locateReviewNode(nodeId: string) {
    setReviewOpen(false);
    setView("structure");
    selectNode(nodeId);
    requestAnimationFrame(() =>
      window.document.querySelector<HTMLElement>(`[data-node-id="${nodeId}"]`)?.scrollIntoView({ block: "center" }));
  }

  async function completeGuide() {
    const next = { ...config, firstRunCompleted: true };
    try {
      await saveAppConfig(next);
      setConfig(next);
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    }
    setGuideOpen(false);
  }

  async function handleCloseSettings() {
    setSettingsOpen(false);
    try {
      const [loadedProviders, loadedKbConfig] = await Promise.all([
        listProviderProfiles(),
        loadKbConfig(),
      ]);
      setProviders(loadedProviders);
      setKbConfig(loadedKbConfig);
      setProviderId((current) => loadedProviders.some((provider) => provider.id === current)
        ? current
        : loadedProviders.find((provider) => provider.enabled)?.id ?? loadedProviders[0]?.id ?? "");
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    }
  }

  async function handleThemeChange(nextTheme: AppConfig["theme"]) {
    const previousConfig = config;
    const nextConfig = { ...config, theme: nextTheme };
    setConfig(nextConfig);
    setTheme(nextTheme);
    try {
      await saveAppConfig(nextConfig);
    } catch (error) {
      setConfig(previousConfig);
      setTheme(previousConfig.theme);
      setNotice(ipcErrorMessage(error));
    }
  }

  async function handleExport() {
    if (errorCount) return;
    const baseName = document.body.title.text.trim() || "未命名公文";
    const snapshot = useWorkbenchStore.getState().document;
    setSaving(true);
    try {
      const saved = await saveDocument(snapshot, "export");
      const current = useWorkbenchStore.getState().document;
      if (current.id === snapshot.id && current.revision === snapshot.revision) {
        markSaved(saved.savedAt);
        setSavedVersion(saved.versionSeq);
      }
      if (desktopRuntime) {
        const result = await exportDocx(snapshot.id);
        downloadBytes(`${baseName}.docx`, Uint8Array.from(result.bytes), "application/vnd.openxmlformats-officedocument.wordprocessingml.document");
        setNotice(result.fallbackReason ?? (result.usedTemplate ? "DOCX 已按机关模板导出" : "DOCX 已按 GB/T 9704 导出"));
      } else {
        downloadDocument(`${baseName}.odoc.json`, JSON.stringify(snapshot, null, 2));
        setNotice("浏览器版已导出 ODoc JSON");
      }
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  async function handlePrint() {
    if (errorCount) return;
    const baseName = document.body.title.text.trim() || "未命名公文";
    if (!desktopRuntime) {
      setView("preview");
      window.setTimeout(() => window.print(), 0);
      return;
    }
    setSaving(true);
    try {
      const snapshot = useWorkbenchStore.getState().document;
      const saved = await saveDocument(snapshot, "pdf_export");
      // Only acknowledge the save if the document has not moved on while the export ran;
      // otherwise the newest edits would be marked as saved and never auto-saved again.
      const current = useWorkbenchStore.getState().document;
      if (current.id === snapshot.id && current.revision === snapshot.revision) {
        markSaved(saved.savedAt);
        setSavedVersion(saved.versionSeq);
      }
      const result = await exportPdf(snapshot.id);
      if (result.kind === "pdf") {
        downloadBytes(`${baseName}.pdf`, Uint8Array.from(result.bytes), "application/pdf");
        setNotice("PDF 已通过内置排版引擎导出");
      } else {
        const preview = window.open("", "_blank", "noopener,noreferrer");
        if (!preview) throw { code: "popup_blocked", message: "浏览器阻止了打印预览窗口。" };
        preview.document.open();
        preview.document.write(result.html);
        preview.document.close();
        preview.focus();
        window.setTimeout(() => preview.print(), 120);
        setNotice("未找到可用中文字体，已打开打印预览");
      }
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  const savedLabel = saving
    ? "正在保存"
    : dirty
      ? "有未保存修改"
      : savedAt
        ? `已保存 ${new Date(savedAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
        : "尚未保存";
  const cloudAllowed = document.meta.securityLevel === "public" || document.meta.securityLevel === "internal";
  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const canReview = desktopRuntime && Boolean(selectedProvider?.enabled) && !drafting && !reviewing;

  return (
    <div className="app-shell">
      <Toolbar
        findings={displayedFindings}
        onNew={handleNew}
        onImport={() => importInputRef.current?.click()}
        onSave={() => void persistDocument("user_save")}
        onExport={handleExport}
        onPrint={() => void handlePrint()}
        onReview={() => void runReview()}
        onArchive={() => setArchiveOpen(true)}
        onOpenHistory={() => setHistoryTarget({
          id: document.id,
          title: document.body.title.text.trim() || "未命名公文",
        })}
        onOpenDocuments={() => setLibraryOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
        onThemeChange={handleThemeChange}
        exportFormat={desktopRuntime ? "docx" : "odoc"}
        pipeline={pipeline}
        activeStage={drafting ? draftStage : null}
        completedStages={completedStages}
        failedStage={failedStage}
        activeReviewStage={reviewing ? reviewStage : null}
        completedReviewStages={completedReviewStages}
        failedReviewStage={failedReviewStage}
        canReview={canReview}
        canArchive={desktopRuntime && kbConfig.enabled}
        hasVersions={savedVersion !== null && savedVersion > 0}
        onRerunStage={(stage) => void rerunPipelineStage(stage)}
        canRerunStage={desktopRuntime && !drafting && !reviewing && draftSessionId !== null}
      />
      <input
        ref={importInputRef}
        className="visually-hidden"
        type="file"
        accept=".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        onChange={(event) => void handleImportDocx(event.target.files?.[0] ?? null)}
      />
      <main className="workspace">
        <IntakeSidebar
          material={material}
          onMaterialChange={setMaterial}
          modelText={modelText}
          onModelTextChange={setModelText}
          onStartImitate={() => void runImitate()}
          onStartPolish={() => void runPolish()}
          providers={providers}
          providerId={providerId}
          onProviderChange={setProviderId}
          onStartDraft={() => void runDraft()}
          drafting={drafting}
          draftStage={draftStage}
          draftText={draftText}
          draftError={draftError}
          typeDecision={typeDecision}
          confirmedDocType={confirmedDocType}
          onConfirmedDocTypeChange={setConfirmedDocType}
          onConfirmDocType={() => confirmedDocType && void runDraft(confirmedDocType)}
          desktopRuntime={desktopRuntime}
          kbEnabled={desktopRuntime && kbConfig.enabled}
        />
        <section className="center-workspace" aria-label="公文编辑区">
          <nav className="view-tabs" aria-label="文档视图">
            <button className={view === "preview" ? "view-tab is-active" : "view-tab"} type="button" onClick={() => setView("preview")}>
              <LayoutTemplate size={15} />
              版式预览
            </button>
            <button className={view === "structure" ? "view-tab is-active" : "view-tab"} type="button" onClick={() => setView("structure")}>
              <Code2 size={15} />
              结构编辑
            </button>
            <div className="view-meta">
              <span>{documentTypeLabels[document.meta.docType]}</span>
              <span>修订 {document.revision}</span>
              {savedVersion !== null && <span>版本 {savedVersion}</span>}
            </div>
          </nav>
          <div className="document-canvas">{view === "preview" ? <DocumentPreview /> : <StructureEditor />}</div>
        </section>
        <LintPanel findings={displayedFindings} />
      </main>
      <footer className="statusbar">
        <span className={cloudAllowed ? "status-item" : "status-item is-alert"}>
          {cloudAllowed ? <LockKeyhole size={13} /> : <ShieldAlert size={13} />}
          {cloudAllowed ? "允许配置的云端模型" : "密级限制：仅允许本地模型"}
        </span>
        <span className="status-item">
          <FileJson2 size={13} /> ODoc {document.schemaVersion}
        </span>
        <span className="status-item runtime-status">{runtime}</span>
        <span className="status-spacer" />
        <span className="status-item">{countChineseCharacters(document)} 字</span>
        <span className={dirty ? "status-item is-unsaved" : "status-item"}>
          <Save size={13} /> {savedLabel}
        </span>
      </footer>
      <DocumentLibrary
        open={libraryOpen}
        documents={documents}
        currentDocumentId={document.id}
        loading={libraryLoading}
        onClose={() => setLibraryOpen(false)}
        onRefresh={() => void refreshDocuments()}
        onOpen={(id) => void handleOpenDocument(id)}
        onHistory={(id, title) => setHistoryTarget({ id, title })}
        onSessions={(id, title) => setSessionTarget({ id, title })}
        onDelete={(id) => void handleDeleteDocument(id)}
      />
      <ArchiveDialog
        open={archiveOpen}
        title={document.body.title.text}
        vault={kbConfig.vault}
        busy={archiveBusy}
        onClose={() => setArchiveOpen(false)}
        onArchive={handleArchive}
      />
      <VersionHistoryDialog
        open={historyTarget !== null}
        documentId={historyTarget?.id ?? null}
        title={historyTarget?.title ?? ""}
        onClose={() => setHistoryTarget(null)}
        onRestored={handleVersionRestored}
      />
      <SessionHistoryDialog
        open={sessionTarget !== null}
        documentId={sessionTarget?.id ?? null}
        title={sessionTarget?.title ?? ""}
        onClose={() => setSessionTarget(null)}
      />
      <SettingsDialog
        open={settingsOpen}
        config={config}
        onClose={() => void handleCloseSettings()}
        onSave={handleSaveConfig}
        onShowGuide={() => {
          setSettingsOpen(false);
          setGuideOpen(true);
        }}
      />
      <WelcomeGuide
        open={guideOpen}
        onComplete={() => void completeGuide()}
        onOpenSettings={() => {
          setGuideOpen(false);
          setSettingsOpen(true);
        }}
        onSelectProvider={(providerId) => setProviderId(providerId)}
      />
      <ReviewReportDialog
        open={reviewOpen}
        outcome={reviewOutcome}
        documentTitle={document.body.title.text}
        running={reviewing}
        stage={reviewStage}
        completedStages={completedReviewStages}
        error={reviewError}
        canRerun={canReview}
        onClose={() => setReviewOpen(false)}
        onLocate={locateReviewNode}
        onRerun={() => void runReview()}
      />
      {notice && <div className="toast" role="status">{notice}</div>}
    </div>
  );
}

export default App;
