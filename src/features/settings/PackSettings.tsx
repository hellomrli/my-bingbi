import { Box, Check, FolderOpen, LoaderCircle, LockKeyhole } from "lucide-react";
import { useEffect, useState } from "react";
import { ipcErrorMessage, listPackSummaries, packLoadIssues, userPackRoot } from "../../lib/ipc";
import type { PackCapability, PackSummary } from "../../lib/pack";

interface PackSettingsProps {
  activePackIds: string[];
  onSave: (activePackIds: string[]) => Promise<void>;
}

/** Core packs the drafting pipeline, lint and export stages inject resources from. */
const corePackIds = new Set(["gb9704-format", "renmin-style"]);

const capabilityLabels: Record<PackCapability, string> = {
  format_rules: "格式规则",
  language_rules: "语言规则",
  templates: "文种模板",
  references: "参考资料",
  checklists: "检查清单",
  style: "版式样式",
  tests: "回归用例",
};

export function PackSettings({ activePackIds, onSave }: PackSettingsProps) {
  const [packs, setPacks] = useState<PackSummary[]>([]);
  const [draft, setDraft] = useState<string[]>(activePackIds);
  const [busy, setBusy] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [userRoot, setUserRoot] = useState<string | null>(null);
  const [loadIssues, setLoadIssues] = useState<string[]>([]);

  useEffect(() => {
    listPackSummaries()
      .then(setPacks)
      .catch((loadError) => setError(ipcErrorMessage(loadError)))
      .finally(() => setBusy(false));
    userPackRoot().then(setUserRoot).catch(() => setUserRoot(null));
    packLoadIssues().then(setLoadIssues).catch(() => setLoadIssues([]));
  }, []);

  function togglePack(packId: string) {
    const enabled = draft.includes(packId);
    if (enabled) {
      const switchable = draft.filter((id) => !corePackIds.has(id));
      if (switchable.length <= 1) {
        setMessage("至少需要启用一个可切换知识包。");
        return;
      }
      setDraft((current) => current.filter((id) => id !== packId));
    } else {
      setDraft((current) => [...current, packId]);
    }
    setMessage(null);
    setError(null);
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      await onSave(draft);
      setMessage("知识包设置已保存");
    } catch (saveError) {
      setError(ipcErrorMessage(saveError));
    } finally {
      setSaving(false);
    }
  }

  const enabledCount = packs.filter((pack) => draft.includes(pack.id)).length;

  return (
    <form className="settings-form" onSubmit={submit}>
      <div className="settings-group">
        <label>已安装知识包</label>
        <span className="settings-hint">
          已启用 {enabledCount} / {packs.length} 个包。核心包由起草、校验与导出流程注入资源，不可停用。
        </span>
        <div className="pack-list">
          {busy && (
            <div className="pack-empty">
              <LoaderCircle size={18} className="is-spinning" />
              正在加载知识包
            </div>
          )}
          {error && <div className="source-message is-error" role="alert">{error}</div>}
          {!busy && !error && packs.length === 0 && (
            <div className="pack-empty">
              <Box size={18} />
              没有检测到知识包
            </div>
          )}
          {packs.map((pack) => {
            const enabled = draft.includes(pack.id);
            const core = corePackIds.has(pack.id);
            return (
              <div className={enabled ? "pack-card is-enabled" : "pack-card"} key={pack.id}>
                <label className="pack-card-main">
                  <input
                    type="checkbox"
                    checked={enabled}
                    disabled={core}
                    onChange={() => togglePack(pack.id)}
                  />
                  <span className="pack-card-text">
                    <strong>
                      {pack.name}
                      {core && <LockKeyhole size={12} aria-label="内置核心包" />}
                    </strong>
                    <span className="pack-card-meta">v{pack.version} · {pack.docTypes.length} 个文种模板</span>
                    {pack.description && <span className="pack-card-description">{pack.description}</span>}
                    <span className="pack-card-capabilities">
                      {pack.capabilities.map((capability) => (
                        <span className="chip chip-neutral" key={capability}>{capabilityLabels[capability] ?? capability}</span>
                      ))}
                    </span>
                  </span>
                  <span className={enabled ? "chip chip-ok" : "chip chip-neutral"}>
                    {enabled ? <Check size={11} /> : null}
                    {enabled ? "已启用" : "已停用"}
                  </span>
                </label>
              </div>
            );
          })}
        </div>
      </div>
      {message && <div className="source-message" role="status">{message}</div>}
      <div className="settings-group">
        <label>用户包目录</label>
        {userRoot ? (
          <>
            <span className="settings-hint">
              <FolderOpen size={12} />
              把自建知识包（含 pack.toml 的目录）放到这里，重启应用后生效；首个空目录已自动放入
              local-corpus 示例包，直接编辑其 rules/ 即可添加本单位规则。
            </span>
            <code className="pack-root-path">{userRoot}</code>
          </>
        ) : (
          <span className="settings-hint">浏览器预览没有用户包目录。</span>
        )}
        {loadIssues.length > 0 && (
          <div className="pack-issues" role="alert">
            {loadIssues.map((issue, index) => (
              <div key={index}>用户包加载失败：{issue}</div>
            ))}
          </div>
        )}
      </div>
      <button className="command-button primary" type="submit" disabled={saving}>
        {saving ? <LoaderCircle size={15} className="is-spinning" /> : <Check size={15} />}
        保存知识包设置
      </button>
    </form>
  );
}
