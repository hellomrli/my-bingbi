import { Check, LoaderCircle, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useModal } from "../../hooks/useModal";
import { chatWithProvider, ipcErrorMessage } from "../../lib/ipc";
import type { ODoc } from "../../lib/odoc";
import {
  REWRITE_PRESETS,
  buildRewriteMessages,
  cleanRewriteOutput,
  diffFactTokens,
} from "../../lib/rewrite";

interface BlockRewriteDialogProps {
  open: boolean;
  blockId: string;
  original: string;
  document: ODoc;
  providerId: string;
  providerName: string;
  onApply: (blockId: string, text: string) => void;
  onClose: () => void;
}

/**
 * Rewrites one paragraph with the selected provider and shows the result before it lands in the
 * document. Numbers, dates and document numbers are compared with the original; an unexplained
 * change must be confirmed by the user.
 */
export function BlockRewriteDialog({
  open,
  blockId,
  original,
  document,
  providerId,
  providerName,
  onApply,
  onClose,
}: BlockRewriteDialogProps) {
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const [presetId, setPresetId] = useState(REWRITE_PRESETS[0].id);
  const [extra, setExtra] = useState("");
  const [result, setResult] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);

  useEffect(() => {
    if (!open) return;
    setPresetId(REWRITE_PRESETS[0].id);
    setExtra("");
    setResult("");
    setError(null);
    setConfirmed(false);
  }, [open, blockId]);

  const preset = REWRITE_PRESETS.find((candidate) => candidate.id === presetId) ?? REWRITE_PRESETS[0];
  const diff = useMemo(() => diffFactTokens(original, result), [original, result]);
  const hasTokenChanges = diff.added.length > 0 || diff.removed.length > 0;
  const canApply = result.trim() !== "" && (!hasTokenChanges || confirmed) && !busy;

  if (!open) return null;

  async function generate() {
    setBusy(true);
    setError(null);
    setConfirmed(false);
    try {
      const instruction = extra.trim() ? `${preset.instruction} 补充要求：${extra.trim()}` : preset.instruction;
      const { system, user } = buildRewriteMessages({
        docType: document.meta.docType,
        instruction,
        text: original,
        facts: document.facts.facts,
      });
      const response = await chatWithProvider(providerId, document.id, {
        messages: [
          { role: "system", content: system },
          { role: "user", content: user },
        ],
        temperature: 0.2,
        maxTokens: 1200,
        jsonMode: false,
        securityLevel: document.meta.securityLevel,
      });
      setResult(cleanRewriteOutput(response.content));
    } catch (rewriteError) {
      setError(ipcErrorMessage(rewriteError));
    } finally {
      setBusy(false);
    }
  }

  function apply() {
    if (!canApply) return;
    onApply(blockId, result.trim());
    onClose();
  }

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => event.target === event.currentTarget && !busy && onClose()}
    >
      <section
        ref={sectionRef}
        tabIndex={-1}
        className="modal rewrite-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rewrite-dialog-title"
      >
        <header className="modal-header">
          <div>
            <h2 id="rewrite-dialog-title">
              <Sparkles size={16} aria-hidden="true" /> AI 改写本段
            </h2>
            <span>{providerName ? `使用 ${providerName}` : "选择一个模型服务"}</span>
          </div>
          <button className="icon-button" type="button" onClick={onClose} title="关闭" aria-label="关闭改写">
            <X size={17} />
          </button>
        </header>

        <div className="rewrite-body">
          <div className="rewrite-controls">
            <label className="wizard-field" htmlFor="rewrite-preset">
              <span>改写目标</span>
              <select id="rewrite-preset" value={presetId} onChange={(event) => setPresetId(event.target.value)}>
                {REWRITE_PRESETS.map((candidate) => (
                  <option value={candidate.id} key={candidate.id}>
                    {candidate.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="wizard-field" htmlFor="rewrite-extra">
              <span>补充要求（可选）</span>
              <input
                id="rewrite-extra"
                value={extra}
                onChange={(event) => setExtra(event.target.value)}
                placeholder="例如：改为对下级单位提要求；不超过 120 字"
              />
            </label>
          </div>

          <div className="rewrite-compare">
            <div>
              <span className="rewrite-label">原文</span>
              <pre className="rewrite-preview" aria-label="原文">{original || "（空段落）"}</pre>
            </div>
            <div>
              <span className="rewrite-label">改写结果</span>
              <textarea
                className="rewrite-result"
                aria-label="改写结果"
                value={result}
                onChange={(event) => {
                  setResult(event.target.value);
                  setConfirmed(false);
                }}
                placeholder="点「生成改写」后在这里预览和微调"
                rows={7}
              />
            </div>
          </div>

          {hasTokenChanges && (
            <div className="rewrite-warning" role="alert">
              <strong>数字、日期或文号发生了变化</strong>
              <span>
                {diff.added.length > 0 && `新增：${diff.added.join("、")}。`}
                {diff.removed.length > 0 && `缺失：${diff.removed.join("、")}。`}
              </span>
              <label className="rewrite-confirm">
                <input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />
                我已核对上述变化，确认应用
              </label>
            </div>
          )}

          {error && <div className="source-message is-error" role="alert">{error}</div>}
        </div>

        <footer className="modal-footer">
          <button className="command-button" type="button" onClick={() => void generate()} disabled={busy || !providerId}>
            {busy ? <LoaderCircle size={14} className="is-spinning" /> : <Sparkles size={14} />}
            {result ? "重新生成" : "生成改写"}
          </button>
          <button className="command-button" type="button" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button className="command-button primary" type="button" onClick={apply} disabled={!canApply}>
            <Check size={14} />
            应用到本段
          </button>
        </footer>
      </section>
    </div>
  );
}
