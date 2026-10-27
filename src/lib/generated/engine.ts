// This file is generated from src-tauri/crates/odoc-engine. Do not edit by hand.

import type { DocumentType, FactKind, ODoc } from "./bindings";
import type { TokenUsage } from "./llm";
import type { StageId } from "./pack";
import type { FactCheckReport, LintReport, Severity } from "./lint";

export type StageKind = "llm" | "pack" | "draft";

export type StageStatus = "running" | "completed" | "failed" | "cancelled";

export type PipelineKind = "draft" | "review" | "imitate" | "polish";

export type LowConfidenceAction = "ask_user";

export type StageConfig = { id: StageId, kind: StageKind, prompt: string | null, inject: Array<string>, outputSchema: string | null, jsonMode: boolean, stream: boolean, onLowConfidence: LowConfidenceAction | null, loopBack: LoopBackConfig | null, };

export type LoopBackConfig = { whenBelow: number, to: StageId, maxIterations: number, };

export type TypeDecision = { value: DocumentType, confidence: number, alternatives: Array<DocumentType>, missingInformation: Array<string>, };

export type FactInput = { kind: FactKind, value: string, unit: string | null, role: string | null, };

export type FactExtraction = { facts: Array<FactInput>, missingInformation: Array<string>, recipient?: RecipientInference, };

export type RecipientInference = { recipients: Array<string>, evidence: string | null, question: string | null, };

export type StageFailure = { stageId: StageId, code: string, message: string, };

export type ScoreDimension = { score: number, maxScore: number, findings: Array<string>, };

export type Score = { total: number, dimensions: { [key in string]?: ScoreDimension }, revisionInstructions: Array<string>, missingInformation: Array<string>, };

export type StageOutput = { stageId: StageId, status: StageStatus, content: string, json: string | null, usage: TokenUsage | null, durationMs: number, };

export type StageSummary = { stageId: StageId, status: StageStatus, usage: TokenUsage | null, durationMs: number, };

export type RerunOutcome = { sessionId: string, stageId: StageId, document: ODoc, output: StageOutput, lintReport: LintReport | null, factCheckReport: FactCheckReport | null, };

export type DraftInput = { material: string, 
/**
 * The reference text an imitate pipeline copies the writing style from. Must be present
 * for `PipelineKind::Imitate` and absent for any other kind.
 */
modelText: string | null, initialDocument: ODoc | null, confirmedDocType: DocumentType | null, 
/**
 * Knowledge-pack template variant (e.g. `notice-meeting`); empty means the per-type default.
 */
templateId: string | null, };

export type DraftOutcome = { sessionId: string, document: ODoc, typeDecision: TypeDecision, facts: FactExtraction, stages: Array<StageSummary>, lintReport: LintReport | null, factCheckReport: FactCheckReport | null, score: Score | null, revisionIterations: number, warnings: Array<StageFailure>, };

export type DraftEvent = { "type": "session_started", sessionId: string, } | { "type": "stage_started", stageId: StageId, } | { "type": "token", stageId: StageId, content: string, } | { "type": "needs_document_type", decision: TypeDecision, } | { "type": "stage_completed", output: StageOutput, } | { "type": "stage_failed", output: StageOutput, recoverable: boolean, } | { "type": "completed", outcome: DraftOutcome, };

export type ReviewDimension = "format" | "content" | "language" | "logic" | "detail";

export type ReviewFinding = { dimension: ReviewDimension, severity: Severity, 
/**
 * `ODoc` node the finding points at, or `None` when it concerns the document as a whole.
 * The engine clears identifiers that are absent from the reviewed document.
 */
nodeId: string | null, message: string, 
/**
 * Text quoted from the document, kept so the reader can judge the finding without hunting.
 */
evidence: string | null, suggestion: string | null, };

export type ReviewReport = { findings: Array<ReviewFinding>, summary: string, missingInformation: Array<string>, };

export type ReviewInput = { document: ODoc, 
/**
 * Optional reviewer focus, e.g. 只看格式. Empty means a full inspection.
 */
focus: string | null, };

export type ReviewOutcome = { sessionId: string, documentId: string, revision: number, 
/**
 * Type declared on the document under review.
 */
declaredDocType: DocumentType, 
/**
 * Type the model considers correct for this content.
 */
typeDecision: TypeDecision, lintReport: LintReport, factCheckReport: FactCheckReport, reviewReport: ReviewReport, score: Score, stages: Array<StageSummary>, };

export type ReviewEvent = { "type": "stage_started", stageId: StageId, } | { "type": "stage_completed", output: StageOutput, } | { "type": "completed", outcome: ReviewOutcome, };
