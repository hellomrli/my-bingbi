import { AlertCircle, CheckCircle2, ClipboardList, Lightbulb, Plus, Trash2, TriangleAlert } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import type { FactKind, FactSource } from "../../lib/odoc";
import type { Finding, Severity } from "../../lib/lint";
import { useWorkbenchStore, type FindingFilter } from "../../store/workbench";

const severityMeta: Record<Severity, { label: string; icon: typeof AlertCircle }> = {
  error: { label: "错误", icon: AlertCircle },
  warn: { label: "警告", icon: TriangleAlert },
  info: { label: "建议", icon: Lightbulb },
};

const sourceLabels: Record<FactSource, string> = {
  user_input: "用户提供",
  kb_retrieved: "库检索",
  model_generated: "模型生成",
};

const factKindLabels: Record<FactKind, string> = {
  subject: "主体",
  object: "客体",
  action: "动作",
  data: "数据",
  time: "时间",
  place: "地点",
  problem: "问题",
  basis: "依据",
  responsibility: "责任",
  feedback: "反馈",
  other: "其他",
};

interface LintPanelProps {
  findings: Finding[];
}

function FactComposer() {
  const addFact = useWorkbenchStore((state) => state.addFact);
  const [value, setValue] = useState("");
  const [unit, setUnit] = useState("");
  const [kind, setKind] = useState<FactKind>("data");

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!value.trim()) return;
    addFact({ kind, value: value.trim(), unit: unit.trim() || null, source: "user_input" });
    setValue("");
    setUnit("");
  }

  return (
    <form className="fact-composer" onSubmit={submit}>
      <select aria-label="事实类型" value={kind} onChange={(event) => setKind(event.target.value as FactKind)}>
        {Object.entries(factKindLabels).map(([factKind, label]) => (
          <option value={factKind} key={factKind}>
            {label}
          </option>
        ))}
      </select>
      <input aria-label="事实值" placeholder="事实值" value={value} onChange={(event) => setValue(event.target.value)} />
      <input className="fact-unit-input" aria-label="事实单位" placeholder="单位" value={unit} onChange={(event) => setUnit(event.target.value)} />
      <button className="icon-button small" type="submit" title="加入事实台账" aria-label="加入事实台账">
        <Plus size={14} />
      </button>
    </form>
  );
}

export function LintPanel({ findings }: LintPanelProps) {
  const odoc = useWorkbenchStore((state) => state.document);
  const filter = useWorkbenchStore((state) => state.findingFilter);
  const setFilter = useWorkbenchStore((state) => state.setFindingFilter);
  const setView = useWorkbenchStore((state) => state.setView);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const removeFact = useWorkbenchStore((state) => state.removeFact);
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

  function locate(finding: Finding) {
    setView("structure");
    selectNode(finding.nodeId);
    requestAnimationFrame(() => window.document.querySelector<HTMLElement>(`[data-node-id="${finding.nodeId}"]`)?.scrollIntoView({ block: "center" }));
  }

  return (
    <aside className="right-inspector" aria-label="质检与事实台账">
      <section className="score-summary">
        <div>
          <span className={counts.error ? "score-value has-errors" : "score-value"}>{score}</span>
          <span className="score-total">/ 100</span>
        </div>
        <span className={`chip ${counts.error ? "chip-error" : counts.warn ? "chip-warn" : "chip-ok"}`}>
          {counts.error ? "导出阻断" : counts.warn ? "待修订" : "规则通过"}
        </span>
      </section>

      <section className="findings-section">
        <div className="panel-heading">
          <ClipboardList size={14} />
          <span>确定性质检</span>
          <span className="panel-count">{findings.length} 项</span>
        </div>
        <div className="finding-filters" aria-label="质检筛选">
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
        <div className="finding-list">
          {filtered.length === 0 && (
            <div className="empty-state compact">
              <CheckCircle2 size={20} />
              <span>当前筛选下没有问题</span>
            </div>
          )}
          {filtered.map((finding) => {
            const meta = severityMeta[finding.severity];
            const Icon = meta.icon;
            return (
              <button className={`finding-item severity-${finding.severity}`} type="button" onClick={() => locate(finding)} key={finding.id}>
                <span className="finding-title">
                  <Icon size={14} />
                  <b>{meta.label}</b>
                  <code>{finding.id}</code>
                </span>
                <span className="finding-message">{finding.message}</span>
                <span className="finding-location">{finding.location}</span>
                {finding.suggestion && <span className="finding-suggestion">{finding.suggestion}</span>}
              </button>
            );
          })}
        </div>
      </section>

      <section className="facts-section">
        <div className="panel-heading">
          <CheckCircle2 size={14} />
          <span>事实台账</span>
          <span className="panel-count">{odoc.facts.facts.length} 项</span>
        </div>
        <FactComposer />
        <div className="fact-list">
          {odoc.facts.facts.map((fact) => (
            <div className="fact-row" key={fact.id}>
              <span className="fact-kind">{factKindLabels[fact.kind]}</span>
              <strong>
                {fact.value}
                {fact.unit ? ` ${fact.unit}` : ""}
              </strong>
              <span className={`fact-source source-${fact.source}`}>{sourceLabels[fact.source]}</span>
              <button className="icon-button tiny danger-action" type="button" onClick={() => removeFact(fact.id)} title="移除事实">
                <Trash2 size={12} />
              </button>
            </div>
          ))}
          {odoc.facts.facts.length === 0 && <span className="facts-empty">尚未录入可信事实</span>}
        </div>
      </section>
    </aside>
  );
}
