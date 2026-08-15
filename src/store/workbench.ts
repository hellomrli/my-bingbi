import { create } from "zustand";
import { persist } from "zustand/middleware";
import { createDemoDocument } from "../lib/document";
import { createId } from "../lib/id";
import type {
  Block,
  DocumentType,
  Fact,
  Header,
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
  updateSigner: (authority: string) => void;
  updateDate: (value: string) => void;
  updateBlock: (id: string, patch: Partial<Block>) => void;
  addBlock: (kind: "paragraph" | "heading") => void;
  removeBlock: (id: string) => void;
  addFact: (input: Pick<Fact, "kind" | "value" | "unit" | "source">) => void;
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
      unverifiedTokens: {},
      setDocument: (document, savedAt = Date.now()) =>
        set({ document, dirty: false, selectedNodeId: null, savedAt, unverifiedTokens: {} }),
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
      updateBlock: (id, patch) =>
        set((state) => ({
          document: changed(state.document, (document) => {
            const index = document.body.blocks.findIndex((block) => block.id === id);
            if (index < 0) return;
            document.body.blocks[index] = { ...document.body.blocks[index], ...patch } as Block;
          }),
          dirty: true,
        })),
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
              sourceRef: null,
              verified: input.source !== "model_generated",
            });
          }),
          dirty: true,
        })),
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
      partialize: (state) =>
        isDesktopRuntime()
          ? { showGrid: state.showGrid, theme: state.theme }
          : {
              document: state.document,
              showGrid: state.showGrid,
              theme: state.theme,
              savedAt: state.savedAt,
            },
    },
  ),
);
