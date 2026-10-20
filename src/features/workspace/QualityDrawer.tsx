import { AlertCircle, CheckCircle2, ClipboardCheck, Lightbulb, LoaderCircle, ScanSearch, TriangleAlert, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useModal } from "../../hooks/useModal";
import type { Finding, Severity } from "../../lib/lint";
import type { FindingFilter } from "../../store/workbench";
import { FactLedger } from "../lint-panel/FactLedger";

const severityMeta: Record<Severity, { label: string; icon: typeof AlertCircle }> = {
  error: { label: "错误", icon: AlertCircle },
  warn: { label: "警告", icon: TriangleAlert },
  info: { label: "建议", icon: Lightbulb },
};

interface QualityDrawerProps {
  open: boolean;
  findings: Finding[];
  canReview: boolean;
  reviewing: boolean;
  onClose: () => void;
  onLocate: (finding: Finding) => void;
  onReview: () => void;
  checking: boolean;
  checkError: string | null;
  onRecheck: () => void;
}

export function QualityDrawer({
  open,
  findings,
  canReview,
  reviewing,
  onClose,
  onLocate,
  onReview,
  checking,
  checkError,
  onRecheck,
}: QualityDrawerProps) {
  const [filter, setFilter] = useState<FindingFilter>("all");
  const sectionRef = useModal<HTMLElement>(open, onClose);
  const filtered = filter === "all" ? findings : findings.filter((finding) => finding.severity === filter);
  const counts = useMemo(
    () => ({
      all: findings.length,
      error: findings.filter((finding) => finding.severity === "error").length,
      warn: findings.filter((finding) => finding.severity === "warn").length,
      info: findings.filter((finding) => finding.severity === "info").length,
    }),
    [findings],
  );
  const score = Math.max(0, 100 - counts.error * 18 - counts.warn * 7 - counts.info * 2);

  if (!open) return null;

  return (
    <div className="quality-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <aside
        ref={sectionRef}
        className="quality-drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="quality-title"
        tabIndex={-1}
      >
        <header className="quality-header">
          <div>
            <span className="quality-eyebrow"><ClipboardCheck size={13} /> 导出前检查</span>
            <h2 id="quality-title">质检结果</h2>
          </div>
          <button className="icon-button" type="button" onClick={onClose} aria-label="关闭质检">
            <X size={16} />
          </button>
        </header>

        <div className="quality-score">
          <div className="quality-score-value">
            <strong className={counts.error ? "has-errors" : ""}>{score}</strong>
            <span>/ 100</span>
          </div>
          <span className={`chip ${counts.error ? "chip-error" : counts.warn ? "chip-warn" : "chip-ok"}`}>
            {counts.error ? "存在错误，暂不能导出" : counts.warn ? "建议再改改" : "规则通过"}
          </span>
        </div>

        <div className="quality-filters" aria-label="质检筛选">
          {(["all", "error", "warn", "info"] as FindingFilter[]).map((item) => (
            <button
              className={filter === item ? "filter-button is-active" : "filter-button"}
              type="button"
              aria-pressed={filter === item}
              onClick={() => setFilter(item)}
              key={item}
            >
              {item === "all" ? "全部" : severityMeta[item].label}
              <span>{counts[item]}</span>
            </button>
          ))}
        </div>

        <div className="quality-scroll">
        {checkError && <p className="composer-message is-error" role="alert">{checkError}</p>}
        <button className="command-button quality-recheck" type="button" onClick={onRecheck} disabled={checking}>
          {checking && <LoaderCircle size={14} className="is-spinning" />}
          {checking ? "正在核验…" : "重新核验规则与事实"}
        </button>
        <div className="quality-list">
          {filtered.length === 0 && (
            <div className="quality-empty">
              <CheckCircle2 size={22} />
              <strong>没有问题</strong>
              <span>当前筛选下没有需要处理的项。</span>
            </div>
          )}
          {filtered.map((finding) => {
            const meta = severityMeta[finding.severity];
            const Icon = meta.icon;
            return (
              <button className={`quality-item severity-${finding.severity}`} type="button" onClick={() => { onLocate(finding); onClose(); }} key={finding.id}>
                <span className="quality-item-title">
                  <Icon size={14} />
                  <b>{meta.label}</b>
                  <code>{finding.id}</code>
                </span>
                <span className="quality-item-message">{finding.message}</span>
                <span className="quality-item-location">{finding.location}</span>
                {finding.suggestion && <span className="quality-item-suggestion">{finding.suggestion}</span>}
              </button>
            );
          })}
        </div>

        <div className="quality-facts">
          <FactLedger />
        </div>
        </div>

        <footer className="quality-footer">
          <button className="command-button" type="button" onClick={onReview} disabled={!canReview || reviewing}>
            {reviewing ? <LoaderCircle size={14} className="is-spinning" /> : <ScanSearch size={14} />}
            {reviewing ? "正在审核" : "运行语义审核"}
          </button>
          <button className="command-button primary" type="button" onClick={onClose}>
            返回正文
          </button>
        </footer>
      </aside>
    </div>
  );
}
