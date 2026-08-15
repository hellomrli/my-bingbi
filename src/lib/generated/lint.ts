// This file is generated from src-tauri/crates/odoc-lint. Do not edit by hand.

export type Severity = "error" | "warn" | "info";

export type Finding = { id: string, severity: Severity, nodeId: string, message: string, location: string, suggestion: string | null, };

export type LintReport = { findings: Array<Finding>, errorCount: number, warnCount: number, infoCount: number, };

export type FactCheckReport = { findings: Array<Finding>, checkedTokens: number, unresolvedTokens: number, };
