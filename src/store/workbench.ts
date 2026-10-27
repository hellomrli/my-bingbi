import { create } from "zustand";
import { persist } from "zustand/middleware";
import { createDemoDocument } from "../lib/document";
import { createId } from "../lib/id";
import type {
  DocumentType,
  Fact,
  Header,
  HeadingLevel,
  ODoc,
  OfficialDate,
  SecurityLevel,
  Signer,
} from "../lib/odoc";
import type { Severity } from "../lib/lint";
import { isDesktopRuntime } from "../lib/ipc";

export type WorkbenchView = "preview" | "structure";
export type FindingFilter = "all" | Severity;
export type Theme = "system" | "light" | "dark";

interface WorkbenchState {
  document: ODoc;
  view: WorkbenchView;
  findingFilter: FindingFilter;
  showGrid: boolean;
  theme: Theme;
  selectedNodeId: string | null;
  dirty: boolean;
  savedAt: number | null;
  /** Id of the last document the user had open; the desktop runtime restores it on launch. */
  lastDocumentId: string | null;
  /** nodeId → unverified fact tokens from the latest kernel fact-check report. */
  unverifiedTokens: Record<string, string[]>;
  setDocument: (document: ODoc, savedAt?: number | null) => void;
  setView: (view: WorkbenchView) => void;
  setFindingFilter: (filter: FindingFilter) => void;
  toggleGrid: () => void;
  setTheme: (theme: Theme) => void;
  selectNode: (nodeId: string | null) => void;
  markSaved: (savedAt?: number) => void;
  setUnverifiedTokens: (tokens: Record<string, string[]>) => void;
  updateDocumentType: (docType: DocumentType) => void;
  updateSecurityLevel: (securityLevel: SecurityLevel) => void;
  updateHeader: (field: keyof Omit<Header, "id">, value: string) => void;
  updateTitle: (text: string) => void;
  updateRecipient: (value: string) => void;
  updateRecipients: (values: string[]) => void;
  updateSigner: (authority: string) => void;
  updateDate: (value: string) => void;
  /** Replaces the text of a heading/paragraph/quote; list and table blocks are left untouched. */
  updateBlockText: (id: string, content: string) => void;
  /** Changes a heading level; a no-op for non-heading blocks. */
  updateHeadingLevel: (id: string, level: HeadingLevel) => void;
  addBlock: (kind: "paragraph" | "heading") => void;
  removeBlock: (id: string) => void;
  addFact: (input: Pick<Fact, "kind" | "value" | "unit" | "source"> & Partial<Pick<Fact, "sourceRef" | "verified">>) => void;
  updateFact: (id: string, patch: Partial<Pick<Fact, "kind" | "value" | "unit" | "sourceRef" | "verified">>) => void;
  removeFact: (id: string) => void;
}

function changed(document: ODoc, update: (draft: ODoc) => void): ODoc {
  const next = structuredClone(document);
  update(next);
  next.revision += 1;
  return next;
}

function ensureHeader(document: ODoc): Header {
  document.header ??= {
    id: createId("header"),
    issuingAuthorityMark: "",
    documentNumber: null,
    signer: null,
  };
  return document.header;
}

function nullable(value: string): string | null {
  return value.trim() ? value : null;
}

/** Shape guard for a persisted document: a stale or corrupt draft must not crash rendering. */
function isValidDocument(value: unknown): value is ODoc {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ODoc>;
  return (
    candidate.schemaVersion === "0.1.0" &&
    typeof candidate.id === "string" &&
    typeof candidate.revision === "number" &&
    Array.isArray(candidate.body?.blocks)
  );
}

export const useWorkbenchStore = create<WorkbenchState>()(
  persist(
    (set) => ({
      document: createDemoDocument(),
      view: "preview",
      findingFilter: "all",
      showGrid: false,
      theme: "system",
      selectedNodeId: null,
      dirty: false,
      savedAt: null,
      lastDocumentId: null,
      unverifiedTokens: {},
      setDocument: (document, savedAt = Date.now()) =>
        set({ document, dirty: false, selectedNodeId: null, savedAt, lastDocumentId: document.id, unverifiedTokens: {} }),
      setView: (view) => set({ view }),
      setFindingFilter: (findingFilter) => set({ findingFilter }),
      toggleGrid: () => set((state) => ({ showGrid: !state.showGrid })),
      setTheme: (theme) => set({ theme }),
      selectNode: (selectedNodeId) => set({ selectedNodeId }),
      markSaved: (savedAt = Date.now()) => set({ dirty: false, savedAt }),
      setUnverifiedTokens: (unverifiedTokens) => set({ unverifiedTokens }),
      updateDocumentType: (docType) =>
        set((state) => ({ document: changed(state.document, (document) => (document.meta.docType = docType)), dirty: true })),
      updateSecurityLevel: (securityLevel) =>
        set((state) => ({
          document: changed(state.document, (document) => (document.meta.securityLevel = securityLevel)),
          dirty: true,
        })),
      updateHeader: (field, value) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            const header = ensureHeader(document);
            if (field === "issuingAuthorityMark") header[field] = value;
            else header[field] = nullable(value);
          }),
          dirty: true,
        })),
      updateTitle: (text) =>
        set((state) => ({
          document: changed(state.document, (document) => (document.body.title.text = text)),
          dirty: true,
        })),
      updateRecipient: (value) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.body.mainRecipients = value.trim() ? [value] : [];
          }),
          dirty: true,
        })),
      updateRecipients: (values) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.body.mainRecipients = [...new Set(values.map((value) => value.trim()).filter(Boolean))];
          }),
          dirty: true,
        })),
      updateSigner: (authority) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.body.signer = authority.trim()
              ? ({ id: document.body.signer?.id ?? createId("signer"), authority, representative: null } satisfies Signer)
              : null;
          }),
          dirty: true,
        })),
      updateDate: (value) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.body.date = value.trim()
              ? ({ id: document.body.date?.id ?? createId("date"), value } satisfies OfficialDate)
              : null;
          }),
          dirty: true,
        })),
      updateBlockText: (id, content) =>
        set((state) => {
          const block = state.document.body.blocks.find((candidate) => candidate.id === id);
          if (!block || (block.type !== "heading" && block.type !== "paragraph" && block.type !== "quote")) return {};
          return {
            document: changed(state.document, (document) => {
              const target = document.body.blocks.find((candidate) => candidate.id === id);
              if (target && (target.type === "heading" || target.type === "paragraph" || target.type === "quote")) {
                target.content = content;
              }
            }),
            dirty: true,
          };
        }),
      updateHeadingLevel: (id, level) =>
        set((state) => {
          const block = state.document.body.blocks.find((candidate) => candidate.id === id);
          if (block?.type !== "heading") return {};
          return {
            document: changed(state.document, (document) => {
              const target = document.body.blocks.find((candidate) => candidate.id === id);
              if (target?.type === "heading") target.level = level;
            }),
            dirty: true,
          };
        }),
      addBlock: (kind) =>
        set((state) => {
          const id = createId("block");
          return {
            document: changed(state.document, (document) => {
              document.body.blocks.push(
                kind === "heading"
                  ? { type: "heading", id, level: "l1", content: "" }
                  : { type: "paragraph", id, content: "" },
              );
            }),
            dirty: true,
            selectedNodeId: id,
          };
        }),
      removeBlock: (id) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.body.blocks = document.body.blocks.filter((block) => block.id !== id);
            if (!document.body.blocks.length) {
              document.body.blocks.push({ type: "paragraph", id: createId("block"), content: "" });
            }
          }),
          dirty: true,
          selectedNodeId: state.selectedNodeId === id ? null : state.selectedNodeId,
        })),
      addFact: (input) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.facts.facts.push({
              id: createId("fact"),
              ...input,
              unit: input.unit?.trim() || null,
              role: null,
              sourceRef: input.sourceRef?.trim() || null,
              verified: input.verified ?? input.source !== "model_generated",
            });
          }),
          dirty: true,
        })),
      updateFact: (id, patch) =>
        set((state) => {
          if (!state.document.facts.facts.some((fact) => fact.id === id)) return {};
          return {
            document: changed(state.document, (document) => {
              const fact = document.facts.facts.find((item) => item.id === id)!;
              const contentChanged = ["kind", "value", "unit", "sourceRef"].some((key) => Object.hasOwn(patch, key));
              Object.assign(fact, patch);
              fact.unit = fact.unit?.trim() || null;
              fact.sourceRef = fact.sourceRef?.trim() || null;
              if (contentChanged || !fact.value.trim()) fact.verified = false;
            }),
            dirty: true,
          };
        }),
      removeFact: (id) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            document.facts.facts = document.facts.facts.filter((fact) => fact.id !== id);
          }),
          dirty: true,
        })),
    }),
    {
      name: "bingbi-workbench-v1",
      version: 1,
      // Validate the persisted document shape so a stale or corrupt draft cannot crash rendering;
      // a missing or invalid document falls back to the demo document.
      migrate: (persisted) => {
        const state = (persisted ?? {}) as Partial<WorkbenchState> & { document?: unknown };
        if (!isValidDocument(state.document)) {
          state.document = createDemoDocument();
        }
        return state as WorkbenchState;
      },
      partialize: (state) =>
        isDesktopRuntime()
          ? { showGrid: state.showGrid, theme: state.theme, lastDocumentId: state.lastDocumentId }
          : {
              document: state.document,
              showGrid: state.showGrid,
              theme: state.theme,
              savedAt: state.savedAt,
              lastDocumentId: state.lastDocumentId,
            },
    },
  ),
);
