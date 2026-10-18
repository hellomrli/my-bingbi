import type { FactCheckReport, Finding as EngineFinding, LintReport } from "./generated/lint";
import type { Finding } from "./lint";

/** A kernel report together with the document identity it describes. */
export interface RevisionedReport<T> {
  documentId: string;
  revision: number;
  report: T;
}

function adapt(finding: EngineFinding): Finding {
  return {
    id: finding.id,
    severity: finding.severity,
    nodeId: finding.nodeId,
    message: finding.message,
    location: finding.location,
    ...(finding.suggestion ? { suggestion: finding.suggestion } : {}),
    ...(finding.token ? { token: finding.token } : {}),
  };
}

/**
 * Picks the findings the panel shows.
 *
 * The kernel is the only source that understands user packs and custom rules, so a report for
 * the current document revision wins over the browser-local rule subset. A stale report is
 * ignored on its own: if the fact check still describes an older revision while the lint report
 * is current, the panel shows just the lint findings rather than mixing two document states.
 */
export function selectFindings(
  documentId: string,
  documentRevision: number,
  localFindings: Finding[],
  lint: RevisionedReport<LintReport> | null,
  factCheck: RevisionedReport<FactCheckReport> | null,
): Finding[] {
  const matches = (candidate: { documentId: string; revision: number } | null): boolean =>
    candidate !== null && candidate.documentId === documentId && candidate.revision === documentRevision;
  const lintFindings = lint !== null && matches(lint) ? lint.report.findings : null;
  const factFindings = factCheck !== null && matches(factCheck) ? factCheck.report.findings : null;
  if (!lintFindings && !factFindings) return localFindings;
  return [...(lintFindings ?? []), ...(factFindings ?? [])].map(adapt);
}
