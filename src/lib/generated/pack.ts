// This file is generated from src-tauri/crates/odoc-pack. Do not edit by hand.

import type { DocumentType } from "./bindings";

export type PackCapability = "format_rules" | "language_rules" | "templates" | "references" | "checklists" | "style" | "tests";

export type StageId = "doctype" | "facts" | "retrieve" | "style" | "outline" | "draft" | "deai" | "lint" | "correct" | "factcheck" | "review" | "score" | "export";

export type PackSummary = { schemaVersion: string, id: string, name: string, version: string, description: string, license: string, source: string, docTypes: Array<DocumentType>, capabilities: Array<PackCapability>, };

export type TemplateSummary = { id: string, name: string, docType: DocumentType, scenario: string | null, description: string | null, isDefault: boolean, };
