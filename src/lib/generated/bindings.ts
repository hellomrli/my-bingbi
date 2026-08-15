// This file is generated from src-tauri/crates/odoc-model. Do not edit by hand.

export type DocumentType = "request" | "notice" | "report" | "letter" | "minutes" | "summary" | "decision" | "announcement" | "regulation" | "proclamation" | "reply" | "instruction" | "proposal" | "work_plan" | "work_report" | "briefing" | "speech" | "presentation" | "memo" | "invitation" | "certificate" | "other";

export type SecurityLevel = "public" | "internal" | "secret" | "confidential";

export type Urgency = "normal" | "urgent" | "extra_urgent";

export type DocumentStatus = "draft" | "review" | "final";

export type DocMeta = { docType: DocumentType, customDocType: string | null, securityLevel: SecurityLevel, urgency: Urgency, copyNumber: string | null, status: DocumentStatus, };

export type Header = { id: string, issuingAuthorityMark: string, documentNumber: string | null, signer: string | null, };

export type Title = { id: string, text: string, };

export type HeadingLevel = "l1" | "l2" | "l3" | "l4";

export type ListStyle = "ordered" | "unordered";

export type ListItem = { id: string, content: string, };

export type TableCell = { id: string, content: string, };

export type TableRow = { id: string, cells: Array<TableCell>, };

export type Block = { "type": "heading", id: string, level: HeadingLevel, content: string, } | { "type": "paragraph", id: string, content: string, } | { "type": "list", id: string, style: ListStyle, items: Array<ListItem>, } | { "type": "table", id: string, rows: Array<TableRow>, } | { "type": "quote", id: string, content: string, };

export type Attachment = { id: string, order: number, title: string, note: string | null, };

export type Signer = { id: string, authority: string, representative: string | null, };

export type OfficialDate = { id: string, value: string, };

export type Body = { id: string, title: Title, mainRecipients: Array<string>, blocks: Array<Block>, attachments: Array<Attachment>, signer: Signer | null, date: OfficialDate | null, note: string | null, };

export type Footer = { id: string, copyRecipients: Array<string>, printingAuthority: string | null, printingDate: string | null, };

export type FactKind = "subject" | "object" | "action" | "data" | "time" | "place" | "problem" | "basis" | "responsibility" | "feedback" | "other";

export type FactSource = "user_input" | "kb_retrieved" | "model_generated";

export type Fact = { id: string, kind: FactKind, value: string, unit: string | null, role: string | null, source: FactSource, sourceRef: string | null, verified: boolean, };

export type FactLedger = { facts: Array<Fact>, };

export type ProvenanceSource = "user" | "knowledge_base" | "model" | "imported";

export type Citation = { id: string, nodeId: string, source: ProvenanceSource, sourceRef: string | null, excerpt: string | null, };

export type ODoc = { schemaVersion: string, id: string, revision: number, meta: DocMeta, header: Header | null, body: Body, footer: Footer | null, facts: FactLedger, provenance: Array<Citation>, };
