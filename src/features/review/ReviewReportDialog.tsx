import {
  AlertCircle,
  CheckCircle2,
  ClipboardCheck,
  Lightbulb,
  LoaderCircle,
  RefreshCw,
  ScanSearch,
  TriangleAlert,
  X,
} from "lucide-react";
import { useModal } from "../../hooks/useModal";
import { useMemo } from "react";
import { documentTypeLabels } from "../../lib/document";
import type {
  ReviewDimension,
  ReviewFinding,
  ReviewOutcome,
  ScoreDimension,
} from "../../lib/generated/engine";
import type { Finding, Severity } from "../../lib/generated/lint";
import {
  reviewDimensionLabels,
  reviewStageLabels,
  reviewStages,
  type ReviewStageId,
} from "../../lib/review";

const severityMeta: Record<Severity, { label: string; icon: typeof AlertCircle }> = {
  error: { label: "错误", icon: AlertCircle },
  warn: { label: "警告", icon: TriangleAlert },
  info: { label: "建议", icon: Lightbulb },
};

const scoreDimensionLabels: Record<string, string> = {
  type_structure: "文种结构",
  facts: "事实",
  execution: "可执行",
  language: "语言",
  format: "格式",
};

interface ReviewReportDialogProps {
  open: boolean;
  outcome: ReviewOutcome | null;
  documentTitle: string;
  running: boolean;
  stage: ReviewStageId | null;
  completedStages: ReviewStageId[];
  error: string | null;
  canRerun: boolean;
  onClose: () => void;
  onLocate: (nodeId: string) => void;
  onRerun: () => void;
}

function severityRank(severity: Severity): number {
  return severity === "error" ? 0 : severity === "warn" ? 1 : 2;
}

/** Groups semantic findings by inspection facet, keeping the fixed facet order. */
function groupByDimension(findings: ReviewFinding[]): [ReviewDimension, ReviewFinding[]][] {
  const order: ReviewDimension[] = ["format", "content", "language", "logic", "detail"];
  return order
    .map((dimension): [ReviewDimension, ReviewFinding[]] => [
      dimension,
      findings
        .filter((finding) => finding.dimension === dimension)
        .sort((left, right) => severityRank(left.severity) - severityRank(right.severity)),
    ])
    .filter(([, items]) => items.length > 0);
}

function RuleFindingList({
  findings,
  onLocate,
}: {
  findings: Finding[];
  onLocate: (nodeId: string) => void;
}) {
  if (findings.length === 0) {
    return (
      <div className="empty-state compact">
        <CheckCircle2 size={18} />
        <span>没有发现问题</span>
      </div>
    );
  }
  return (
    <div className="review-finding-list">
      {[...findings]
        .sort((left, right) => severityRank(left.severity) - severityRank(right.severity))
        .map((finding) => {
          const meta = severityMeta[finding.severity];
          const Icon = meta.icon;
          return (
            <button
              className={`review-finding severity-${finding.severity}`}
              type="button"
              onClick={() => onLocate(finding.nodeId)}
              key={finding.id}
            >
              <span className="review-finding-head">
                <Icon size={13} />
                <b>{meta.label}</b>
                <code>{finding.id}</code>
                <span className="review-finding-location">{finding.location}</span>
              </span>
              <span className="review-finding-message">{finding.message}</span>
              {finding.suggestion && <span className="review-finding-fix">{finding.suggestion}</span>}
            </button>
          );
        })}
    </div>
  );
}

export function ReviewReportDialog({
  open,
  outcome,
  documentTitle,
  running,
  stage,
  completedStages,
  error,
  canRerun,
  onClose,
  onLocate,
  onRerun,
}: ReviewReportDialogProps) {
  const grouped = useMemo(
    () => groupByDimension(outcome?.reviewReport.findings ?? []),
    [outcome],
  );

  const sectionRef = useModal<HTMLElement>(open, onClose);
  if (!open) return null;

  const typeMismatch = outcome !== null && outcome.declaredDocType !== outcome.typeDecision.value;
  const score = outcome?.score ?? null;
  // The kernel maps dimension names to optional entries, so drop the holes before rendering.
  const dimensions: [string, ScoreDimension][] = score
    ? Object.entries(score.dimensions).filter(
        (entry): entry is [string, ScoreDimension] => entry[1] !== undefined,
      )
    : [];

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => event.target === event.currentTarget && !running && onClose()}
    >
      <section ref={sectionRef} tabIndex={-1} className="modal review-dialog" role="dialog" aria-modal="true" aria-labelledby="review-dialog-title">
        <header className="modal-header">
          <div>
            <h2 id="review-dialog-title">公文体检报告</h2>
            <span title={documentTitle}>{documentTitle || "未命名公文"}</span>
          </div>
          <div className="modal-header-actions">
            <button
              className="icon-button"
              type="button"
              onClick={onRerun}
              disabled={running || !canRerun}
              title={canRerun ? "重新审核" : "请先在桌面版选择可用模型"}
              aria-label="重新审核"
            >
              {running ? <LoaderCircle size={16} className="is-spinning" /> : <RefreshCw size={16} />}
            </button>
            <button className="icon-button" type="button" onClick={onClose} disabled={running} title="关闭" aria-label="关闭体检报告">
              <X size={17} />
            </button>
          </div>
        </header>

        <div className="review-body">
          <div className="review-progress" aria-label="审核阶段" role="status" aria-live="polite">
            {reviewStages.map((item) => (
              <span
                className={`pipeline-stage ${
                  running && stage === item
                    ? "is-active"
                    : completedStages.includes(item)
                      ? "is-complete"
                      : ""
                }`.trim()}
                key={item}
              >
                <i />
                {reviewStageLabels[item]}
              </span>
            ))}
          </div>

          {error && <div className="draft-error" role="alert">{error}</div>}

          {!outcome && !error && (
            <div className="empty-state compact">
              {running ? <LoaderCircle size={20} className="is-spinning" /> : <ScanSearch size={20} />}
              <span>{running ? "正在体检，规则与模型双轨并行" : "尚未生成体检报告"}</span>
            </div>
          )}

          {outcome && (
            <>
              <section className="review-scoreboard">
                <div className="review-score-total">
                  <span className={score && score.total < 80 ? "score-value has-errors" : "score-value"}>
                    {score?.total ?? 0}
                  </span>
                  <span className="score-total">/ 100</span>
                  <span className="review-revision">修订 {outcome.revision}</span>
                </div>
                <div className="review-dimensions">
                  {dimensions.map(([name, dimension]) => (
                    <div className="review-dimension" key={name}>
                      <span>{scoreDimensionLabels[name] ?? name}</span>
                      <div className="review-meter" role="img" aria-label={`${dimension.score} / ${dimension.maxScore}`}>
                        <i style={{ width: `${dimension.maxScore ? (dimension.score / dimension.maxScore) * 100 : 0}%` }} />
                      </div>
                      <b>{dimension.score}/{dimension.maxScore}</b>
                    </div>
                  ))}
                </div>
              </section>

              <section className="review-block">
                <div className="panel-heading">
                  <ClipboardCheck size={14} />
                  <span>文种复核</span>
                </div>
                <div className="review-doctype">
                  <span className="chip chip-neutral">声明：{documentTypeLabels[outcome.declaredDocType]}</span>
                  <span className={typeMismatch ? "chip chip-warn" : "chip chip-ok"}>
                    判定：{documentTypeLabels[outcome.typeDecision.value]}
                  </span>
                  <span className="review-confidence">{Math.round(outcome.typeDecision.confidence * 100)}% 置信度</span>
                </div>
                {typeMismatch && (
                  <p className="review-note">
                    模型认为本文更适合按“{documentTypeLabels[outcome.typeDecision.value]}”起草，请核对文种与行文关系。
                  </p>
                )}
                {outcome.reviewReport.summary && <p className="review-summary">{outcome.reviewReport.summary}</p>}
              </section>

              <section className="review-block">
                <div className="panel-heading">
                  <ClipboardCheck size={14} />
                  <span>规则校验</span>
                  <span className="panel-count">{outcome.lintReport.findings.length} 项</span>
                </div>
                <RuleFindingList findings={outcome.lintReport.findings} onLocate={onLocate} />
              </section>

              <section className="review-block">
                <div className="panel-heading">
                  <ClipboardCheck size={14} />
                  <span>事实回查</span>
                  <span className="panel-count">
                    {outcome.factCheckReport.unresolvedTokens} / {outcome.factCheckReport.checkedTokens} 待核实
                  </span>
                </div>
                <RuleFindingList findings={outcome.factCheckReport.findings} onLocate={onLocate} />
              </section>

              <section className="review-block">
                <div className="panel-heading">
                  <ScanSearch size={14} />
                  <span>语义评审</span>
                  <span className="panel-count">{outcome.reviewReport.findings.length} 项</span>
                </div>
                {grouped.length === 0 && (
                  <div className="empty-state compact">
                    <CheckCircle2 size={18} />
                    <span>模型未提出额外意见</span>
                  </div>
                )}
                {grouped.map(([dimension, findings]) => (
                  <div className="review-dimension-group" key={dimension}>
                    <h3>{reviewDimensionLabels[dimension]}</h3>
                    <div className="review-finding-list">
                      {findings.map((finding, index) => {
                        const meta = severityMeta[finding.severity];
                        const Icon = meta.icon;
                        const locatable = finding.nodeId !== null;
                        return (
                          <button
                            className={`review-finding severity-${finding.severity}${locatable ? "" : " is-global"}`}
                            type="button"
                            onClick={() => finding.nodeId && onLocate(finding.nodeId)}
                            disabled={!locatable}
                            key={`${dimension}-${index}`}
                          >
                            <span className="review-finding-head">
                              <Icon size={13} />
                              <b>{meta.label}</b>
                              <span className="review-finding-location">{locatable ? "点击定位" : "全篇"}</span>
                            </span>
                            <span className="review-finding-message">{finding.message}</span>
                            {finding.evidence && <q className="review-finding-evidence">{finding.evidence}</q>}
                            {finding.suggestion && <span className="review-finding-fix">{finding.suggestion}</span>}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                ))}
              </section>

              {score && score.revisionInstructions.length > 0 && (
                <section className="review-block">
                  <div className="panel-heading">
                    <ClipboardCheck size={14} />
                    <span>修改建议</span>
                    <span className="panel-count">{score.revisionInstructions.length} 条</span>
                  </div>
                  <ol className="review-instructions">
                    {score.revisionInstructions.map((instruction, index) => (
                      <li key={index}>{instruction}</li>
                    ))}
                  </ol>
                </section>
              )}

              {outcome.reviewReport.missingInformation.length > 0 && (
                <section className="review-block">
                  <div className="panel-heading">
                    <TriangleAlert size={14} />
                    <span>需要补充的信息</span>
                  </div>
                  <ul className="review-instructions">
                    {outcome.reviewReport.missingInformation.map((item, index) => (
                      <li key={index}>{item}</li>
                    ))}
                  </ul>
                </section>
              )}
            </>
          )}
        </div>
      </section>
    </div>
  );
}
