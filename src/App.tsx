import { LayoutTemplate, LockKeyhole, PenLine, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArchiveDialog } from "./features/documents/ArchiveDialog";
import { DocumentLibrary } from "./features/documents/DocumentLibrary";
import { VersionHistoryDialog } from "./features/documents/VersionHistoryDialog";
import { StructureEditor } from "./features/editor/StructureEditor";
import { WelcomeGuide } from "./features/onboarding/WelcomeGuide";
import { DocumentPreview } from "./features/preview/DocumentPreview";
import { ReviewReportDialog } from "./features/review/ReviewReportDialog";
import { SessionHistoryDialog } from "./features/documents/SessionHistoryDialog";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { AppHeader } from "./features/workspace/AppHeader";
import { DocumentInfoDialog } from "./features/workspace/DocumentInfoDialog";
import { QualityDrawer } from "./features/workspace/QualityDrawer";
import { WriteHome } from "./features/workspace/WriteHome";
import { CommandPalette, type CommandAction } from "./features/workbench/CommandPalette";
import { UpdateBanner } from "./features/workbench/UpdateBanner";
import { useDraftPipeline } from "./hooks/useDraftPipeline";
import { countChineseCharacters } from "./lib/document";
import { isReviewStage, startReview, type ReviewStageId } from "./lib/review";
import {
  backendVersion,
  archiveDocumentToKb,
  applyUpdate,
  checkForUpdate,
  createEmptyDocument,
  defaultAppConfig,
  defaultKbConfig,
  deleteDocument,
  exportDocx,
  exportPdf,
  importDocx,
  ipcErrorMessage,
  IpcError,
  isDesktopRuntime,
  listDocuments,
  loadKbConfig,
  listProviderProfiles,
  lintDocumentNow,
  loadAppConfig,
  loadDocument,
  saveAppConfig,
  saveDocument,
  type ArchiveResult,
  type RestoreVersionResult,
  type UpdateCheckResult,
} from "./lib/ipc";
import type { DraftOutcome, RerunOutcome, ReviewEvent, ReviewOutcome } from "./lib/generated/engine";
import type { FactCheckReport, LintReport } from "./lib/generated/lint";
import type { ProviderProfile } from "./lib/llm";
import type { KbConfig } from "./lib/kb";
import type { DocumentType, ODoc } from "./lib/odoc";
import { lintDocument, unverifiedTokenMap } from "./lib/lint";
import { selectFindings, type RevisionedReport } from "./lib/report";
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

/** True when starting a new draft can safely reuse the current document instead of replacing it. */
function isBlankDocument(document: ODoc): boolean {
  const hasHeader = Boolean(
    document.header?.issuingAuthorityMark?.trim()
    || document.header?.documentNumber
    || document.header?.signer,
  );
  const hasBody = document.body.blocks.some((block) => {
    if (block.type === "heading" || block.type === "paragraph" || block.type === "quote") {
      return block.content.trim().length > 0;
    }
    if (block.type === "list") return block.items.some((item) => item.content.trim().length > 0);
    return false;
  });
  return !document.body.title.text.trim()
    && !hasHeader
    && !hasBody
    && document.body.mainRecipients.length === 0
    && !document.body.signer
    && !document.body.date
    && document.facts.facts.length === 0;
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
  const updateTitle = useWorkbenchStore((state) => state.updateTitle);
  const showGrid = useWorkbenchStore((state) => state.showGrid);
  const toggleGrid = useWorkbenchStore((state) => state.toggleGrid);
  const importInputRef = useRef<HTMLInputElement>(null);
  const [runtime, setRuntime] = useState("正在连接内核");
  const [notice, setNotice] = useState<string | null>(null);
  const [config, setConfig] = useState<AppConfig>(defaultAppConfig);
  const [kbConfig, setKbConfig] = useState<KbConfig>(defaultKbConfig);
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<"general" | "providers" | "clinepass" | "knowledge" | "packs">("general");
  const [guideOpen, setGuideOpen] = useState(false);
  const [composing, setComposing] = useState(true);
  const [qualityOpen, setQualityOpen] = useState(false);
  const [infoOpen, setInfoOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<UpdateCheckResult | null>(null);
  const [installingUpdate, setInstallingUpdate] = useState(false);
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
  const [engineLintReport, setEngineLintReport] = useState<RevisionedReport<LintReport> | null>(null);
  const [engineFactCheckReport, setEngineFactCheckReport] = useState<RevisionedReport<FactCheckReport> | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [reviewStage, setReviewStage] = useState<ReviewStageId | null>(null);
  const [completedReviewStages, setCompletedReviewStages] = useState<ReviewStageId[]>([]);
  const [reviewOutcome, setReviewOutcome] = useState<ReviewOutcome | null>(null);
  const [reviewError, setReviewError] = useState<string | null>(null);
  // Tracks the stage a failed review died on: the state value read inside the catch block would
  // still be whatever it was when the run started.
  const reviewStageRef = useRef<ReviewStageId | null>(null);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const findings = useMemo(() => lintDocument(document), [document]);
  const displayedFindings = useMemo(
    () => selectFindings(document.id, document.revision, findings, engineLintReport, engineFactCheckReport),
    [document.id, document.revision, engineFactCheckReport, engineLintReport, findings],
  );
  const errorCount = displayedFindings.filter((finding) => finding.severity === "error").length;
  const desktopRuntime = isDesktopRuntime();

  useEffect(() => {
    const root = window.document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  }, [theme]);

  // Desktop: keep the panel on the kernel's pack-driven rules. The local rule subset still
  // renders instantly while the debounced kernel check runs, so typing stays responsive.
  const lintFailureLogged = useRef(false);
  useEffect(() => {
    if (!desktopRuntime) return;
    if (engineLintReport?.documentId === document.id && engineLintReport.revision === document.revision) return;
    const snapshot = document;
    const timeout = window.setTimeout(() => {
      lintDocumentNow(snapshot)
        .then((report) => {
          setEngineLintReport((current) =>
            current && current.documentId === snapshot.id && current.revision >= snapshot.revision
              ? current
              : { documentId: snapshot.id, revision: snapshot.revision, report },
          );
        })
        .catch(() => {
          if (lintFailureLogged.current) return;
          lintFailureLogged.current = true;
          console.warn("kernel lint unavailable; showing the local rule subset");
        });
    }, 250);
    return () => window.clearTimeout(timeout);
  }, [desktopRuntime, document, engineLintReport]);

  // Desktop: check the signed release channel once per start when auto-update is enabled. A
  // failure stays silent; the settings page still offers a manual check with an explicit error.
  useEffect(() => {
    if (!desktopRuntime || !config.autoUpdateEnabled) return;
    let active = true;
    const timer = window.setTimeout(() => {
      checkForUpdate()
        .then((result) => {
          if (active && result.available) setAvailableUpdate(result);
        })
        .catch(() => {});
    }, 2500);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [config.autoUpdateEnabled, desktopRuntime]);

  async function installAvailableUpdate() {
    if (installingUpdate) return;
    setInstallingUpdate(true);
    try {
      await applyUpdate();
    } catch (error) {
      setNotice(ipcErrorMessage(error));
    } finally {
      setInstallingUpdate(false);
    }
  }

  // Fact-check highlights describe one revision; once the text moves on they must not linger.
  useEffect(() => {
    if (!engineFactCheckReport) return;
    if (engineFactCheckReport.documentId === document.id && engineFactCheckReport.revision === document.revision) return;
    if (Object.keys(useWorkbenchStore.getState().unverifiedTokens).length === 0) return;
    setUnverifiedTokens({});
  }, [document.id, document.revision, engineFactCheckReport, setUnverifiedTokens]);

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
        setNotice("已恢复上次打开的公文，可继续编辑");
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
    if (libraryOpen || composing) void refreshDocuments();
  }, [libraryOpen, composing, refreshDocuments]);

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
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen((current) => !current);
        return;
      }
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
      setSavedVersion(null);
      setView("structure");
      setMaterial("");
      setComposing(true);
      setQualityOpen(false);
      setNotice("已新建空白文档，从写作台开始吧");
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
      setSavedVersion(result.saved.versionSeq);
      setView("structure");
      setComposing(false);
      setNotice("DOCX 已导入，复杂版式请在编辑视图中复核");
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
      setSavedVersion(summary?.versionCount ?? null);
      setLibraryOpen(false);
      setComposing(false);
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
        setSavedVersion(null);
        setView("structure");
        setMaterial("");
        setComposing(true);
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
    setSavedVersion(result.saved.versionSeq);
    setView("structure");
    setComposing(false);
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
    setDocument(outcome.document, Date.now());
    setComposing(false);
    setView("preview");
    setQualityOpen(false);
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
    setEngineLintReport(
      outcome.lintReport
        ? { documentId: outcome.document.id, revision: outcome.document.revision, report: outcome.lintReport }
        : null,
    );
    setEngineFactCheckReport(
      outcome.factCheckReport
        ? { documentId: outcome.document.id, revision: outcome.document.revision, report: outcome.factCheckReport }
        : null,
    );
    void refreshDocuments().then((loaded) => {
      setSavedVersion(loaded.find((item) => item.id === outcome.document.id)?.versionCount ?? null);
    });
  }

  function applyRerunOutcome(outcome: RerunOutcome) {
    setDocument(outcome.document, Date.now());
    // Only lint and factcheck report fresh results from a rerun; the panel falls back to live
    // deterministic findings while their reports are null, which also drops stale highlights.
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
    setEngineLintReport(
      outcome.lintReport
        ? { documentId: outcome.document.id, revision: outcome.document.revision, report: outcome.lintReport }
        : null,
    );
    setEngineFactCheckReport(
      outcome.factCheckReport
        ? { documentId: outcome.document.id, revision: outcome.document.revision, report: outcome.factCheckReport }
        : null,
    );
    void refreshDocuments().then((loaded) => {
      setSavedVersion(loaded.find((item) => item.id === outcome.document.id)?.versionCount ?? null);
    });
  }

  const draftPipeline = useDraftPipeline({
    document,
    providers,
    providerId,
    material,
    modelText: "",
    templateId: "",
    saveBeforeDraft,
    onDraftOutcome: applyDraftOutcome,
    onRerunOutcome: applyRerunOutcome,
  });

  function beginDraft(
    confirmedDocType: DocumentType | null = null,
    materialOverride?: string,
    templateIdOverride?: string | null,
  ) {
    void draftPipeline.startDraft(confirmedDocType, materialOverride, templateIdOverride);
  }

  async function handleStartWriting(docType: DocumentType | null) {
    if (draftPipeline.drafting) return;
    if (!material.trim()) {
      setNotice("先写一下想生成什么，再开始");
      return;
    }
    const current = useWorkbenchStore.getState().document;
    if (!isBlankDocument(current)) {
      if (useWorkbenchStore.getState().dirty && !window.confirm("当前修改尚未保存，仍要新建一篇吗？")) return;
      try {
        const next = await createEmptyDocument();
        setDocument(next, null);
        setEngineLintReport(null);
        setEngineFactCheckReport(null);
        setSavedVersion(null);
      } catch (error) {
        setNotice(ipcErrorMessage(error));
        return;
      }
    }
    void draftPipeline.startDraft(docType, material);
  }

  function handleReviewEvent(event: ReviewEvent) {
    if (event.type === "stage_started") {
      if (isReviewStage(event.stageId)) {
        reviewStageRef.current = event.stageId;
        setReviewStage(event.stageId);
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
    // The kernel reports on the stored revision, so its findings also drive the lint panel.
    setEngineLintReport({
      documentId: outcome.documentId,
      revision: outcome.revision,
      report: outcome.lintReport,
    });
    setEngineFactCheckReport({
      documentId: outcome.documentId,
      revision: outcome.revision,
      report: outcome.factCheckReport,
    });
    setUnverifiedTokens(unverifiedTokenMap(outcome.factCheckReport?.findings ?? []));
  }

  async function runReview() {
    if (reviewing) return;
    setReviewOpen(true);
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
    try {
      await saveBeforeDraft();
      const snapshot = useWorkbenchStore.getState().document;
      const outcome = await startReview({ documentId: snapshot.id, providerId }, handleReviewEvent);
      // Older runtimes may return the outcome without delivering the final channel event.
      applyReviewOutcome(outcome);
    } catch (error) {
      setReviewError(ipcErrorMessage(error));
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
      setProviderId((current) => loadedProviders.some((provider) => provider.id === current && provider.enabled)
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
        if (!preview) throw new IpcError("popup_blocked", "浏览器阻止了打印预览窗口。");
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
  const canReview = desktopRuntime && Boolean(selectedProvider?.enabled) && !draftPipeline.drafting && !reviewing;

  const commandActions: CommandAction[] = [
    {
      id: "home",
      label: "回到写作台",
      hint: "只写要办的事，其余交给模型",
      keywords: ["home", "写作", "起草", "新建"],
      run: () => setComposing(true),
    },
    { id: "new", label: "新建公文", keywords: ["new", "新建"], run: () => void handleNew() },
    { id: "documents", label: "打开最近文档", keywords: ["document", "文档"], run: () => setLibraryOpen(true) },
    { id: "save", label: "保存到本机", hint: "Ctrl+S", run: () => void persistDocument("user_save") },
    {
      id: "export",
      label: desktopRuntime ? "导出 DOCX" : "导出 ODoc JSON",
      keywords: ["export", "导出"],
      run: () => void handleExport(),
    },
    { id: "print", label: "打印 / 导出 PDF", run: () => void handlePrint() },
    {
      id: "review",
      label: "审核当前公文",
      hint: "规则校验 + 语义评审",
      disabled: !canReview,
      run: () => void runReview(),
    },
    {
      id: "clinepass",
      label: "Cline Pass 订阅",
      hint: "额度、模型目录与订阅入口",
      keywords: ["cline", "订阅", "国模"],
      run: () => {
        setSettingsTab("clinepass");
        setSettingsOpen(true);
      },
    },
    {
      id: "settings",
      label: "设置",
      keywords: ["settings", "配置"],
      run: () => {
        setSettingsTab("general");
        setSettingsOpen(true);
      },
    },
    {
      id: "theme",
      label: "切换主题",
      hint: "跟随系统 / 亮色 / 暗色",
      run: () => void handleThemeChange(theme === "system" ? "light" : theme === "light" ? "dark" : "system"),
    },
    {
      id: "preview",
      label: "切换到版式预览",
      disabled: view === "preview",
      run: () => setView("preview"),
    },
    {
      id: "structure",
      label: "切换到正文编辑",
      disabled: view === "structure",
      run: () => setView("structure"),
    },
    {
      id: "document-info",
      label: "文档信息（可选）",
      hint: "发文字号、主送机关、落款等",
      disabled: composing,
      run: () => setInfoOpen(true),
    },
  ];

  return (
    <div className={composing ? "app-shell is-home" : "app-shell"}>
      <AppHeader
        composing={composing}
        title={document.body.title.text}
        docType={document.meta.docType}
        dirty={dirty}
        saving={saving}
        savedLabel={savedLabel}
        errorCount={errorCount}
        canReview={canReview}
        canArchive={desktopRuntime && kbConfig.enabled}
        hasVersions={savedVersion !== null && savedVersion > 0}
        showGrid={showGrid}
        theme={theme}
        drafting={draftPipeline.drafting}
        stage={draftPipeline.stage}
        completedStages={draftPipeline.completedStages}
        reviewing={reviewing}
        reviewStage={reviewStage}
        completedReviewStages={completedReviewStages}
        onTitleChange={updateTitle}
        onHome={() => setComposing(true)}
        onNew={() => void handleNew()}
        onSave={() => void persistDocument("user_save")}
        onExport={() => void handleExport()}
        onImport={() => importInputRef.current?.click()}
        onPrint={() => void handlePrint()}
        onOpenDocuments={() => setLibraryOpen(true)}
        onOpenHistory={() => setHistoryTarget({
          id: document.id,
          title: document.body.title.text.trim() || "未命名公文",
        })}
        onOpenInfo={() => setInfoOpen(true)}
        onOpenSettings={() => {
          setSettingsTab("general");
          setSettingsOpen(true);
        }}
        onReview={() => void runReview()}
        onArchive={() => setArchiveOpen(true)}
        onToggleGrid={toggleGrid}
        onThemeChange={() => void handleThemeChange(theme === "system" ? "light" : theme === "light" ? "dark" : "system")}
      />
      <input
        ref={importInputRef}
        className="visually-hidden"
        type="file"
        accept=".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        onChange={(event) => void handleImportDocx(event.target.files?.[0] ?? null)}
      />
      <main className={composing ? "app-main is-home" : "app-main"}>
        {composing ? (
          <WriteHome
            material={material}
            onMaterialChange={setMaterial}
            providers={providers}
            providerId={providerId}
            onProviderChange={setProviderId}
            desktop={desktopRuntime}
            drafting={draftPipeline.drafting}
            stage={draftPipeline.stage}
            completedStages={draftPipeline.completedStages}
            draftText={draftPipeline.text}
            error={draftPipeline.error}
            typeDecision={draftPipeline.typeDecision}
            confirmedDocType={draftPipeline.confirmedDocType}
            onConfirmedDocTypeChange={draftPipeline.setConfirmedDocType}
            onConfirmDocType={() => {
              if (draftPipeline.confirmedDocType) beginDraft(draftPipeline.confirmedDocType);
            }}
            onStart={(docType) => void handleStartWriting(docType)}
            onOpenSettings={() => {
              setSettingsTab("providers");
              setSettingsOpen(true);
            }}
            onOpenDocuments={() => setLibraryOpen(true)}
            onOpenRecent={(id) => void handleOpenDocument(id)}
            recentDocuments={documents}
            documentId={document.id}
            kbEnabled={desktopRuntime && kbConfig.enabled}
            currentTitle={document.body.title.text}
            onContinueDocument={() => setComposing(false)}
          />
        ) : (
          <section className="document-workspace" aria-label="公文编辑区">
            <div className="document-bar">
              <div className="view-switch" role="tablist" aria-label="文档视图">
                <button
                  className={view === "structure" ? "view-tab is-active" : "view-tab"}
                  type="button"
                  role="tab"
                  aria-selected={view === "structure"}
                  onClick={() => setView("structure")}
                >
                  <PenLine size={14} />
                  编辑
                </button>
                <button
                  className={view === "preview" ? "view-tab is-active" : "view-tab"}
                  type="button"
                  role="tab"
                  aria-selected={view === "preview"}
                  onClick={() => setView("preview")}
                >
                  <LayoutTemplate size={14} />
                  版式
                </button>
              </div>
              <div className="document-bar-meta">
                <span>{countChineseCharacters(document)} 字</span>
                <button
                  className={errorCount ? "quality-trigger is-error" : "quality-trigger"}
                  type="button"
                  onClick={() => setQualityOpen(true)}
                >
                  质检{errorCount ? ` · ${errorCount} 项错误` : ""}
                </button>
              </div>
            </div>
            <div className="document-canvas">
              {view === "preview" ? (
                <DocumentPreview />
              ) : (
                <StructureEditor
                  providerId={providerId}
                  providerName={selectedProvider?.displayName ?? ""}
                  modelReady={desktopRuntime
                    && Boolean(selectedProvider?.enabled)
                    && (cloudAllowed || selectedProvider?.isLocal === true)}
                  showTitle={false}
                />
              )}
            </div>
          </section>
        )}
      </main>
      <footer className="app-statusbar">
        <span className={cloudAllowed ? "status-security" : "status-security is-alert"}>
          {cloudAllowed ? <LockKeyhole size={12} /> : <ShieldAlert size={12} />}
          {cloudAllowed ? "云端模型可用" : "仅本地模型"}
        </span>
        <span className="status-spacer" />
        {!composing && <span>{countChineseCharacters(document)} 字</span>}
        <span>{selectedProvider?.displayName ?? "未选择模型"}</span>
        <span className="status-runtime" title={runtime}>{desktopRuntime ? "桌面内核" : "浏览器预览"}</span>
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
        initialTab={settingsTab}
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
        onClose={() => setGuideOpen(false)}
        onOpenSettings={() => {
          setGuideOpen(false);
          setSettingsTab("general");
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
      <QualityDrawer
        open={qualityOpen}
        findings={displayedFindings}
        canReview={canReview}
        reviewing={reviewing}
        onClose={() => setQualityOpen(false)}
        onLocate={(finding) => locateReviewNode(finding.nodeId)}
        onReview={() => {
          setQualityOpen(false);
          void runReview();
        }}
      />
      <DocumentInfoDialog open={infoOpen} onClose={() => setInfoOpen(false)} />
      <CommandPalette
        open={commandOpen}
        actions={commandActions}
        onClose={() => setCommandOpen(false)}
      />
      {availableUpdate && (
        <UpdateBanner
          update={availableUpdate}
          installing={installingUpdate}
          onInstall={() => void installAvailableUpdate()}
          onDismiss={() => setAvailableUpdate(null)}
        />
      )}
      {notice && <div className="toast" role="status">{notice}</div>}
    </div>
  );
}

export default App;
