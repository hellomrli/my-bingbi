import {
  Check,
  Database,
  Eye,
  EyeOff,
  FolderPlus,
  KeyRound,
  LoaderCircle,
  NotebookPen,
  Radio,
  RefreshCw,
  SearchCheck,
  Trash2,
  Unplug,
  Upload,
} from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import type { KbConfig, KbProviderKind } from "../../lib/kb";
import {
  defaultKbConfig,
  deleteKbFnsToken,
  deleteKbFnsWsToken,
  deleteKbMcpToken,
  hasKbFnsToken,
  hasKbFnsWsToken,
  hasKbMcpToken,
  ipcErrorMessage,
  kbClearIndex,
  kbCreateLocalVault,
  kbCreateNote,
  kbIndexState,
  kbRebuildIndex,
  kbSearch,
  kbSyncOnce,
  loadKbConfig,
  saveKbConfig,
  setKbFnsToken,
  setKbFnsWsToken,
  setKbMcpToken,
  type KbIndexProgress,
} from "../../lib/ipc";
import type { KbIndexState } from "../../lib/store";
import {
  MAX_KB_NOTE_BYTES,
  extractUploadedFileText,
  textByteLength,
  titleFromFileName,
  uploadAccept,
} from "../../lib/referenceFiles";

interface KnowledgeBaseSettingsProps {
  desktop: boolean;
}

type CredentialKind = "fns" | "fnsWs" | "mcp";
type Operation = "save" | "test" | "index" | "sync" | "clear";

interface CredentialState {
  value: string;
  visible: boolean;
  exists: boolean;
  busy: boolean;
}

const providerOptions: Array<{ value: KbProviderKind; label: string }> = [
  { value: "fns", label: "FNS" },
  { value: "local_vault", label: "本地 Vault" },
  { value: "mcp", label: "MCP" },
];

const credentialActions = {
  fns: { save: setKbFnsToken, remove: deleteKbFnsToken },
  fnsWs: { save: setKbFnsWsToken, remove: deleteKbFnsWsToken },
  mcp: { save: setKbMcpToken, remove: deleteKbMcpToken },
} satisfies Record<CredentialKind, {
  save: (token: string) => Promise<void>;
  remove: () => Promise<boolean>;
}>;

const emptyCredential = (): CredentialState => ({
  value: "",
  visible: false,
  exists: false,
  busy: false,
});

function formatIndexTime(value: number): string {
  if (!value) return "尚未建立";
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function stateLabel(state: KbIndexState | null): string {
  if (!state) return "未建立";
  if (state.status === "ready") return "可用";
  if (state.status === "running") return "重建中";
  if (state.status === "error") return "异常";
  return state.status;
}

export function KnowledgeBaseSettings({ desktop }: KnowledgeBaseSettingsProps) {
  const [config, setConfig] = useState<KbConfig>(defaultKbConfig);
  const [loading, setLoading] = useState(true);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [indexState, setIndexState] = useState<KbIndexState | null>(null);
  const [indexProgress, setIndexProgress] = useState<KbIndexProgress | null>(null);
  const [creatingVault, setCreatingVault] = useState(false);
  const [noteTitle, setNoteTitle] = useState("");
  const [noteContent, setNoteContent] = useState("");
  const [noteBusy, setNoteBusy] = useState(false);
  const [indexAfterNote, setIndexAfterNote] = useState(true);
  const [uploadBusy, setUploadBusy] = useState(false);
  const [uploadProgress, setUploadProgress] = useState<{ done: number; total: number; name: string } | null>(null);
  const [uploadErrors, setUploadErrors] = useState<string[]>([]);
  const [credentials, setCredentials] = useState<Record<CredentialKind, CredentialState>>({
    fns: emptyCredential(),
    fnsWs: emptyCredential(),
    mcp: emptyCredential(),
  });
  const busy = loading || operation !== null;

  useEffect(() => {
    let active = true;
    setLoading(true);
    Promise.all([
      loadKbConfig(),
      desktop ? hasKbFnsToken() : Promise.resolve(false),
      desktop ? hasKbFnsWsToken() : Promise.resolve(false),
      desktop ? hasKbMcpToken() : Promise.resolve(false),
      desktop ? kbIndexState() : Promise.resolve(null),
    ])
      .then(([loaded, hasFns, hasFnsWs, hasMcp, loadedIndexState]) => {
        if (!active) return;
        setConfig(loaded);
        setIndexState(loadedIndexState);
        setCredentials((current) => ({
          fns: { ...current.fns, exists: hasFns },
          fnsWs: { ...current.fnsWs, exists: hasFnsWs },
          mcp: { ...current.mcp, exists: hasMcp },
        }));
      })
      .catch((error) => active && setMessage(ipcErrorMessage(error)))
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [desktop]);

  async function persistConfig(): Promise<KbConfig> {
    const saved = await saveKbConfig(config);
    setConfig(saved);
    return saved;
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setOperation("save");
    setMessage(null);
    try {
      const saved = await persistConfig();
      if (desktop) setIndexState(await kbIndexState(saved.vault));
      setMessage("知识库配置已保存");
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setOperation(null);
    }
  }

  async function testConnection() {
    setOperation("test");
    setMessage(null);
    try {
      const saved = await persistConfig();
      if (!saved.enabled) {
        setMessage("请先启用知识库");
        return;
      }
      const hits = await kbSearch({
        vault: null,
        keyword: "",
        searchContent: true,
        searchMode: "content",
        pathPrefix: null,
        filters: {},
        limit: 1,
      });
      setMessage(`连接成功${hits.length ? "，已读取素材" : "，当前库中没有素材"}`);
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setOperation(null);
    }
  }

  async function saveCredential(kind: CredentialKind) {
    const value = credentials[kind].value.trim();
    if (!value) return;
    updateCredential(kind, { busy: true });
    setMessage(null);
    try {
      await credentialActions[kind].save(value);
      updateCredential(kind, { value: "", exists: true });
      setMessage("凭据已保存到系统凭据库");
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      updateCredential(kind, { busy: false });
    }
  }

  async function removeCredential(kind: CredentialKind) {
    updateCredential(kind, { busy: true });
    setMessage(null);
    try {
      await credentialActions[kind].remove();
      updateCredential(kind, { value: "", exists: false });
      setMessage("凭据已删除");
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      updateCredential(kind, { busy: false });
    }
  }

  async function rebuildIndex() {
    setOperation("index");
    setMessage(null);
    setIndexProgress({ status: "running", processed: 0, total: 0, path: null, message: null });
    try {
      const saved = await persistConfig();
      if (!saved.enabled || !saved.indexEnabled) {
        setMessage("请先启用知识库和本地索引");
        return;
      }
      const state = await kbRebuildIndex(undefined, setIndexProgress);
      setIndexState(state);
      setMessage(`索引已重建，共收录 ${state.noteCount} 篇素材`);
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setOperation(null);
    }
  }

  async function syncOnce() {
    setOperation("sync");
    setMessage(null);
    try {
      const saved = await persistConfig();
      if (saved.providerKind !== "fns" || !saved.syncEnabled) {
        setMessage("请先启用 FNS 增量同步");
        return;
      }
      const state = await kbSyncOnce();
      setIndexState(state);
      setMessage(`同步完成，当前游标 ${state.lastCursor}`);
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setOperation(null);
    }
  }

  async function clearIndex() {
    if (!window.confirm("确定清理当前知识库的本地索引吗？知识库原始笔记不会被删除。")) return;
    setOperation("clear");
    setMessage(null);
    try {
      await kbClearIndex(config.vault || undefined);
      setIndexState(null);
      setIndexProgress(null);
      setMessage("本地索引已清理");
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setOperation(null);
    }
  }

  async function createLocalVault() {
    if (creatingVault) return;
    setCreatingVault(true);
    setMessage(null);
    try {
      const path = await kbCreateLocalVault(config.localPath.trim() || undefined);
      const saved = await saveKbConfig({
        ...config,
        enabled: true,
        providerKind: "local_vault",
        localPath: path,
        vault: config.vault.trim() || "本地资料库",
      });
      setConfig(saved);
      setIndexState(await kbIndexState(saved.vault));
      setMessage("本地知识库已就绪；可在下方新建资料，或把 Markdown 放进该目录后重建索引");
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setCreatingVault(false);
    }
  }

  async function saveNote() {
    if (!noteTitle.trim() || !noteContent.trim() || noteBusy) return;
    setNoteBusy(true);
    setMessage(null);
    try {
      const created = await kbCreateNote({ title: noteTitle, content: noteContent });
      setNoteTitle("");
      setNoteContent("");
      if (indexAfterNote && config.indexEnabled) {
        setIndexProgress({ status: "running", processed: 0, total: 0, path: null, message: null });
        const state = await kbRebuildIndex(undefined, setIndexProgress);
        setIndexState(state);
        setMessage(`已保存《${created.title}》，索引收录 ${state.noteCount} 篇素材`);
      } else {
        setMessage(`已保存《${created.title}》到 ${created.path}`);
      }
    } catch (error) {
      setMessage(ipcErrorMessage(error));
    } finally {
      setNoteBusy(false);
      setIndexProgress(null);
    }
  }

  async function uploadMaterials(files: File[]) {
    if (files.length === 0 || uploadBusy || !config.enabled) return;
    setUploadBusy(true);
    setUploadErrors([]);
    setMessage(null);
    let created = 0;
    const errors: string[] = [];
    try {
      for (const [index, file] of files.entries()) {
        setUploadProgress({ done: index, total: files.length, name: file.name });
        try {
          const content = await extractUploadedFileText(file, desktop);
          if (!content) {
            throw new Error(`${file.name} 中没有可读取的文字`);
          }
          if (textByteLength(content) > MAX_KB_NOTE_BYTES) {
            throw new Error(`${file.name} 解析后超过 4 MiB，请拆分后再上传`);
          }
          await kbCreateNote({ title: titleFromFileName(file.name), content });
          created += 1;
        } catch (error) {
          errors.push(`${file.name}：${ipcErrorMessage(error)}`);
        }
      }
      setUploadProgress(null);
      if (created > 0 && indexAfterNote && config.indexEnabled) {
        setIndexProgress({ status: "running", processed: 0, total: 0, path: null, message: null });
        const state = await kbRebuildIndex(undefined, setIndexProgress);
        setIndexState(state);
        setMessage(`已导入 ${created} 份材料，索引收录 ${state.noteCount} 篇素材`);
      } else if (created > 0) {
        setMessage(`已导入 ${created} 份材料；记得重建索引后才能在引用面板检索`);
      } else {
        setMessage("没有导入任何材料");
      }
    } catch (error) {
      errors.push(`索引重建失败：${ipcErrorMessage(error)}`);
      setMessage("材料已保存，但索引重建失败");
    } finally {
      setUploadProgress(null);
      setUploadErrors(errors);
      setUploadBusy(false);
      setIndexProgress(null);
    }
  }

  function updateCredential(kind: CredentialKind, patch: Partial<CredentialState>) {
    setCredentials((current) => ({
      ...current,
      [kind]: { ...current[kind], ...patch },
    }));
  }

  function credentialEditor(kind: CredentialKind, label: string, placeholder: string): ReactNode {
    const credential = credentials[kind];
    return (
      <div className="credential-panel" key={kind}>
        <div className="settings-group">
          <label htmlFor={`kb-${kind}-token`}>{label}</label>
          <div className="secret-input">
            <KeyRound size={15} />
            <input
              id={`kb-${kind}-token`}
              type={credential.visible ? "text" : "password"}
              value={credential.value}
              onChange={(event) => updateCredential(kind, { value: event.target.value })}
              disabled={!desktop || credential.busy}
              autoComplete="new-password"
              placeholder={credential.exists ? "已保存，输入新值可替换" : placeholder}
            />
            <button
              className="icon-button tiny"
              type="button"
              onClick={() => updateCredential(kind, { visible: !credential.visible })}
              disabled={!desktop || credential.busy}
              title={credential.visible ? "隐藏凭据" : "显示凭据"}
              aria-label={credential.visible ? "隐藏凭据" : "显示凭据"}
            >
              {credential.visible ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
        </div>
        <div className="credential-actions">
          <span className={`chip ${credential.exists ? "chip-ok" : "chip-neutral"}`}>
            {credential.busy ? "处理中" : credential.exists ? "已保存" : "无凭据"}
          </span>
          <button
            className="icon-button tiny"
            type="button"
            onClick={() => void removeCredential(kind)}
            disabled={!desktop || !credential.exists || credential.busy}
            title="删除凭据"
            aria-label={`删除${label}`}
          >
            <Trash2 size={14} />
          </button>
          <button
            className="command-button"
            type="button"
            onClick={() => void saveCredential(kind)}
            disabled={!desktop || !credential.value.trim() || credential.busy}
          >
            <KeyRound size={14} />
            保存
          </button>
        </div>
      </div>
    );
  }

  const progressValue = indexProgress?.total
    ? Math.min(100, Math.round((indexProgress.processed / indexProgress.total) * 100))
    : 0;

  return (
    <form className="settings-form knowledge-form" onSubmit={submit}>
      <section className="kb-intro">
        <Database size={16} />
        <div>
          <b>可选组件</b>
          <span>未启用时左侧不显示引用检索，也不影响起草。启用后原始笔记仍留在你的 Vault / FNS，本项目只在本机建立索引。</span>
        </div>
      </section>

      <label className="toggle-row">
        <span>
          <b>启用知识库</b>
          <small>历史公文与政策素材</small>
        </span>
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={(event) => setConfig({ ...config, enabled: event.target.checked })}
          disabled={busy}
        />
      </label>

      <div className="settings-group">
        <label>接入方式</label>
        <div className="segmented-control kb-provider-control">
          {providerOptions.map((option) => (
            <button
              className={config.providerKind === option.value ? "is-active" : ""}
              type="button"
              onClick={() => setConfig({ ...config, providerKind: option.value })}
              disabled={busy}
              key={option.value}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="provider-field-grid">
        <div className="settings-group">
          <label htmlFor="kb-vault">Vault 名称</label>
          <input
            id="kb-vault"
            value={config.vault}
            onChange={(event) => setConfig({ ...config, vault: event.target.value })}
            placeholder="政务资料库"
            maxLength={256}
            required={config.enabled}
          />
        </div>
        {config.providerKind === "local_vault" ? (
          <div className="settings-group">
            <label htmlFor="kb-local-path">Vault 目录</label>
            <input
              id="kb-local-path"
              value={config.localPath}
              onChange={(event) => setConfig({ ...config, localPath: event.target.value })}
              placeholder="/data/ObsidianVault"
              required={config.enabled}
            />
          </div>
        ) : (
          <div className="settings-group">
            <label htmlFor={config.providerKind === "fns" ? "kb-base-url" : "kb-mcp-url"}>
              {config.providerKind === "fns" ? "FNS 服务地址" : "MCP 地址"}
            </label>
            <input
              id={config.providerKind === "fns" ? "kb-base-url" : "kb-mcp-url"}
              type="url"
              value={config.providerKind === "fns" ? config.baseUrl : config.mcpUrl}
              onChange={(event) => setConfig(config.providerKind === "fns"
                ? { ...config, baseUrl: event.target.value }
                : { ...config, mcpUrl: event.target.value })}
              placeholder={config.providerKind === "fns"
                ? "http://127.0.0.1:9000"
                : "http://127.0.0.1:9000/api/mcp"}
              required={config.enabled}
            />
          </div>
        )}
      </div>

      {config.providerKind === "local_vault" && (
        <div className="kb-vault-actions">
          <button
            className="command-button"
            type="button"
            onClick={() => void createLocalVault()}
            disabled={!desktop || busy || creatingVault}
          >
            {creatingVault ? <LoaderCircle className="spin" size={14} /> : <FolderPlus size={14} />}
            {config.localPath.trim() ? "初始化 / 修复目录" : "创建本地知识库"}
          </button>
          <small>默认建在应用配置目录；目录已存在时只补齐子目录和说明，不覆盖笔记。</small>
        </div>
      )}

      {config.providerKind === "fns" && (
        <div className="credential-stack">
          {credentialEditor("fns", "REST token", "输入带 REST 权限的 token")}
          {credentialEditor("fnsWs", "WebSocket token", "输入带 p:ws 权限的 token")}
        </div>
      )}
      {config.providerKind === "mcp" && credentialEditor("mcp", "MCP token", "输入 Bearer token")}

      <div className="knowledge-options">
        <label className="toggle-row">
          <span><b>本地索引</b><small>离线缓存与全文检索</small></span>
          <input
            type="checkbox"
            checked={config.indexEnabled}
            onChange={(event) => setConfig({ ...config, indexEnabled: event.target.checked })}
            disabled={busy}
          />
        </label>
        <label className="toggle-row">
          <span><b>自动打标</b><small>补全文种、年份与主题字段</small></span>
          <input
            type="checkbox"
            checked={config.autoTaggingEnabled}
            onChange={(event) => setConfig({ ...config, autoTaggingEnabled: event.target.checked })}
            disabled={busy || !config.indexEnabled}
          />
        </label>
        <label className="toggle-row">
          <span><b>增量同步</b><small>FNS WebSocket 变更游标</small></span>
          <input
            type="checkbox"
            checked={config.syncEnabled}
            onChange={(event) => setConfig({ ...config, syncEnabled: event.target.checked })}
            disabled={busy || config.providerKind !== "fns" || !config.indexEnabled}
          />
        </label>
      </div>

      {config.providerKind === "local_vault" && (
        <section className="kb-note-panel" aria-label="添加知识资料">
          <header>
            <span><NotebookPen size={14} /> 添加资料</span>
            <small>上传或手写，保存为 Markdown 进入「收件箱 /」</small>
          </header>
          <div className="kb-upload-row">
            <label className="command-button kb-upload-button">
              {uploadBusy ? <LoaderCircle className="spin" size={14} /> : <Upload size={14} />}
              {uploadBusy && uploadProgress
                ? `正在导入 ${uploadProgress.done + 1}/${uploadProgress.total}`
                : "上传材料"}
              <input
                className="visually-hidden"
                type="file"
                multiple
                accept={uploadAccept()}
                aria-label="上传知识库材料"
                onChange={(event) => {
                  const files = Array.from(event.target.files ?? []);
                  event.target.value = "";
                  if (files.length > 0) void uploadMaterials(files);
                }}
                disabled={!desktop || uploadBusy || !config.enabled}
              />
            </label>
            <span>
              支持 TXT、Markdown、CSV、JSON、YAML、PDF、OFD、DOC、DOCX、XLSX、XLS、WPS；文本会转换为 Markdown 存入「收件箱 /」。
            </span>
          </div>
          {uploadProgress && (
            <div className="kb-upload-progress" role="status">
              <LoaderCircle className="spin" size={13} /> 正在解析：{uploadProgress.name}
            </div>
          )}
          {uploadErrors.length > 0 && (
            <ul className="kb-upload-errors" aria-label="上传失败的文件">
              {uploadErrors.map((item) => <li key={item}>{item}</li>)}
            </ul>
          )}
          <div className="kb-manual-divider">或手动填写</div>
          <input
            aria-label="资料标题"
            value={noteTitle}
            onChange={(event) => setNoteTitle(event.target.value)}
            placeholder="资料标题，例如：2026 年防汛工作要点"
            maxLength={120}
            disabled={!desktop || noteBusy || uploadBusy || !config.enabled}
          />
          <textarea
            aria-label="资料内容"
            rows={5}
            value={noteContent}
            onChange={(event) => setNoteContent(event.target.value)}
            placeholder="粘贴政策原文、会议纪要、历史公文或写作素材。保存后可在左侧检索并引用。"
            disabled={!desktop || noteBusy || uploadBusy || !config.enabled}
          />
          <div className="kb-note-actions">
            <label className="kb-inline-check">
              <input
                type="checkbox"
                checked={indexAfterNote}
                onChange={(event) => setIndexAfterNote(event.target.checked)}
                disabled={!desktop || noteBusy || uploadBusy || !config.indexEnabled}
              />
              保存 / 上传后重建索引
            </label>
            <button
              className="command-button primary"
              type="button"
              onClick={() => void saveNote()}
              disabled={!desktop || noteBusy || uploadBusy || !config.enabled || !noteTitle.trim() || !noteContent.trim()}
            >
              {noteBusy ? <LoaderCircle className="spin" size={14} /> : <NotebookPen size={14} />}
              保存到知识库
            </button>
          </div>
        </section>
      )}

      <section className="index-panel" aria-label="本地索引状态">
        <header>
          <span><Database size={14} /> 本地索引</span>
          <span className={`chip ${indexState?.status === "ready" ? "chip-ok" : "chip-neutral"}`}>
            {stateLabel(indexState)}
          </span>
        </header>
        <div className="index-metrics">
          <span><b>{indexState?.noteCount ?? 0}</b> 篇素材</span>
          <span><b>{indexState?.chunkCount ?? 0}</b> 个片段</span>
          <span><b>{indexState?.lastCursor ?? 0}</b> 同步游标</span>
          <span>{formatIndexTime(indexState?.updatedAt ?? 0)}</span>
        </div>
        {indexState?.errorMessage && <div className="index-error">{indexState.errorMessage}</div>}
        {indexProgress && operation === "index" && (
          <div className="index-progress">
            <progress max={100} value={progressValue} />
            <span>{indexProgress.total ? `${indexProgress.processed} / ${indexProgress.total}` : "正在读取素材"}</span>
            {indexProgress.path && <small>{indexProgress.path}</small>}
          </div>
        )}
        <div className="index-actions">
          <button
            className="command-button"
            type="button"
            onClick={() => void rebuildIndex()}
            disabled={!desktop || busy || !config.enabled || !config.indexEnabled}
          >
            {operation === "index" ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}
            重建索引
          </button>
          <button
            className="command-button"
            type="button"
            onClick={() => void syncOnce()}
            disabled={!desktop || busy || config.providerKind !== "fns" || !config.syncEnabled}
          >
            {operation === "sync" ? <LoaderCircle className="spin" size={14} /> : <Radio size={14} />}
            立即同步
          </button>
          <button
            className="icon-button small danger-action"
            type="button"
            onClick={() => void clearIndex()}
            disabled={!desktop || busy || !indexState}
            title="清理本地索引"
            aria-label="清理本地索引"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </section>

      <div className="provider-status" role="status">
        {!desktop && <span><Unplug size={13} /> 桌面应用中可连接知识库和保存凭据</span>}
        {message && <span>{message}</span>}
      </div>
      <footer className="modal-footer">
        <button
          className="command-button"
          type="button"
          onClick={() => void testConnection()}
          disabled={!desktop || busy || !config.enabled}
        >
          {operation === "test" ? <LoaderCircle className="spin" size={14} /> : <SearchCheck size={14} />}
          测试连接
        </button>
        <button className="command-button primary" type="submit" disabled={busy}>
          {operation === "save" ? <LoaderCircle className="spin" size={14} /> : <Check size={15} />}
          保存配置
        </button>
      </footer>
    </form>
  );
}
