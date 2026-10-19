import {
  ArrowRight,
  Check,
  Clock3,
  Database,
  FileText,
  FileUp,
  FolderOpen,
  LoaderCircle,
  Settings2,
  Sparkles,
  X,
} from "lucide-react";
import { useRef, useState } from "react";
import { documentTypeLabels } from "../../lib/document";
import { draftStageLabels, draftStages, type DraftStageId } from "../../lib/engine";
import type { TypeDecision } from "../../lib/generated/engine";
import type { ProviderProfile } from "../../lib/llm";
import { createId } from "../../lib/id";
import {
  materialByteLength,
  MAX_DRAFT_MATERIAL_BYTES,
  referenceContentBytes,
  safeReferenceLabel,
  type DraftReference,
} from "../../lib/materialReferences";
import type { DocumentType } from "../../lib/odoc";
import { extractUploadedFileText, uploadAccept } from "../../lib/referenceFiles";
import type { DocumentSummary } from "../../lib/store";
import { KnowledgePickerDialog } from "./KnowledgePickerDialog";

const QUICK_TYPES: Array<{ value: DocumentType; label: string }> = [
  { value: "notice", label: "通知" },
  { value: "request", label: "请示" },
  { value: "report", label: "报告" },
  { value: "summary", label: "总结" },
  { value: "letter", label: "函" },
  { value: "speech", label: "讲话稿" },
];

interface WriteHomeProps {
  material: string;
  onMaterialChange: (value: string) => void;
  providers: ProviderProfile[];
  providerId: string;
  onProviderChange: (value: string) => void;
  desktop: boolean;
  /** True from the click until the pipeline reports its first stage. */
  starting: boolean;
  drafting: boolean;
  stage: DraftStageId | null;
  completedStages: DraftStageId[];
  draftText: string;
  error: string | null;
  typeDecision: TypeDecision | null;
  confirmedDocType: DocumentType | null;
  onConfirmedDocTypeChange: (value: DocumentType) => void;
  onConfirmDocType: () => void;
  onStart: (docType: DocumentType | null) => void;
  onOpenSettings: () => void;
  onOpenDocuments: () => void;
  onOpenRecent: (id: string) => void;
  recentDocuments: DocumentSummary[];
  documentId: string;
  kbEnabled: boolean;
  currentTitle: string;
  onContinueDocument: () => void;
  references: DraftReference[];
  onAddReferences: (references: DraftReference[]) => void;
  onRemoveReference: (id: string) => void;
}

export function WriteHome({
  material,
  onMaterialChange,
  providers,
  providerId,
  onProviderChange,
  desktop,
  starting,
  drafting,
  stage,
  completedStages,
  draftText,
  error,
  typeDecision,
  confirmedDocType,
  onConfirmedDocTypeChange,
  onConfirmDocType,
  onStart,
  onOpenSettings,
  onOpenDocuments,
  onOpenRecent,
  recentDocuments,
  documentId,
  kbEnabled,
  currentTitle,
  onContinueDocument,
  references,
  onAddReferences,
  onRemoveReference,
}: WriteHomeProps) {
  const [docType, setDocType] = useState<DocumentType | null>(null);
  const [knowledgeOpen, setKnowledgeOpen] = useState(false);
  const [referenceBusy, setReferenceBusy] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [referenceMessage, setReferenceMessage] = useState<{ text: string; error: boolean } | null>(null);
  const uploadRef = useRef<HTMLInputElement>(null);
  const enabledProviders = providers.filter((provider) => provider.enabled);
  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const stageCount = draftStages.length;
  const progress = Math.min(100, Math.round((completedStages.length / stageCount) * 100));
  const busy = starting || drafting;
  const byteLength = materialByteLength(material);
  const referenceBytes = referenceContentBytes(references);
  const totalBytes = byteLength + referenceBytes;
  const overLimit = totalBytes > MAX_DRAFT_MATERIAL_BYTES;
  const canStart = desktop && Boolean(selectedProvider?.enabled) && Boolean(material.trim()) && !overLimit;
  const recent = recentDocuments.slice(0, 4);

  async function addFiles(files: File[]) {
    if (files.length === 0 || referenceBusy || busy) return;
    setReferenceBusy(true);
    setReferenceMessage(null);
    const incoming: DraftReference[] = [];
    const labels = new Set(references.map((reference) => reference.label));
    let failure: string | null = null;
    try {
      for (const file of files) {
        const label = safeReferenceLabel(file.name);
        if (labels.has(label)) {
          failure = `${label} 已经加入参考材料。`;
          continue;
        }
        try {
          const text = await extractUploadedFileText(file, desktop);
          if (!text.trim()) {
            failure = `${label} 没有可读取的文字。`;
            continue;
          }
          incoming.push({ id: createId("ref"), kind: "上传文件", label, content: text });
          labels.add(label);
        } catch (uploadError) {
          failure = uploadError instanceof Error ? uploadError.message : "参考文件读取失败";
        }
      }
      const nextBytes = byteLength + referenceBytes + referenceContentBytes(incoming);
      if (incoming.length > 0 && nextBytes > MAX_DRAFT_MATERIAL_BYTES) {
        setReferenceMessage({ text: "参考材料与写作要求合计超过 2 MiB，请减少文件或摘取关键段落。", error: true });
        return;
      }
      if (incoming.length > 0) {
        onAddReferences(incoming);
        setReferenceMessage({ text: `已加入 ${incoming.length} 份参考材料（正文不会粘贴文件内容）`, error: false });
      }
      if (failure) setReferenceMessage({ text: failure, error: true });
    } finally {
      setReferenceBusy(false);
    }
  }

  function removeReference(reference: DraftReference) {
    if (busy) return;
    onRemoveReference(reference.id);
    setReferenceMessage({ text: `已移除：${reference.label}`, error: false });
  }

  return (
    <div className="write-home">
      <div className="write-home-inner">
        <header className="write-intro">
          <span className="write-eyebrow">秉笔 · 写作台</span>
          <h1>今天想写点什么？</h1>
          <p>把要写的事说清楚就行。文种、标题和格式会自动处理，不用先填表。</p>
        </header>

        {currentTitle.trim() && !busy && (
          <button className="continue-document" type="button" onClick={onContinueDocument}>
            <FileText size={15} />
            继续编辑《{currentTitle.trim()}》
            <ArrowRight size={14} />
          </button>
        )}

        <section className={busy ? "composer-card is-busy" : "composer-card"} aria-label="写作输入区">
          {busy && (
            <div className="composer-running" role="status" aria-live="polite">
              <div className="composer-running-head">
                <LoaderCircle size={15} className="is-spinning" />
                <strong>{stage ? draftStageLabels[stage] : "正在准备写作…"}</strong>
                <span>{completedStages.length} / {stageCount}</span>
              </div>
              <div className={stage ? "composer-progress-bar" : "composer-progress-bar is-indeterminate"} aria-hidden="true">
                <span style={stage ? { width: `${Math.max(6, progress)}%` } : undefined} />
              </div>
            </div>
          )}
          <textarea
            className="composer-input"
            aria-label="写作要求"
            placeholder="例如：给各乡镇发一个防汛检查通知，下周开始检查，重点看值班和物资；具体时间地点我来定。"
            value={material}
            onChange={(event) => onMaterialChange(event.target.value)}
            onKeyDown={(event) => {
              if ((event.ctrlKey || event.metaKey) && event.key === "Enter" && canStart && !busy) {
                event.preventDefault();
                onStart(docType);
              }
            }}
            rows={7}
            disabled={busy}
          />

          <div className="composer-types" role="group" aria-label="文种">
            <span className="composer-types-label">文种</span>
            <button
              className={docType === null ? "type-chip is-active" : "type-chip"}
              type="button"
              aria-pressed={docType === null}
              onClick={() => setDocType(null)}
              disabled={busy}
            >
              自动识别
            </button>
            {QUICK_TYPES.map((type) => (
              <button
                className={docType === type.value ? "type-chip is-active" : "type-chip"}
                type="button"
                aria-pressed={docType === type.value}
                key={type.value}
                onClick={() => setDocType(docType === type.value ? null : type.value)}
                disabled={busy}
              >
                {type.label}
              </button>
            ))}
          </div>

          <input
            ref={uploadRef}
            className="visually-hidden"
            type="file"
            multiple
            accept={uploadAccept()}
            onChange={(event) => {
              const files = Array.from(event.target.files ?? []);
              event.target.value = "";
              void addFiles(files);
            }}
            disabled={busy || referenceBusy}
          />
          <button
            className={dragActive ? "reference-dropzone is-dragging" : "reference-dropzone"}
            type="button"
            onClick={() => uploadRef.current?.click()}
            onDragOver={(event) => {
              event.preventDefault();
              if (!busy && !referenceBusy) setDragActive(true);
            }}
            onDragLeave={() => setDragActive(false)}
            onDrop={(event) => {
              event.preventDefault();
              setDragActive(false);
              void addFiles(Array.from(event.dataTransfer.files));
            }}
            disabled={busy || referenceBusy}
            aria-label="添加参考文件"
          >
            <span className="reference-dropzone-icon">
              {referenceBusy ? <LoaderCircle size={22} className="is-spinning" /> : <FileUp size={22} />}
            </span>
            <span className="reference-dropzone-text">
              <strong>{referenceBusy ? "正在读取参考材料…" : "拖拽参考文件到这里"}</strong>
              <small>或点击选择；支持 TXT / MD / CSV / JSON / PDF / OFD / DOC / DOCX / XLSX / XLS / WPS</small>
            </span>
            <span className="reference-count">{references.length > 0 ? `${references.length} 份` : "可选"}</span>
          </button>
          {references.length > 0 && (
            <div className="composer-references" aria-label="已加入的参考材料">
              {references.map((reference) => (
                <span className="reference-chip" key={reference.id}>
                  <FileText size={12} />
                  <span title={reference.label}>{reference.label}</span>
                  <button
                    className="reference-remove"
                    type="button"
                    onClick={() => removeReference(reference)}
                    disabled={busy}
                    aria-label={`移除参考文件 ${reference.label}`}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
          )}

          <div className="composer-footer">
            <div className="composer-tools">
              {kbEnabled && (
                <button
                  className="composer-tool"
                  type="button"
                  onClick={() => setKnowledgeOpen(true)}
                  disabled={busy}
                  title="从知识库引用（可选）"
                >
                  <Database size={14} />
                  知识库
                </button>
              )}
              {enabledProviders.length > 0 ? (
                <label className="composer-model">
                  <span className="visually-hidden">模型服务</span>
                  <select
                    aria-label="模型服务"
                    value={selectedProvider?.enabled ? providerId : enabledProviders[0]?.id ?? ""}
                    onChange={(event) => onProviderChange(event.target.value)}
                    disabled={busy}
                  >
                    {enabledProviders.map((provider) => (
                      <option key={provider.id} value={provider.id}>
                        {provider.displayName}
                      </option>
                    ))}
                  </select>
                </label>
              ) : (
                <button className="composer-tool is-warning" type="button" onClick={onOpenSettings}>
                  <Settings2 size={14} />
                  配置模型
                </button>
              )}
            </div>
            <button
              className="start-writing-button"
              type="button"
              onClick={() => onStart(docType)}
              disabled={!canStart || busy}
              title={!desktop ? "浏览器预览不调用模型，请在桌面版生成" : !material.trim() ? "先写一下想生成什么" : !selectedProvider?.enabled ? "请先配置模型服务" : undefined}
            >
              {busy ? <LoaderCircle size={16} className="is-spinning" /> : <Sparkles size={15} />}
              {starting ? "正在准备" : drafting ? "正在写" : "开始写作"}
              {!busy && <ArrowRight size={15} />}
            </button>
          </div>
          <div className="composer-footnote">
            <span>
              正文 {byteLength.toLocaleString("zh-CN")} 字节
              {references.length > 0 && ` · 参考 ${references.length} 份`}
              {" "}· 合计 {totalBytes.toLocaleString("zh-CN")} / {MAX_DRAFT_MATERIAL_BYTES.toLocaleString("zh-CN")} 字节
            </span>
            <span>Ctrl / ⌘ + Enter 开始</span>
          </div>
          {overLimit && (
            <div className="composer-message is-error" role="alert">
              正文与参考材料合计超过 2 MiB，请减少文件或摘取关键段落。
            </div>
          )}
        </section>

        {referenceMessage && (
          <div className={referenceMessage.error ? "composer-message is-error" : "composer-message"} role="status">
            {referenceMessage.text}
          </div>
        )}
        {!desktop && (
          <div className="composer-message">
            浏览器预览不会调用模型；桌面版可以直接生成。
          </div>
        )}
        {desktop && enabledProviders.length === 0 && (
          <div className="composer-message is-error">
            还没有可用的模型服务。
            <button className="inline-link" type="button" onClick={onOpenSettings}>去设置</button>
          </div>
        )}
        {error && <div className="composer-message is-error" role="alert">{error}</div>}

        {typeDecision && (
          <div className="type-confirmation-card" role="status">
            <div>
              <strong>需要确认一下文种</strong>
              <span>模型判断为“{documentTypeLabels[typeDecision.value]}”，{Math.round(typeDecision.confidence * 100)}% 把握。</span>
            </div>
            <select
              aria-label="确认文种"
              value={confirmedDocType ?? typeDecision.value}
              onChange={(event) => onConfirmedDocTypeChange(event.target.value as DocumentType)}
              disabled={busy}
            >
              {Object.entries(documentTypeLabels).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
            <button className="command-button primary" type="button" onClick={onConfirmDocType} disabled={busy}>
              <Check size={14} /> 确认并继续
            </button>
          </div>
        )}

        {busy && draftText && (
          <div className="composer-progress" role="status" aria-live="polite">
            <div className="composer-progress-head">
              <LoaderCircle size={15} className="is-spinning" />
              <strong>正在写入正文</strong>
              <span>{completedStages.length} / {stageCount}</span>
            </div>
            <pre className="composer-stream">{draftText.slice(-2200)}</pre>
            <span className="composer-progress-note">完成后会自动进入正文。</span>
          </div>
        )}

        <KnowledgePickerDialog
          open={knowledgeOpen}
          documentId={documentId}
          onClose={() => setKnowledgeOpen(false)}
          onInsert={(label, content) => {
            if (references.some((reference) => reference.label === label)) {
              setReferenceMessage({ text: "这份知识库资料已经加入参考材料。", error: true });
              return;
            }
            onAddReferences([{ id: createId("ref"), kind: "知识库", label, content }]);
            setReferenceMessage({ text: `已加入知识库参考：${label}（正文不会粘贴内容）`, error: false });
          }}
        />

        {!busy && recent.length > 0 && (
          <section className="recent-section" aria-label="最近文档">
            <div className="recent-heading">
              <span>最近编辑</span>
              <button className="inline-link" type="button" onClick={onOpenDocuments}>
                全部文档 <FolderOpen size={13} />
              </button>
            </div>
            <div className="recent-grid">
              {recent.map((document) => (
                <button className="recent-card" type="button" onClick={() => onOpenRecent(document.id)} key={document.id}>
                  <span className="recent-type">{documentTypeLabels[document.docType]}</span>
                  <strong>{document.title || "未命名公文"}</strong>
                  <span className="recent-meta">
                    <Clock3 size={12} />
                    {new Date(document.updatedAt).toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" })}
                    <span>·</span>
                    {document.versionCount} 个版本
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
