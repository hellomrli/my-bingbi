import { BookOpen, Check, Database, FilePlus2, FileUp, LoaderCircle, Play, Search, ShieldAlert, Sparkles, WandSparkles, X } from "lucide-react";
import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { documentTypeLabels, securityLabels } from "../../lib/document";
import { extractDocxText, extractReferenceText, ipcErrorMessage, kbGet, kbSearch } from "../../lib/ipc";
import type { KbHit, KbNote } from "../../lib/kb";
import type { ProviderProfile } from "../../lib/llm";
import type { DocumentType, SecurityLevel } from "../../lib/odoc";
import type { TemplateSummary } from "../../lib/pack";
import type { TypeDecision } from "../../lib/generated/engine";
import type { DraftStageId } from "../../lib/engine";
import { useWorkbenchStore } from "../../store/workbench";

const MAX_DRAFT_MATERIAL_BYTES = 2 * 1024 * 1024;
const MAX_REFERENCE_FILE_BYTES = 64 * 1024 * 1024;
const TEXT_REFERENCE_EXTENSIONS = new Set(["txt", "md", "markdown", "csv", "json", "yaml", "yml"]);
const BINARY_REFERENCE_EXTENSIONS = new Set(["pdf", "ofd", "doc", "docx", "xlsx", "xls", "wps"]);
const REFERENCE_FORMAT_MESSAGE = "暂支持 TXT、Markdown、CSV、JSON、YAML、PDF、OFD、DOC、DOCX、XLSX、XLS 和 WPS。";
const REFERENCE_START_PREFIX = "【参考来源｜";
const REFERENCE_END_MARKER = "【参考来源结束】";

interface MaterialReference {
  kind: "知识库" | "上传文件";
  label: string;
  start: number;
  end: number;
}

function fileExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
}

function safeReferenceLabel(value: string): string {
  return value
    // eslint-disable-next-line no-control-regex -- the point is to strip control characters
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replaceAll("【", "［")
    .replaceAll("】", "］")
    .trim()
    .slice(0, 160) || "未命名参考文件";
}

function materialByteLength(value: string): number {
  return new Blob([value]).size;
}

async function readReferenceBytes(file: File): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === "function") {
    try {
      return await file.arrayBuffer();
    } catch {
      // Fall back to FileReader for WebViews with a partial Blob implementation.
    }
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) resolve(reader.result);
      else reject(new Error("参考文件读取失败"));
    };
    reader.onerror = () => reject(reader.error ?? new Error("参考文件读取失败"));
    reader.readAsArrayBuffer(file);
  });
}

function decodeTextReference(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder("utf-16le").decode(bytes);
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return new TextDecoder("utf-16be").decode(bytes);
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    // Windows-produced TXT/CSV files in Chinese offices are commonly GBK/GB18030.
    return new TextDecoder("gb18030", { fatal: true }).decode(bytes);
  }
}

async function readTextReference(file: File): Promise<string> {
  return decodeTextReference(await readReferenceBytes(file));
}

function parseMaterialReferences(value: string): MaterialReference[] {
  const pattern = /【参考来源｜(知识库|上传文件)：([^\r\n】]+)】\r?\n[\s\S]*?\r?\n【参考来源结束】/g;
  return Array.from(value.matchAll(pattern), (match) => ({
    kind: match[1] as MaterialReference["kind"],
    label: match[2],
    start: match.index,
    end: match.index + match[0].length,
  }));
}

function safeReferenceContent(value: string): string {
  return value
    .replaceAll(REFERENCE_START_PREFIX, "［参考来源｜")
    .replaceAll(REFERENCE_END_MARKER, "［参考来源结束］");
}

interface IntakeSidebarProps {
  material: string;
  onMaterialChange: (value: string) => void;
  modelText: string;
  onModelTextChange: (value: string) => void;
  onStartImitate: () => void;
  onStartPolish: () => void;
  providers: ProviderProfile[];
  providerId: string;
  onProviderChange: (value: string) => void;
  /** Scenario templates from the knowledge pack, for the current document type. */
  templates?: TemplateSummary[];
  templateId?: string;
  onTemplateChange?: (value: string) => void;
  onStartDraft: () => void;
  onOpenWizard: () => void;
  drafting: boolean;
  draftStage: DraftStageId | null;
  draftText: string;
  draftError: string | null;
  typeDecision: TypeDecision | null;
  confirmedDocType: DocumentType | null;
  onConfirmedDocTypeChange: (value: DocumentType) => void;
  onConfirmDocType: () => void;
  desktopRuntime: boolean;
  kbEnabled: boolean;
}

export function IntakeSidebar({
  material,
  onMaterialChange,
  modelText,
  onModelTextChange,
  onStartImitate,
  onStartPolish,
  providers,
  providerId,
  onProviderChange,
  templates = [],
  templateId = "",
  onTemplateChange,
  onStartDraft,
  onOpenWizard,
  drafting,
  draftStage,
  draftText,
  draftError,
  typeDecision,
  confirmedDocType,
  onConfirmedDocTypeChange,
  onConfirmDocType,
  desktopRuntime,
  kbEnabled,
}: IntakeSidebarProps) {
  const document = useWorkbenchStore((state) => state.document);
  const updateDocumentType = useWorkbenchStore((state) => state.updateDocumentType);
  const updateSecurityLevel = useWorkbenchStore((state) => state.updateSecurityLevel);
  const updateHeader = useWorkbenchStore((state) => state.updateHeader);
  const updateRecipient = useWorkbenchStore((state) => state.updateRecipient);
  const updateSigner = useWorkbenchStore((state) => state.updateSigner);
  const updateDate = useWorkbenchStore((state) => state.updateDate);
  const localOnly = document.meta.securityLevel === "secret" || document.meta.securityLevel === "confidential";
  const typeTemplates = templates.filter((template) => template.docType === document.meta.docType);
  const selectedTemplate = typeTemplates.find((template) => template.id === templateId) ?? null;
  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const providerBlocked = Boolean(selectedProvider && localOnly && !selectedProvider.isLocal);
  const [sourceQuery, setSourceQuery] = useState("");
  const [fieldsOpen, setFieldsOpen] = useState(true);
  const [sourceHits, setSourceHits] = useState<KbHit[]>([]);
  const [sourceBusy, setSourceBusy] = useState(false);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [selectedNote, setSelectedNote] = useState<KbNote | null>(null);
  const [hasSearched, setHasSearched] = useState(false);
  const [referenceStatus, setReferenceStatus] = useState<{ message: string; error: boolean } | null>(null);
  const [referenceBusy, setReferenceBusy] = useState(false);
  const materialRef = useRef(material);
  const materialReferences = useMemo(() => parseMaterialReferences(material), [material]);

  useEffect(() => {
    materialRef.current = material;
  }, [material]);

  function changeMaterial(value: string) {
    if (materialByteLength(value) > MAX_DRAFT_MATERIAL_BYTES) {
      setReferenceStatus({ message: "起草材料超过 2 MiB 限制，请删减文字或参考来源。", error: true });
      return;
    }
    materialRef.current = value;
    onMaterialChange(value);
  }

  async function searchSources(event: FormEvent) {
    event.preventDefault();
    if (!sourceQuery.trim() || sourceBusy || !kbEnabled) return;
    setSourceBusy(true);
    setSourceError(null);
    setSelectedNote(null);
    setHasSearched(true);
    try {
      setSourceHits(await kbSearch({
        vault: null,
        keyword: sourceQuery.trim(),
        searchContent: true,
        searchMode: "content",
        pathPrefix: null,
        filters: {},
        limit: 12,
      }, document.id));
    } catch (error) {
      setSourceHits([]);
      setSourceError(ipcErrorMessage(error));
    } finally {
      setSourceBusy(false);
    }
  }

  async function openSource(path: string) {
    setSourceBusy(true);
    setSourceError(null);
    try {
      setSelectedNote(await kbGet(path, document.id));
    } catch (error) {
      setSourceError(ipcErrorMessage(error));
    } finally {
      setSourceBusy(false);
    }
  }

  function appendReference(kind: "知识库" | "上传文件", label: string, content: string): boolean {
    const safeLabel = safeReferenceLabel(label);
    const cleaned = safeReferenceContent(content.trim());
    if (!cleaned) {
      setReferenceStatus({ message: `${safeLabel} 没有可读取的文字。`, error: true });
      return false;
    }
    if (cleaned.includes("\0")) {
      setReferenceStatus({ message: `${safeLabel} 含有不可处理的控制字符，请先转换为纯文本。`, error: true });
      return false;
    }
    const marker = `${REFERENCE_START_PREFIX}${kind}：${safeLabel}】`;
    const currentMaterial = materialRef.current;
    if (currentMaterial.includes(marker)) {
      setReferenceStatus({ message: `${safeLabel} 已经加入起草材料。`, error: true });
      return false;
    }
    const source = `${marker}\n${cleaned}\n${REFERENCE_END_MARKER}`;
    const next = currentMaterial.trim() ? `${currentMaterial.trim()}\n\n${source}` : source;
    if (materialByteLength(next) > MAX_DRAFT_MATERIAL_BYTES) {
      setReferenceStatus({ message: "加入后将超过起草材料的 2 MiB 限制，请先摘取所需段落。", error: true });
      return false;
    }
    changeMaterial(next);
    setReferenceStatus({ message: `已引用：${safeLabel}`, error: false });
    return true;
  }

  function removeReference(kind: MaterialReference["kind"], label: string) {
    if (drafting || referenceBusy) return;
    const currentMaterial = materialRef.current;
    const reference = parseMaterialReferences(currentMaterial).find(
      (candidate) => candidate.kind === kind && candidate.label === label,
    );
    if (!reference) {
      setReferenceStatus({ message: "该参考来源已不存在，请刷新后重试。", error: true });
      return;
    }
    const before = currentMaterial.slice(0, reference.start).trimEnd();
    const after = currentMaterial.slice(reference.end).trimStart();
    changeMaterial(before && after ? `${before}\n\n${after}` : before || after);
    setReferenceStatus({ message: `已移除：${label}`, error: false });
  }

  function addSourceToMaterial() {
    if (!selectedNote || drafting || referenceBusy) return;
    appendReference("知识库", `${selectedNote.title}｜${selectedNote.path}`, selectedNote.content);
  }

  async function addUploadedReferences(files: File[]) {
    if (files.length === 0 || drafting || referenceBusy) return;
    setReferenceBusy(true);
    try {
      for (const file of files) {
        const extension = fileExtension(file.name);
        const label = safeReferenceLabel(file.name);
        if (file.size === 0) {
          setReferenceStatus({ message: `${label} 是空文件。`, error: true });
          continue;
        }
        if (extension === "docx") {
          if (!desktopRuntime) {
            setReferenceStatus({ message: "浏览器预览不能解析 DOCX，请在桌面版上传。", error: true });
            continue;
          }
          if (file.size > MAX_REFERENCE_FILE_BYTES) {
            setReferenceStatus({ message: "DOCX 参考文件超过 64 MiB 限制。", error: true });
            continue;
          }
          setReferenceStatus({ message: `正在读取 ${label}…`, error: false });
          try {
            const text = await extractDocxText(new Uint8Array(await readReferenceBytes(file)));
            appendReference("上传文件", label, text);
          } catch (error) {
            setReferenceStatus({ message: ipcErrorMessage(error), error: true });
          }
          continue;
        }
        if (BINARY_REFERENCE_EXTENSIONS.has(extension)) {
          if (!desktopRuntime) {
            setReferenceStatus({ message: `${REFERENCE_FORMAT_MESSAGE} ${extension.toUpperCase()} 参考文件需要在桌面版读取。`, error: true });
            continue;
          }
          if (file.size > MAX_REFERENCE_FILE_BYTES) {
            setReferenceStatus({ message: `${extension.toUpperCase()} 参考文件超过 64 MiB 限制。`, error: true });
            continue;
          }
          setReferenceStatus({ message: `正在读取 ${label}…`, error: false });
          try {
            const text = await extractReferenceText(file.name, new Uint8Array(await readReferenceBytes(file)));
            appendReference("上传文件", label, text);
          } catch (error) {
            setReferenceStatus({ message: ipcErrorMessage(error), error: true });
          }
          continue;
        }
        if (!TEXT_REFERENCE_EXTENSIONS.has(extension) && !file.type.startsWith("text/")) {
          setReferenceStatus({ message: REFERENCE_FORMAT_MESSAGE, error: true });
          continue;
        }
        if (file.size > MAX_DRAFT_MATERIAL_BYTES) {
          setReferenceStatus({ message: "文本参考文件超过起草材料 2 MiB 限制。", error: true });
          continue;
        }
        setReferenceStatus({ message: `正在读取 ${label}…`, error: false });
        try {
          appendReference("上传文件", label, await readTextReference(file));
        } catch (error) {
          setReferenceStatus({ message: ipcErrorMessage(error), error: true });
        }
      }
    } finally {
      setReferenceBusy(false);
    }
  }
  const stageLabel = draftStage
    ? ({ doctype: "文种识别", facts: "事实提取", retrieve: "知识检索", style: "笔法分析", outline: "骨架生成", draft: "正文起草", deai: "降 AI 味", lint: "规则校验", correct: "文本校对", factcheck: "事实回查", score: "质量评分" } as const)[draftStage]
    : null;

  return (
    <aside className="left-rail" aria-label="公文要素与素材">
      <details
        className="panel-section intake-section"
        open={fieldsOpen}
        onToggle={(event) => setFieldsOpen(event.currentTarget.open)}
      >
        <summary className="panel-heading">
          <BookOpen size={14} />
          <span>公文要素</span>
        </summary>
        <div className="form-fields">
          <label className="field">
            <span>文种</span>
            <select value={document.meta.docType} onChange={(event) => updateDocumentType(event.target.value as DocumentType)}>
              {Object.entries(documentTypeLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>发文机关标志</span>
            <input
              value={document.header?.issuingAuthorityMark ?? ""}
              onChange={(event) => updateHeader("issuingAuthorityMark", event.target.value)}
              placeholder="发文机关名称 + 文件"
            />
          </label>
          <label className="field">
            <span>发文字号</span>
            <input
              data-node-id={document.header?.id}
              value={document.header?.documentNumber ?? ""}
              onChange={(event) => updateHeader("documentNumber", event.target.value)}
              placeholder="机关代字〔年份〕序号"
            />
          </label>
          <label className="field">
            <span>签发人</span>
            <input
              value={document.header?.signer ?? ""}
              onChange={(event) => updateHeader("signer", event.target.value)}
              placeholder="姓名"
            />
          </label>
          <label className="field">
            <span>主送机关</span>
            <input
              value={document.body.mainRecipients[0] ?? ""}
              onChange={(event) => updateRecipient(event.target.value)}
              placeholder="主送机关"
            />
          </label>
          <label className="field">
            <span>发文机关署名</span>
            <input
              value={document.body.signer?.authority ?? ""}
              onChange={(event) => updateSigner(event.target.value)}
              placeholder="落款机关"
            />
          </label>
          <label className="field">
            <span>成文日期</span>
            <input data-node-id={document.body.date?.id} value={document.body.date?.value ?? ""} onChange={(event) => updateDate(event.target.value)} placeholder="2026年8月12日" />
          </label>
          <label className="field">
            <span>密级</span>
            <select
              value={document.meta.securityLevel}
              onChange={(event) => updateSecurityLevel(event.target.value as SecurityLevel)}
            >
              {Object.entries(securityLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </details>

      <section className="panel-section draft-section">
        <div className="panel-heading">
          <Play size={14} />
          <span>智能起草</span>
          <span className="panel-count">{desktopRuntime ? "桌面内核" : "浏览器预览"}</span>
        </div>
        <div className="draft-form">
          {typeTemplates.length > 0 && (
            <label className="field">
              <span>写作模板</span>
              <select
                value={templateId}
                onChange={(event) => onTemplateChange?.(event.target.value)}
                disabled={drafting}
              >
                <option value="">标准模板</option>
                {typeTemplates.map((template) => (
                  <option value={template.id} key={template.id}>
                    {template.name}
                  </option>
                ))}
              </select>
              {selectedTemplate?.scenario && <small>{selectedTemplate.scenario}</small>}
            </label>
          )}
          <label className="field material-field">
            <span>写作意图与基础材料 <em>不必先写正文</em></span>
            <textarea
              value={material}
              onChange={(event) => changeMaterial(event.target.value)}
              placeholder="写下想解决什么问题、准备做什么、对象和时限即可；也可以只上传/引用文件，让模型先提取事实再起草。"
              rows={6}
              disabled={drafting}
            />
            <small>{material.length.toLocaleString("zh-CN")} 字符 · {materialByteLength(material).toLocaleString("zh-CN")} 字节 / 2 MiB</small>
          </label>
          <div className="material-reference-actions">
            <span>可一次选择多个参考文件；它们会进入完整起草流程：事实提取、检索、起草、校验和事实回查。</span>
            <label className={"command-button reference-upload-button" + (drafting || referenceBusy ? " is-disabled" : "")}>
              <FileUp size={14} />
              上传参考文件
              <input
                className="visually-hidden"
                type="file"
                multiple
                accept=".txt,.md,.markdown,.csv,.json,.yaml,.yml,.pdf,.ofd,.doc,.docx,.xlsx,.xls,.wps,text/plain,text/markdown,application/json,application/yaml,application/pdf,application/ofd,application/msword,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.ms-excel,application/vnd.ms-works"
                disabled={drafting || referenceBusy}
                onChange={(event) => {
                  const files = Array.from(event.target.files ?? []);
                  event.target.value = "";
                  void addUploadedReferences(files);
                }}
              />
            </label>
          </div>
          {materialReferences.length > 0 && (
            <div className="material-reference-list" aria-label="已引用的参考来源">
              <div className="material-reference-list-heading">
                <span>已引用来源</span>
                <small>{materialReferences.length} 份</small>
              </div>
              <ul>
                {materialReferences.map((reference) => (
                  <li key={`${reference.kind}:${reference.label}`}>
                    <span className="reference-kind">{reference.kind === "知识库" ? "库" : "文件"}</span>
                    <span className="reference-label" title={reference.label}>{reference.label}</span>
                    <button
                      className="icon-button tiny"
                      type="button"
                      onClick={() => removeReference(reference.kind, reference.label)}
                      disabled={drafting || referenceBusy}
                      title={`移除 ${reference.label}`}
                      aria-label={`移除参考来源 ${reference.label}`}
                    >
                      <X size={13} />
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {referenceStatus && (
            <div className={"reference-status" + (referenceStatus.error ? " is-error" : "")} role={referenceStatus.error ? "alert" : "status"}>
              {referenceStatus.message}
            </div>
          )}
          <label className="field model-text-field">
            <span>范文（可选，仿写模式）</span>
            <textarea
              value={modelText}
              onChange={(event) => onModelTextChange(event.target.value)}
              placeholder="粘贴一篇本单位同类公文。起草前模型会先提炼它的笔法：结构、句式、用词与惯用表述。"
              rows={4}
              disabled={drafting}
            />
            <small>{modelText.trim() ? "提供范文时将按仿写流程起草" : "留空则使用普通起草流程"}</small>
          </label>
          <label className="field">
            <span>模型服务</span>
            <select value={providerId} onChange={(event) => onProviderChange(event.target.value)} disabled={drafting || providers.length === 0}>
              {providers.length === 0 && <option value="">暂无可用服务</option>}
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id} disabled={localOnly && !provider.isLocal}>
                  {provider.displayName}{provider.isLocal ? " · 本地" : " · 云端"}{!provider.enabled ? " · 已停用" : ""}
                </option>
              ))}
            </select>
          </label>
          {localOnly && (
            <div className="draft-security-note">
              <ShieldAlert size={13} /> 秘密及以上文档仅允许本地模型
            </div>
          )}
          {!desktopRuntime && <div className="draft-browser-note">浏览器预览不会发起模型调用，请在桌面版中起草。</div>}
          {typeDecision && (
            <div className="type-confirmation" role="status">
              <div className="type-confirmation-heading">
                <span>需要确认文种</span>
                <b>{Math.round(typeDecision.confidence * 100)}% 置信度</b>
              </div>
              <p>模型判断为“{documentTypeLabels[typeDecision.value]}”，请确认后继续。</p>
              <select
                aria-label="确认文种"
                value={confirmedDocType ?? typeDecision.value}
                onChange={(event) => onConfirmedDocTypeChange(event.target.value as DocumentType)}
                disabled={drafting}
              >
                {Object.entries(documentTypeLabels).map(([value, label]) => (
                  <option key={value} value={value}>{label}</option>
                ))}
              </select>
              <button className="command-button primary" type="button" onClick={onConfirmDocType} disabled={drafting}>
                <Check size={14} /> 确认并继续
              </button>
            </div>
          )}
          {draftError && <div className="draft-error" role="alert">{draftError}</div>}
          {drafting && (
            <div className="draft-progress" role="status" aria-live="polite">
              <div className="draft-progress-heading">
                <LoaderCircle size={14} className="is-spinning" />
                <span>{stageLabel ?? "准备起草"}</span>
                <span className="panel-count">进行中</span>
              </div>
              {draftText && <pre className="draft-token-output">{draftText}</pre>}
            </div>
          )}
          <button
            className="command-button primary draft-start-button"
            type="button"
            onClick={onStartDraft}
            disabled={drafting || referenceBusy || !material.trim() || !providerId || providerBlocked || !selectedProvider?.enabled}
            title={referenceBusy ? "正在读取参考文件，请稍候" : !material.trim() ? "请填写写作意图，或上传/引用至少一份参考文件" : providerBlocked ? "当前密级禁止使用云端模型" : undefined}
          >
            {drafting ? <LoaderCircle size={15} className="is-spinning" /> : <Play size={15} />}
            {drafting ? "正在起草" : typeDecision ? "重新起草" : "开始起草"}
          </button>
          <button
            className="command-button wizard-inline-button"
            type="button"
            onClick={onOpenWizard}
            disabled={drafting}
            title="一步一步填写文种、事由、事实与时限，再交给模型起草"
          >
            <WandSparkles size={15} />
            不知道怎么写？用向导模式
          </button>
          <button
            className="command-button imitate-start-button"
            type="button"
            onClick={onStartImitate}
            disabled={drafting || referenceBusy || !material.trim() || !modelText.trim() || !providerId || providerBlocked || !selectedProvider?.enabled}
            title={referenceBusy ? "正在读取参考文件，请稍候" : !material.trim() ? "请填写写作意图，或上传/引用至少一份参考文件" : !modelText.trim() ? "请先粘贴范文" : providerBlocked ? "当前密级禁止使用云端模型" : undefined}
          >
            {drafting ? <LoaderCircle size={15} className="is-spinning" /> : <FilePlus2 size={15} />}
            仿写起草
          </button>
          <button
            className="command-button polish-start-button"
            type="button"
            onClick={onStartPolish}
            disabled={drafting || referenceBusy || !material.trim() || !providerId || providerBlocked || !selectedProvider?.enabled}
            title={referenceBusy ? "正在读取参考文件，请稍候" : providerBlocked ? "当前密级禁止使用云端模型" : "润色已有正文；只提供意图或参考文件时请使用开始起草"}
          >
            {drafting ? <LoaderCircle size={15} className="is-spinning" /> : <Sparkles size={15} />}
            润色改写
          </button>
        </div>
      </section>

      {kbEnabled && (
        <section className="panel-section source-section">
          <div className="panel-heading">
            <Database size={14} />
            <span>知识库引用</span>
            <span className="panel-count">{sourceHits.length} 项</span>
          </div>
          <form className="source-search" onSubmit={searchSources}>
            <Search size={14} />
            <input
              aria-label="搜索素材"
              placeholder="搜索可引用的历史公文/政策文件"
              value={sourceQuery}
              onChange={(event) => setSourceQuery(event.target.value)}
              disabled={sourceBusy}
            />
            <button
              className="icon-button tiny"
              type="submit"
              disabled={!sourceQuery.trim() || sourceBusy}
              title="搜索素材"
              aria-label="搜索素材"
            >
              {sourceBusy ? <LoaderCircle size={13} className="is-spinning" /> : <Search size={13} />}
            </button>
          </form>
          {sourceError && <div className="source-message is-error" role="alert">{sourceError}</div>}
          {selectedNote && (
            <article className="source-note">
              <div>
                <strong>{selectedNote.title}</strong>
                <span>{selectedNote.path}</span>
              </div>
              <p>{selectedNote.content}</p>
              <button className="command-button" type="button" onClick={addSourceToMaterial} disabled={drafting || referenceBusy}>
                <FilePlus2 size={14} />
                引用到起草材料
              </button>
            </article>
          )}
          <div className="source-list">
            {sourceHits.map((hit) => (
              <button className="source-hit" type="button" onClick={() => void openSource(hit.path)} key={`${hit.vault}:${hit.path}`} title={hit.path}>
                <strong>{hit.title}</strong>
                <span>{hit.path}</span>
                {hit.snippet && <span className="source-snippet">{hit.snippet}</span>}
              </button>
            ))}
            {hasSearched && !sourceBusy && sourceHits.length === 0 && !sourceError && (
              <span className="source-message">没有匹配素材</span>
            )}
          </div>
        </section>
      )}
    </aside>
  );
}
