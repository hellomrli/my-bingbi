import { Check, ExternalLink, KeyRound, LoaderCircle, RefreshCw, Trash2, Unplug } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  CLINE_PASS_BASE_URL,
  CLINE_PASS_PROVIDER_ID,
  clinePassOverview,
  formatQuotaReset,
  openClinePassSubscription,
  quotaLabel,
  type ClinePassModel,
  type ClinePassOverview,
} from "../../lib/clinepass";
import {
  deleteProviderSecret,
  hasProviderSecret,
  ipcErrorMessage,
  listProviderProfiles,
  saveProviderProfile,
  setProviderSecret,
} from "../../lib/ipc";

interface ClinePassSettingsProps {
  desktop: boolean;
}

function modelMeta(model: ClinePassModel): string {
  const bits = [model.id];
  if (model.contextWindow) bits.push(`${Math.round(model.contextWindow / 1000)}K 上下文`);
  return bits.join(" · ");
}

/**
 * Cline Pass subscription console: quota windows, the Chinese-model catalog, and a one-click
 * switch of the `cline-pass` provider profile. The key is stored through the same keyring path
 * as every other provider; this panel never receives it back.
 */
export function ClinePassSettings({ desktop }: ClinePassSettingsProps) {
  const [overview, setOverview] = useState<ClinePassOverview | null>(null);
  const [currentModel, setCurrentModel] = useState("");
  const [currentEnabled, setCurrentEnabled] = useState(false);
  const [key, setKey] = useState("");
  const [keyExists, setKeyExists] = useState(false);
  const [busy, setBusy] = useState(false);
  const [keyBusy, setKeyBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const refresh = useCallback(async () => {
    if (!desktop) return;
    setBusy(true);
    setError(null);
    try {
      const [data, profiles] = await Promise.all([clinePassOverview(), listProviderProfiles()]);
      setOverview(data);
      const profile = profiles.find((candidate) => candidate.id === CLINE_PASS_PROVIDER_ID);
      setCurrentModel(profile?.model ?? "");
      setCurrentEnabled(profile?.enabled ?? false);
    } catch (loadError) {
      setError(ipcErrorMessage(loadError));
    } finally {
      setBusy(false);
    }
  }, [desktop]);

  useEffect(() => {
    if (!desktop) return;
    hasProviderSecret(CLINE_PASS_PROVIDER_ID)
      .then(setKeyExists)
      .catch((loadError) => setError(ipcErrorMessage(loadError)));
    void refresh();
  }, [desktop, refresh]);

  async function saveKey() {
    if (!key.trim() || keyBusy) return;
    setKeyBusy(true);
    setMessage(null);
    setError(null);
    try {
      await setProviderSecret(CLINE_PASS_PROVIDER_ID, key.trim());
      setKey("");
      setKeyExists(true);
      setMessage("Cline Pass Key 已保存到系统凭据库");
      await refresh();
    } catch (saveError) {
      setError(ipcErrorMessage(saveError));
    } finally {
      setKeyBusy(false);
    }
  }

  async function clearKey() {
    if (keyBusy) return;
    setKeyBusy(true);
    setMessage(null);
    setError(null);
    try {
      await deleteProviderSecret(CLINE_PASS_PROVIDER_ID);
      setKeyExists(false);
      setOverview((current) => (current ? { ...current, configured: false, limits: [] } : current));
      setMessage("已删除本机保存的 Cline Pass Key");
    } catch (deleteError) {
      setError(ipcErrorMessage(deleteError));
    } finally {
      setKeyBusy(false);
    }
  }

  async function applyModel(model: ClinePassModel) {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await saveProviderProfile({
        id: CLINE_PASS_PROVIDER_ID,
        kind: "openai_compatible",
        displayName: "Cline Pass",
        baseUrl: CLINE_PASS_BASE_URL,
        model: model.id,
        enabled: true,
      });
      setCurrentModel(model.id);
      setCurrentEnabled(true);
      setMessage(`已切换到 ${model.name}；关闭设置后即可在写作区选择 Cline Pass`);
    } catch (saveError) {
      setError(ipcErrorMessage(saveError));
    } finally {
      setBusy(false);
    }
  }

  async function openSubscription() {
    setError(null);
    try {
      await openClinePassSubscription();
    } catch (openError) {
      setError(ipcErrorMessage(openError));
    }
  }

  if (!desktop) {
    return (
      <div className="clinepass">
        <div className="clinepass-hero">
          <div>
            <strong>Cline Pass</strong>
            <span>一个订阅覆盖 DeepSeek、GLM、Kimi、Qwen、MiniMax 等国模。</span>
          </div>
        </div>
        <div className="modal-empty">
          <Unplug size={15} /> Cline Pass 的额度与模型列表需要桌面内核读取，请在桌面版中使用。
        </div>
      </div>
    );
  }

  const models = (overview?.models ?? []).filter((model) => {
    const needle = query.trim().toLowerCase();
    return !needle || model.name.toLowerCase().includes(needle) || model.id.toLowerCase().includes(needle);
  });

  return (
    <div className="clinepass">
      <div className="clinepass-hero">
        <div>
          <strong>Cline Pass</strong>
          <span>一个订阅覆盖 DeepSeek、GLM、Kimi、Qwen、MiniMax 等国模；Key 只存本机凭据库。</span>
        </div>
        <button className="command-button" type="button" onClick={() => void openSubscription()}>
          <ExternalLink size={14} />
          打开订阅页
        </button>
      </div>

      <div className="settings-group">
        <label htmlFor="clinepass-key">API Key</label>
        <div className="secret-input">
          <KeyRound size={15} />
          <input
            id="clinepass-key"
            type="password"
            value={key}
            onChange={(event) => setKey(event.target.value)}
            placeholder={keyExists ? "已保存，输入新值可替换" : "粘贴 Cline Pass API Key（sk_…）"}
            autoComplete="new-password"
          />
        </div>
        <div className="clinepass-actions">
          <span className={`chip ${keyExists ? "chip-ok" : "chip-neutral"}`}>
            {keyExists ? "Key 已保存" : "未配置 Key"}
          </span>
          <button
            className="command-button"
            type="button"
            onClick={() => void saveKey()}
            disabled={!key.trim() || keyBusy}
          >
            {keyBusy ? <LoaderCircle size={14} className="is-spinning" /> : <Check size={14} />}
            保存 Key
          </button>
          <button
            className="command-button"
            type="button"
            onClick={() => void clearKey()}
            disabled={!keyExists || keyBusy}
          >
            <Trash2 size={14} />
            删除 Key
          </button>
          <button className="command-button" type="button" onClick={() => void refresh()} disabled={busy}>
            {busy ? <LoaderCircle size={14} className="is-spinning" /> : <RefreshCw size={14} />}
            刷新额度
          </button>
        </div>
      </div>

      {error && <div className="source-message is-error" role="alert">{error}</div>}
      {message && <div className="source-message">{message}</div>}
      {overview?.error && <div className="source-message is-error" role="status">{overview.error}</div>}

      <div className="clinepass-quota">
        <div className="clinepass-section-title">订阅额度</div>
        {(overview?.limits ?? []).length === 0 ? (
          <div className="modal-empty">
            {keyExists ? "暂无额度数据，点「刷新额度」重试。" : "保存 Key 后显示 5 小时 / 每周 / 每月额度。"}
          </div>
        ) : (
          <div className="clinepass-quota-grid">
            {overview?.limits.map((limit) => (
              <div className={`clinepass-quota-card ${limit.percentUsed >= 80 ? "is-alert" : ""}`} key={limit.kind}>
                <div>
                  <strong>{quotaLabel(limit.kind)}</strong>
                  <span>{Math.round(limit.percentUsed)}%</span>
                </div>
                <div className="clinepass-bar" aria-hidden="true">
                  <span style={{ width: `${Math.min(100, limit.percentUsed)}%` }} />
                </div>
                <small>{formatQuotaReset(limit.resetsAt) ? `${formatQuotaReset(limit.resetsAt)} 重置` : "重置时间未知"}</small>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="clinepass-models">
        <div className="clinepass-section-title">
          订阅模型
          <span className="panel-count">{(overview?.models ?? []).length} 个</span>
        </div>
        <input
          className="clinepass-search"
          aria-label="搜索 Cline Pass 模型"
          placeholder="搜索模型名或 ID，如 GLM / kimi"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className="clinepass-model-list">
          {models.map((model) => {
            const active = model.id === currentModel && currentEnabled;
            return (
              <div className={active ? "clinepass-model is-active" : "clinepass-model"} key={model.id}>
                <div>
                  <strong>{model.name}</strong>
                  <span>{modelMeta(model)}</span>
                </div>
                {model.recommended && <span className="chip chip-neutral">订阅推荐</span>}
                {model.gateway && <span className="chip chip-ok">网关可用</span>}
                <button
                  className="command-button"
                  type="button"
                  onClick={() => void applyModel(model)}
                  disabled={busy || active}
                  aria-label={`使用 ${model.name}`}
                >
                  {active ? "使用中" : "使用"}
                </button>
              </div>
            );
          })}
          {models.length === 0 && <div className="modal-empty">没有匹配的模型</div>}
        </div>
      </div>
    </div>
  );
}
