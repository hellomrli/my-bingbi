import { BookOpen, Check, Database, FilePlus2, LoaderCircle, Play, Search, ShieldAlert } from "lucide-react";
import { FormEvent, useState } from "react";
import { documentTypeLabels, securityLabels } from "../../lib/document";
import { ipcErrorMessage, kbGet, kbSearch } from "../../lib/ipc";
import type { KbHit, KbNote } from "../../lib/kb";
import type { ProviderProfile } from "../../lib/llm";
import type { DocumentType, SecurityLevel } from "../../lib/odoc";
import type { TypeDecision } from "../../lib/generated/engine";
import type { DraftStageId } from "../../lib/engine";
import { useWorkbenchStore } from "../../store/workbench";

interface IntakeSidebarProps {
  material: string;
  onMaterialChange: (value: string) => void;
  modelText: string;
  onModelTextChange: (value: string) => void;
  onStartImitate: () => void;
  providers: ProviderProfile[];
  providerId: string;
  onProviderChange: (value: string) => void;
  onStartDraft: () => void;
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
  providers,
  providerId,
  onProviderChange,
  onStartDraft,
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
  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const providerBlocked = Boolean(selectedProvider && localOnly && !selectedProvider.isLocal);
  const [sourceQuery, setSourceQuery] = useState("");
  const [sourceHits, setSourceHits] = useState<KbHit[]>([]);
  const [sourceBusy, setSourceBusy] = useState(false);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [selectedNote, setSelectedNote] = useState<KbNote | null>(null);
  const [hasSearched, setHasSearched] = useState(false);

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

  function addSourceToMaterial() {
    if (!selectedNote) return;
    const source = `【知识库素材：${selectedNote.title}｜${selectedNote.path}】\n${selectedNote.content.trim()}`;
    if (new Blob([material, source]).size > 2 * 1024 * 1024) {
      setSourceError("加入后将超过起草材料的 2 MiB 限制，请先摘取所需段落。");
      return;
    }
    onMaterialChange(material.trim() ? `${material.trim()}\n\n${source}` : source);
    setSourceError(null);
  }
  const stageLabel = draftStage
    ? ({ doctype: "文种识别", facts: "事实提取", retrieve: "知识检索", style: "笔法分析", outline: "骨架生成", draft: "正文起草", deai: "降 AI 味", lint: "规则校验", factcheck: "事实回查", score: "质量评分" } as const)[draftStage]
    : null;

  return (
    <aside className="left-rail" aria-label="公文要素与素材">
      <section className="panel-section intake-section">
        <div className="panel-heading">
          <BookOpen size={14} />
          <span>公文要素</span>
        </div>
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
            <input value={document.body.date?.value ?? ""} onChange={(event) => updateDate(event.target.value)} placeholder="2026年8月12日" />
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
      </section>

      <section className="panel-section draft-section">
        <div className="panel-heading">
          <Play size={14} />
          <span>智能起草</span>
          <span className="panel-count">{desktopRuntime ? "桌面内核" : "浏览器预览"}</span>
        </div>
        <div className="draft-form">
          <label className="field material-field">
            <span>起草材料</span>
            <textarea
              value={material}
              onChange={(event) => onMaterialChange(event.target.value)}
              placeholder="粘贴背景、事项、数据、时限和依据。模型只使用这里提供的事实。"
              rows={6}
              maxLength={2 * 1024 * 1024}
              disabled={drafting}
            />
            <small>{material.length.toLocaleString("zh-CN")} 字符</small>
          </label>
          <label className="field model-text-field">
            <span>范文（仿写模式）</span>
            <textarea
              value={modelText}
              onChange={(event) => onModelTextChange(event.target.value)}
              placeholder="粘贴一篇本单位同类公文。起草前模型会先提炼它的笔法：结构、句式、用词与惯用表述。"
              rows={4}
              maxLength={2 * 1024 * 1024}
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
            disabled={drafting || !material.trim() || !providerId || providerBlocked || !selectedProvider?.enabled}
            title={providerBlocked ? "当前密级禁止使用云端模型" : undefined}
          >
            {drafting ? <LoaderCircle size={15} className="is-spinning" /> : <Play size={15} />}
            {drafting ? "正在起草" : typeDecision ? "重新起草" : "开始起草"}
          </button>
          <button
            className="command-button imitate-start-button"
            type="button"
            onClick={onStartImitate}
            disabled={drafting || !material.trim() || !modelText.trim() || !providerId || providerBlocked || !selectedProvider?.enabled}
            title={!modelText.trim() ? "请先粘贴范文" : providerBlocked ? "当前密级禁止使用云端模型" : undefined}
          >
            {drafting ? <LoaderCircle size={15} className="is-spinning" /> : <FilePlus2 size={15} />}
            仿写起草
          </button>
        </div>
      </section>

      <section className="panel-section source-section">
        <div className="panel-heading">
          <Database size={14} />
          <span>库中素材</span>
          <span className="panel-count">{kbEnabled ? `${sourceHits.length} 项` : "未启用"}</span>
        </div>
        <form className="source-search" onSubmit={searchSources}>
          <Search size={14} />
          <input
            aria-label="搜索素材"
            placeholder={kbEnabled ? "搜索历史公文" : "请先在设置中启用知识库"}
            value={sourceQuery}
            onChange={(event) => setSourceQuery(event.target.value)}
            disabled={!kbEnabled || sourceBusy}
          />
          <button
            className="icon-button tiny"
            type="submit"
            disabled={!kbEnabled || !sourceQuery.trim() || sourceBusy}
            title="搜索素材"
            aria-label="搜索素材"
          >
            {sourceBusy ? <LoaderCircle size={13} className="is-spinning" /> : <Search size={13} />}
          </button>
        </form>
        {sourceError && <div className="source-message is-error" role="alert">{sourceError}</div>}
        {kbEnabled && selectedNote && (
          <article className="source-note">
            <div>
              <strong>{selectedNote.title}</strong>
              <span>{selectedNote.path}</span>
            </div>
            <p>{selectedNote.content}</p>
            <button className="command-button" type="button" onClick={addSourceToMaterial}>
              <FilePlus2 size={14} />
              加入起草材料
            </button>
          </article>
        )}
        <div className="source-list">
          {kbEnabled && sourceHits.map((hit) => (
            <button className="source-hit" type="button" onClick={() => void openSource(hit.path)} key={`${hit.vault}:${hit.path}`} title={hit.path}>
              <strong>{hit.title}</strong>
              <span>{hit.path}</span>
              {hit.snippet && <span className="source-snippet">{hit.snippet}</span>}
            </button>
          ))}
          {kbEnabled && hasSearched && !sourceBusy && sourceHits.length === 0 && !sourceError && (
            <span className="source-message">没有匹配素材</span>
          )}
        </div>
      </section>
    </aside>
  );
}
