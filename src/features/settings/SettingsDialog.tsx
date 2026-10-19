import {
  BookOpen,
  Check,
  Download,
  Eye,
  EyeOff,
  KeyRound,
  LoaderCircle,
  FileUp,
  Plus,
  RefreshCw,
  ServerCog,
  ShieldAlert,
  Trash2,
  Unplug,
  X,
} from "lucide-react";
import { useModal } from "../../hooks/useModal";
import { type FormEvent, useEffect, useMemo, useState } from "react";
import {
  applyUpdate,
  checkForUpdate,
  deleteProviderSecret,
  deleteDocxTemplate,
  exportCapabilities,
  hasProviderSecret,
  ipcErrorMessage,
  isDesktopRuntime,
  listProviderModels,
  listProviderProfiles,
  saveProviderProfile,
  saveDocxTemplate,
  setProviderSecret,
  testProviderConnection,
  type UpdateCheckResult,
} from "../../lib/ipc";
import type { ProviderKind, ProviderProfile, ProviderProfileInput } from "../../lib/llm";
import { PROVIDER_PRESETS, providerPresetInput } from "../../lib/providerPresets";
import { probeGbFonts, type FontProbe } from "../../lib/fonts";
import type { AppConfig, AppTheme } from "../../lib/store";
import { CrashDiagnosticsSettings } from "./CrashDiagnosticsSettings";
import { ClinePassSettings } from "./ClinePassSettings";
import { GenerationLogSettings } from "./GenerationLogSettings";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";
import { PackSettings } from "./PackSettings";

type SettingsTab = "general" | "providers" | "clinepass" | "knowledge" | "packs";

interface SettingsDialogProps {
  open: boolean;
  config: AppConfig;
  onClose: () => void;
  onSave: (config: AppConfig) => Promise<void>;
  onShowGuide?: () => void;
  /** Tab to open on; command-palette and deep links use this. Defaults to 常规. */
  initialTab?: SettingsTab;
}

const themeOptions: Array<{ value: AppTheme; label: string }> = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "亮色" },
  { value: "dark", label: "暗色" },
];

const providerKindOptions: Array<{ value: ProviderKind; label: string }> = [
  { value: "openai_compatible", label: "OpenAI 兼容" },
  { value: "anthropic", label: "Anthropic (Claude)" },
  { value: "ollama", label: "Ollama" },
];

const fontFields = ["仿宋_GB2312", "楷体_GB2312", "方正小标宋简体"];

function newProviderDraft(): ProviderProfileInput {
  return {
    id: "",
    kind: "openai_compatible",
    displayName: "",
    baseUrl: "https://api.example.com",
    model: "",
    enabled: true,
  };
}

function profileInput(profile: ProviderProfile): ProviderProfileInput {
  const { isLocal: _isLocal, ...input } = profile;
  return input;
}

export function SettingsDialog({ open, config, onClose, onSave, onShowGuide, initialTab = "general" }: SettingsDialogProps) {
  const [tab, setTab] = useState<SettingsTab>("general");
  const [draft, setDraft] = useState(config);
  const [saving, setSaving] = useState(false);
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [providerDraft, setProviderDraft] = useState<ProviderProfileInput | null>(null);
  const [creatingProvider, setCreatingProvider] = useState(false);
  const [providerBusy, setProviderBusy] = useState(false);
  const [providerMessage, setProviderMessage] = useState<string | null>(null);
  const [presetNote, setPresetNote] = useState<string | null>(null);
  const [secret, setSecret] = useState("");
  const [showSecret, setShowSecret] = useState(false);
  const [secretExists, setSecretExists] = useState(false);
  const [secretBusy, setSecretBusy] = useState(false);
  const [secretMessage, setSecretMessage] = useState<string | null>(null);
  const [templateConfigured, setTemplateConfigured] = useState(false);
  const [templateBusy, setTemplateBusy] = useState(false);
  const [templateMessage, setTemplateMessage] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [sniffing, setSniffing] = useState(false);
  const [sniffMessage, setSniffMessage] = useState<string | null>(null);
  const [fontProbe, setFontProbe] = useState<FontProbe | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const desktop = isDesktopRuntime();

  const selectedProfile = useMemo(
    () => profiles.find((profile) => profile.id === providerDraft?.id) ?? null,
    [profiles, providerDraft?.id],
  );

  useEffect(() => {
    if (!open) return;
    setDraft(structuredClone(config));
    setTab("general");
    setFontProbe(probeGbFonts());
    setProviderBusy(true);
    setProviderMessage(null);
    setCreatingProvider(false);
    listProviderProfiles()
      .then((loaded) => {
        setProfiles(loaded);
        const initial = loaded.find((profile) => profile.id === "openai-compatible") ?? loaded[0];
        setProviderDraft(initial ? profileInput(initial) : newProviderDraft());
        setCreatingProvider(!initial);
      })
      .catch((error) => setProviderMessage(ipcErrorMessage(error)))
      .finally(() => setProviderBusy(false));
    exportCapabilities()
      .then((capabilities) => setTemplateConfigured(capabilities.customTemplateConfigured))
      .catch((error) => setTemplateMessage(ipcErrorMessage(error)));
  }, [config, open]);

  useEffect(() => {
    if (!open || !desktop || creatingProvider || !selectedProfile) {
      setSecretExists(false);
      return;
    }
    setSecretBusy(true);
    setSecretMessage(null);
    hasProviderSecret(selectedProfile.id)
      .then(setSecretExists)
      .catch((error) => setSecretMessage(ipcErrorMessage(error)))
      .finally(() => setSecretBusy(false));
  }, [creatingProvider, desktop, open, selectedProfile]);

  const sectionRef = useModal<HTMLElement>(open, onClose);

  useEffect(() => {
    if (open) setTab(initialTab);
  }, [initialTab, open]);

  if (!open) return null;

  async function submitGeneral(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave(draft);
      onClose();
    } catch {
      // The parent surfaces the safe IPC error and keeps this dialog open.
    } finally {
      setSaving(false);
    }
  }

  async function runUpdateCheck() {
    setUpdateBusy(true);
    setUpdateMessage(null);
    setUpdateResult(null);
    try {
      setUpdateResult(await checkForUpdate());
    } catch (error) {
      setUpdateMessage(ipcErrorMessage(error));
    } finally {
      setUpdateBusy(false);
    }
  }

  async function installUpdate() {
    setUpdateBusy(true);
    setUpdateMessage(null);
    try {
      await applyUpdate();
      // Success restarts the app; reaching here means the restart was deferred.
    } catch (error) {
      setUpdateMessage(ipcErrorMessage(error));
    } finally {
      setUpdateBusy(false);
    }
  }

  function selectProvider(providerId: string) {
    const profile = profiles.find((candidate) => candidate.id === providerId);
    if (!profile) return;
    setProviderDraft(profileInput(profile));
    setCreatingProvider(false);
    setProviderMessage(null);
    setPresetNote(null);
    setSecret("");
    setSecretMessage(null);
    setModels([]);
    setSniffMessage(null);
  }

  function addProvider() {
    setProviderDraft(newProviderDraft());
    setCreatingProvider(true);
    setProviderMessage(null);
    setPresetNote(null);
    setSecret("");
    setSecretExists(false);
    setSecretMessage(null);
    setModels([]);
    setSniffMessage(null);
  }

  function applyPreset(presetId: string) {
    const preset = PROVIDER_PRESETS.find((candidate) => candidate.id === presetId);
    if (!preset) return;
    setProviderDraft(providerPresetInput(preset, profiles.map((profile) => profile.id)));
    setCreatingProvider(true);
    setProviderMessage(null);
    setPresetNote(`${preset.note} 填写 API Key 后点「保存服务」即可启用。`);
    setSecret("");
    setSecretExists(false);
    setSecretMessage(null);
    setModels([]);
    setSniffMessage(null);
  }

  async function persistProvider(): Promise<ProviderProfile | null> {
    if (!providerDraft) return null;
    const saved = await saveProviderProfile(providerDraft);
    setProfiles((current) => {
      const next = current.filter((profile) => profile.id !== saved.id);
      next.push(saved);
      return next.sort(
        (left, right) => left.displayName.localeCompare(right.displayName, "zh-CN") || left.id.localeCompare(right.id),
      );
    });
    setProviderDraft(profileInput(saved));
    setCreatingProvider(false);
    return saved;
  }

  async function submitProvider(event: FormEvent) {
    event.preventDefault();
    setProviderBusy(true);
    setProviderMessage(null);
    try {
      const saved = await persistProvider();
      if (saved && secret.trim()) {
        // A preset flow should be one shot: save the profile and its key together.
        await setProviderSecret(saved.id, secret);
        setSecret("");
        setSecretExists(true);
        setProviderMessage("服务配置与凭据已保存");
      } else {
        setProviderMessage("服务配置已保存");
      }
    } catch (error) {
      setProviderMessage(ipcErrorMessage(error));
    } finally {
      setProviderBusy(false);
    }
  }

  async function testConnection() {
    if (!providerDraft) return;
    setProviderBusy(true);
    setProviderMessage(null);
    try {
      const saved = await persistProvider();
      if (!saved) return;
      if (secret.trim()) {
        // Testing right after adding a preset should not fail for lack of the key the user
        // already typed into the form.
        await setProviderSecret(saved.id, secret);
        setSecret("");
        setSecretExists(true);
      }
      const result = await testProviderConnection(saved.id);
      setProviderMessage(`连接成功 · ${result.latencyMs} ms · ${result.model}`);
    } catch (error) {
      setProviderMessage(ipcErrorMessage(error));
    } finally {
      setProviderBusy(false);
    }
  }

  async function saveSecret() {
    if (!providerDraft || creatingProvider || !secret.trim()) return;
    setSecretBusy(true);
    setSecretMessage(null);
    try {
      await setProviderSecret(providerDraft.id, secret);
      setSecret("");
      setSecretExists(true);
      setSecretMessage("凭据已保存");
    } catch (error) {
      setSecretMessage(ipcErrorMessage(error));
    } finally {
      setSecretBusy(false);
    }
  }

  async function removeSecret() {
    if (!providerDraft || creatingProvider) return;
    setSecretBusy(true);
    setSecretMessage(null);
    try {
      await deleteProviderSecret(providerDraft.id);
      setSecretExists(false);
      setSecret("");
      setSecretMessage("凭据已删除");
    } catch (error) {
      setSecretMessage(ipcErrorMessage(error));
    } finally {
      setSecretBusy(false);
    }
  }

  async function uploadTemplate(file: File | null) {
    if (!file) return;
    setTemplateBusy(true);
    setTemplateMessage(null);
    try {
      await saveDocxTemplate(new Uint8Array(await file.arrayBuffer()));
      setTemplateConfigured(true);
      setTemplateMessage("机关 DOCX 模板已保存");
    } catch (error) {
      setTemplateMessage(ipcErrorMessage(error));
    } finally {
      setTemplateBusy(false);
    }
  }

  async function removeTemplate() {
    setTemplateBusy(true);
    setTemplateMessage(null);
    try {
      await deleteDocxTemplate();
      setTemplateConfigured(false);
      setTemplateMessage("已恢复 GB/T 9704 默认导出");
    } catch (error) {
      setTemplateMessage(ipcErrorMessage(error));
    } finally {
      setTemplateBusy(false);
    }
  }

  async function sniffModels() {
    if (!providerDraft || !desktop) return;
    setSniffing(true);
    setSniffMessage(null);
    try {
      const saved = await persistProvider();
      if (!saved) return;
      const discovered = await listProviderModels(saved.id);
      setModels(discovered);
      setSniffMessage(
        discovered.length > 0 ? `发现 ${discovered.length} 个可用模型` : "上游未返回可用模型",
      );
    } catch (error) {
      setModels([]);
      setSniffMessage(ipcErrorMessage(error));
    } finally {
      setSniffing(false);
    }
  }

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section ref={sectionRef} tabIndex={-1} className="modal settings-modal" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header className="modal-header">
          <div>
            <h2 id="settings-title">设置</h2>
            <span>本地应用配置</span>
          </div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭" aria-label="关闭设置">
            <X size={17} />
          </button>
        </header>
        <div className="settings-layout">
          <nav className="settings-tabs" aria-label="设置分类">
            <button className={tab === "general" ? "is-active" : ""} type="button" onClick={() => setTab("general")}>
              常规
            </button>
            <button className={tab === "providers" ? "is-active" : ""} type="button" onClick={() => setTab("providers")}>
              模型服务
            </button>
            <button className={tab === "clinepass" ? "is-active" : ""} type="button" onClick={() => setTab("clinepass")}>
              Cline Pass
            </button>
            <button className={tab === "knowledge" ? "is-active" : ""} type="button" onClick={() => setTab("knowledge")}>
              知识库
            </button>
            <button className={tab === "packs" ? "is-active" : ""} type="button" onClick={() => setTab("packs")}>
              知识包
            </button>
          </nav>
          {tab === "general" ? (
            <form className="settings-form" onSubmit={submitGeneral}>
              <div className="settings-group">
                <label>主题</label>
                <div className="segmented-control">
                  {themeOptions.map((option) => (
                    <button
                      className={draft.theme === option.value ? "is-active" : ""}
                      type="button"
                      onClick={() => setDraft({ ...draft, theme: option.value })}
                      key={option.value}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>
              <label className="toggle-row">
                <span>
                  <b>自动保存</b>
                  <small>编辑停止 1.2 秒后保存版本</small>
                </span>
                <input
                  type="checkbox"
                  checked={draft.autosaveEnabled}
                  onChange={(event) => setDraft({ ...draft, autosaveEnabled: event.target.checked })}
                />
              </label>
              <label className="toggle-row">
                <span>
                  <b>自动检查更新</b>
                  <small>启动时检查 GitHub Releases；下载安装仍需确认</small>
                </span>
                <input
                  type="checkbox"
                  checked={draft.autoUpdateEnabled}
                  onChange={(event) => setDraft({ ...draft, autoUpdateEnabled: event.target.checked })}
                  disabled={!desktop}
                />
              </label>
              <CrashDiagnosticsSettings
                enabled={draft.crashReportingEnabled}
                desktop={desktop}
                onEnabledChange={(enabled) => setDraft({ ...draft, crashReportingEnabled: enabled })}
              />
              <GenerationLogSettings />
              <div className="settings-group">
                <label>字体映射</label>
                {fontProbe?.supported && fontProbe.missing.length > 0 && (
                  <div className="font-warning" role="status">
                    <ShieldAlert size={13} />
                    系统缺少：{fontProbe.missing.join("、")}。预览使用替代字体，导出的 DOCX 仍写入标准字体名，
                    请在装有正版字体的机器上定稿，或在下方为缺失字体指定替代字体。
                  </div>
                )}
                {fontProbe && !fontProbe.supported && (
                  <div className="font-warning">
                    <ShieldAlert size={13} />
                    当前运行环境无法探测系统字体，字体提示不可用。
                  </div>
                )}
                <div className="font-mapping-list">
                  {fontFields.map((font) => (
                    <label className="font-mapping-row" key={font}>
                      <span>{font}</span>
                      <input
                        value={draft.fontMappings[font] ?? ""}
                        onChange={(event) =>
                          setDraft({
                            ...draft,
                            fontMappings: { ...draft.fontMappings, [font]: event.target.value },
                          })
                        }
                        placeholder="使用系统默认回退字体"
                      />
                    </label>
                  ))}
                </div>
              </div>
              <div className="settings-group export-template-settings">
                <label>机关 DOCX 模板</label>
                <div className="template-actions">
                  <span className={`chip ${templateConfigured ? "chip-ok" : "chip-neutral"}`}>
                    {templateConfigured ? "已配置" : "使用国标默认"}
                  </span>
                  <label className={`command-button ${!desktop || templateBusy ? "is-disabled" : ""}`}>
                    <FileUp size={14} />
                    选择模板
                    <input
                      className="visually-hidden"
                      type="file"
                      accept=".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                      disabled={!desktop || templateBusy}
                      onChange={(event) => {
                        void uploadTemplate(event.target.files?.[0] ?? null);
                        event.target.value = "";
                      }}
                    />
                  </label>
                  <button
                    className="icon-button tiny"
                    type="button"
                    onClick={() => void removeTemplate()}
                    disabled={!desktop || !templateConfigured || templateBusy}
                    title="删除机关模板"
                    aria-label="删除机关模板"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
                {templateMessage && <span className="field-message">{templateMessage}</span>}
              </div>
              <div className="settings-group">
                <label>关于与更新</label>
                <div className="update-row">
                  <span className="settings-hint">
                    当前 v{updateResult?.currentVersion ?? (typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev")}。更新包由发布签名验证，仅在桌面版可用。
                  </span>
                  <button
                    className="command-button"
                    type="button"
                    onClick={() => void runUpdateCheck()}
                    disabled={updateBusy || !desktop}
                    title={desktop ? undefined : "浏览器预览不可用"}
                  >
                    {updateBusy ? <LoaderCircle size={14} className="is-spinning" /> : <RefreshCw size={14} />}
                    检查更新
                  </button>
                </div>
                {updateResult?.available && (
                  <div className="update-available" role="status">
                    <span>
                      发现新版本 v{updateResult.version}（当前 v{updateResult.currentVersion}），安装后将自动重启。
                    </span>
                    <button
                      className="command-button primary"
                      type="button"
                      onClick={() => void installUpdate()}
                      disabled={updateBusy}
                    >
                      <Download size={14} />
                      立即更新
                    </button>
                  </div>
                )}
                {updateResult && !updateResult.available && !updateMessage && (
                  <div className="source-message" role="status">
                    当前 v{updateResult.currentVersion} 已是最新版本。
                  </div>
                )}
                {updateMessage && <div className="source-message is-error" role="alert">{updateMessage}</div>}
              </div>
              <footer className="modal-footer">
                <button className="command-button" type="button" onClick={onClose}>取消</button>
                {onShowGuide && (
                  <button className="command-button" type="button" onClick={onShowGuide}>
                    <BookOpen size={14} />
                    重新查看欢迎引导
                  </button>
                )}
                <button className="command-button primary" type="submit" disabled={saving}>
                  <Check size={15} />
                  保存设置
                </button>
              </footer>
            </form>
          ) : tab === "providers" ? (
            <form className="settings-form provider-form" onSubmit={submitProvider}>
              <div className="provider-preset-picker">
                <label htmlFor="provider-preset">常用服务</label>
                <select
                  id="provider-preset"
                  value=""
                  onChange={(event) => applyPreset(event.target.value)}
                  disabled={providerBusy}
                >
                  <option value="">选择预设快速填入…</option>
                  <optgroup label="云端服务">
                    {PROVIDER_PRESETS.filter((preset) => preset.group === "cloud").map((preset) => (
                      <option value={preset.id} key={preset.id}>{preset.displayName}</option>
                    ))}
                  </optgroup>
                  <optgroup label="本地服务">
                    {PROVIDER_PRESETS.filter((preset) => preset.group === "local").map((preset) => (
                      <option value={preset.id} key={preset.id}>{preset.displayName}</option>
                    ))}
                  </optgroup>
                </select>
              </div>
              {presetNote && <p className="provider-preset-note">{presetNote}</p>}
              <div className="provider-picker">
                <label htmlFor="provider-picker">模型服务</label>
                <select
                  id="provider-picker"
                  value={creatingProvider ? "__new__" : providerDraft?.id ?? ""}
                  onChange={(event) => selectProvider(event.target.value)}
                  disabled={providerBusy || profiles.length === 0}
                >
                  {creatingProvider && <option value="__new__">新模型服务</option>}
                  {profiles.map((profile) => (
                    <option value={profile.id} key={profile.id}>{profile.displayName}</option>
                  ))}
                </select>
                <button className="icon-button" type="button" onClick={addProvider} title="新增模型服务" aria-label="新增模型服务">
                  <Plus size={16} />
                </button>
              </div>

              {providerDraft ? (
                <>
                  <div className="provider-field-grid">
                    <div className="settings-group">
                      <label htmlFor="provider-name">名称</label>
                      <input
                        id="provider-name"
                        value={providerDraft.displayName}
                        onChange={(event) => setProviderDraft({ ...providerDraft, displayName: event.target.value })}
                        placeholder="模型服务名称"
                        required
                      />
                    </div>
                    <div className="settings-group">
                      <label htmlFor="provider-id">服务 ID</label>
                      <input
                        id="provider-id"
                        value={providerDraft.id}
                        onChange={(event) => setProviderDraft({ ...providerDraft, id: event.target.value })}
                        disabled={!creatingProvider}
                        pattern={["[A-Za-z0-9._\\-]+"].join("")}
                        maxLength={64}
                        placeholder="custom-provider"
                        required
                      />
                    </div>
                  </div>
                  {/* Cline Pass has one fixed protocol and its model list lives in the Cline Pass tab. */}
                  {providerDraft.id !== "cline-pass" && (
                    <div className="settings-group">
                      <label>接口类型</label>
                      <div className="segmented-control provider-kind-control">
                        {providerKindOptions.map((option) => (
                          <button
                            className={providerDraft.kind === option.value ? "is-active" : ""}
                            type="button"
                            onClick={() => setProviderDraft({ ...providerDraft, kind: option.value })}
                            key={option.value}
                          >
                            {option.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                  <div className="settings-group">
                    <label htmlFor="provider-url">服务地址</label>
                    <input
                      id="provider-url"
                      type="url"
                      value={providerDraft.baseUrl}
                      onChange={(event) => setProviderDraft({ ...providerDraft, baseUrl: event.target.value })}
                      placeholder="https://api.example.com"
                      required
                    />
                  </div>
                  <div className="settings-group">
                    <label htmlFor="provider-model">模型</label>
                    <div className="model-sniff-row">
                      <input
                        id="provider-model"
                        value={providerDraft.model}
                        onChange={(event) => setProviderDraft({ ...providerDraft, model: event.target.value })}
                        placeholder="model-name"
                        list="provider-model-list"
                        required
                      />
                      <button
                        className="icon-button"
                        type="button"
                        onClick={() => void sniffModels()}
                        disabled={!desktop || sniffing || !providerDraft.enabled}
                        title="嗅探上游可用模型"
                        aria-label="嗅探上游可用模型"
                      >
                        {sniffing ? <LoaderCircle className="spin" size={15} /> : <RefreshCw size={15} />}
                      </button>
                    </div>
                    <datalist id="provider-model-list">
                      {models.map((model) => <option value={model} key={model} />)}
                    </datalist>
                    {sniffMessage && <span className="field-message">{sniffMessage}</span>}
                  </div>
                  <label className="toggle-row provider-enabled-row">
                    <span><b>启用服务</b></span>
                    <input
                      type="checkbox"
                      checked={providerDraft.enabled}
                      onChange={(event) => setProviderDraft({ ...providerDraft, enabled: event.target.checked })}
                    />
                  </label>

                  {(providerDraft.kind === "openai_compatible" || providerDraft.kind === "anthropic") && (
                    <div className="credential-panel">
                      <div className="settings-group">
                        <label htmlFor="credential-secret">API Key</label>
                        <div className="secret-input">
                          <KeyRound size={15} />
                          <input
                            id="credential-secret"
                            type={showSecret ? "text" : "password"}
                            value={secret}
                            onChange={(event) => setSecret(event.target.value)}
                            disabled={!desktop}
                            autoComplete="new-password"
                            placeholder={secretExists ? "已保存，输入新值可替换" : "输入凭据"}
                          />
                          <button
                            className="icon-button tiny"
                            type="button"
                            onClick={() => setShowSecret(!showSecret)}
                            disabled={!desktop}
                            title={showSecret ? "隐藏凭据" : "显示凭据"}
                            aria-label={showSecret ? "隐藏凭据" : "显示凭据"}
                          >
                            {showSecret ? <EyeOff size={14} /> : <Eye size={14} />}
                          </button>
                        </div>
                      </div>
                      <div className="credential-actions">
                        <span className={`chip ${secretExists ? "chip-ok" : "chip-neutral"}`}>
                          {secretBusy ? "检查中" : secretExists ? "凭据已保存" : "无凭据"}
                        </span>
                        <button
                          className="icon-button tiny"
                          type="button"
                          onClick={() => void removeSecret()}
                          disabled={!desktop || !secretExists || secretBusy || creatingProvider}
                          title="删除凭据"
                          aria-label="删除凭据"
                        >
                          <Trash2 size={14} />
                        </button>
                        <button
                          className="command-button"
                          type="button"
                          onClick={() => void saveSecret()}
                          disabled={!desktop || !secret.trim() || secretBusy || creatingProvider}
                        >
                          <KeyRound size={14} />
                          保存凭据
                        </button>
                      </div>
                    </div>
                  )}

                  <div className="provider-status" role="status">
                    {!desktop && <span><Unplug size={13} /> 桌面应用中可测试连接</span>}
                    {providerMessage && <span>{providerMessage}</span>}
                    {secretMessage && <span>{secretMessage}</span>}
                  </div>
                  <footer className="modal-footer">
                    <button
                      className="command-button"
                      type="button"
                      onClick={() => void testConnection()}
                      disabled={!desktop || providerBusy || !providerDraft.enabled}
                    >
                      {providerBusy ? <LoaderCircle className="spin" size={14} /> : <ServerCog size={14} />}
                      测试连接
                    </button>
                    <button className="command-button primary" type="submit" disabled={providerBusy}>
                      <Check size={15} />
                      保存服务
                    </button>
                  </footer>
                </>
              ) : (
                <div className="modal-empty">{providerBusy ? "正在加载" : "没有可用的模型服务"}</div>
              )}
            </form>
          ) : tab === "clinepass" ? (
            <ClinePassSettings desktop={desktop} />
          ) : tab === "packs" ? (
            <PackSettings
              activePackIds={draft.activePackIds}
              onSave={async (activePackIds) => {
                await onSave({ ...draft, activePackIds });
                onClose();
              }}
            />
          ) : <KnowledgeBaseSettings desktop={desktop} />}
        </div>
      </section>
    </div>
  );
}
