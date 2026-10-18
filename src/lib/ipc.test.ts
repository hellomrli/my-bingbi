// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from "vitest";
import { createEmptyDocument } from "./document";
import {
  applyUpdate,
  checkForUpdate,
  clearCrashEvents,
  defaultAppConfig,
  defaultKbConfig,
  deleteDocument,
  exportDocx,
  extractDocxText,
  extractReferenceText,
  hasKbFnsToken,
  hasProviderSecret,
  listPackSummaries,
  listProviderModels,
  listProviderProfiles,
  listDocuments,
  listCrashEvents,
  listDocumentVersions,
  loadDocumentVersion,
  loadKbConfig,
  loadAppConfig,
  loadDocument,
  saveAppConfig,
  saveKbConfig,
  saveDocument,
  saveProviderProfile,
  testProviderConnection,
} from "./ipc";

describe("browser document repository", () => {
  beforeEach(() => localStorage.clear());

  it("saves, lists and loads documents", async () => {
    const document = createEmptyDocument();
    document.body.title.text = "存储测试请示";

    const result = await saveDocument(document, "user_save");
    expect(result.created).toBe(true);
    expect(result.versionSeq).toBe(1);

    const summaries = await listDocuments();
    expect(summaries).toHaveLength(1);
    expect(summaries[0]).toMatchObject({ title: "存储测试请示", versionCount: 1 });
    expect(await loadDocument(document.id)).toEqual(document);
  });

  it("deduplicates identical saves and versions changed content", async () => {
    const document = createEmptyDocument();
    await saveDocument(document, "user_save");
    expect((await saveDocument(document, "autosave")).versionCreated).toBe(false);

    document.revision += 1;
    document.body.title.text = "第二版";
    const changed = await saveDocument(document, "autosave");
    expect(changed.versionCreated).toBe(true);
    expect(changed.versionSeq).toBe(2);
    expect((await listDocuments())[0].versionCount).toBe(2);
  });

  it("deletes documents", async () => {
    const document = createEmptyDocument();
    await saveDocument(document, "user_save");
    expect(await deleteDocument(document.id)).toBe(true);
    await expect(loadDocument(document.id)).rejects.toMatchObject({ code: "document_not_found" });
  });

  it("keeps a bounded version window while sequence numbers stay addressable", async () => {
    const document = createEmptyDocument();
    let latest = await saveDocument(document, "user_save");
    for (let index = 0; index < 54; index += 1) {
      document.revision += 1;
      document.body.title.text = `第 ${index + 2} 版`;
      latest = await saveDocument(document, "autosave");
    }
    // 55 saves total; the preview keeps the newest 50 snapshots.
    expect(latest.versionSeq).toBe(55);

    const versions = await listDocumentVersions(document.id);
    expect(versions).toHaveLength(50);
    expect(versions[0].seq).toBe(55);
    expect(versions[versions.length - 1].seq).toBe(6);

    const oldestKept = await loadDocumentVersion(document.id, 6);
    expect(oldestKept.body.title.text).toBe("第 6 版");
    await expect(loadDocumentVersion(document.id, 5)).rejects.toMatchObject({ code: "version_not_found" });
  });

  it("reports a full browser storage instead of a raw quota error", async () => {
    const document = createEmptyDocument();
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("quota", "QuotaExceededError");
    });
    try {
      await expect(saveDocument(document, "user_save")).rejects.toMatchObject({ code: "storage_full" });
    } finally {
      setItem.mockRestore();
    }
  });

  it("rejects stale revisions that would overwrite newer content", async () => {
    const document = createEmptyDocument();
    document.revision = 2;
    document.body.title.text = "较新内容";
    await saveDocument(document, "autosave");

    const stale = structuredClone(document);
    stale.revision = 1;
    stale.body.title.text = "过期内容";
    await expect(saveDocument(stale, "autosave")).rejects.toMatchObject({ code: "stale_revision" });
    expect((await loadDocument(document.id)).body.title.text).toBe("较新内容");
  });

  it("round-trips application config", async () => {
    expect(await loadAppConfig()).toEqual(defaultAppConfig);
    const config = { ...defaultAppConfig, theme: "dark" as const, autosaveEnabled: false };
    await saveAppConfig(config);
    expect(await loadAppConfig()).toEqual(config);
  });

  it("does not emulate a plaintext secret store in the browser", async () => {
    await expect(hasProviderSecret("deepseek")).rejects.toMatchObject({ code: "secret_store_unavailable" });
    await expect(hasKbFnsToken()).rejects.toMatchObject({ code: "secret_store_unavailable" });
  });

  it("round-trips only non-sensitive knowledge-base metadata in browser preview", async () => {
    expect(await loadKbConfig()).toEqual(defaultKbConfig);
    const saved = await saveKbConfig({
      ...defaultKbConfig,
      enabled: true,
      providerKind: "local_vault",
      vault: " 政务库 ",
      localPath: "/tmp/obsidian-vault",
    });
    expect(saved).toMatchObject({ vault: "政务库", localPath: "/tmp/obsidian-vault" });
    expect(await loadKbConfig()).toEqual(saved);
    const persisted = localStorage.getItem("bingbi-browser-kb-config-v1") ?? "";
    expect(persisted.toLowerCase()).not.toContain("token");
  });

  it("does not emulate the desktop pack registry in browser preview", async () => {
    expect(await listPackSummaries()).toEqual([]);
  });

  it("round-trips only non-sensitive provider metadata in browser preview", async () => {
    const builtins = await listProviderProfiles();
    expect(builtins.map((profile) => profile.id)).toEqual(
      expect.arrayContaining(["ollama", "openai-compatible", "deepseek", "anthropic"]),
    );

    const saved = await saveProviderProfile({
      id: "local-test",
      kind: "ollama",
      displayName: " 本地测试 ",
      baseUrl: "http://localhost:11434/",
      model: " qwen3 ",
      enabled: true,
    });
    expect(saved).toMatchObject({ displayName: "本地测试", model: "qwen3", isLocal: true });
    expect(await listProviderProfiles()).toContainEqual(saved);
    expect(localStorage.getItem("bingbi-browser-providers-v1")).not.toContain("apiKey");
  });

  it("keeps update checks behind the desktop boundary", async () => {
    await expect(checkForUpdate()).rejects.toMatchObject({ code: "update_unavailable" });
    await expect(applyUpdate()).rejects.toMatchObject({ code: "update_unavailable" });
  });

  it("keeps local crash diagnostics behind the desktop boundary", async () => {
    expect(await listCrashEvents()).toEqual([]);
    await expect(clearCrashEvents()).rejects.toMatchObject({ code: "crash_diagnostics_unavailable" });
  });

  it("does not allow model calls to bypass the Rust security boundary", async () => {
    await expect(testProviderConnection("ollama")).rejects.toMatchObject({ code: "model_runtime_unavailable" });
    await expect(listProviderModels("anthropic")).rejects.toMatchObject({ code: "model_runtime_unavailable" });
  });

  it("keeps DOCX export behind the desktop boundary", async () => {
    await expect(exportDocx("doc-browser")).rejects.toMatchObject({ code: "export_unavailable" });
    await expect(extractDocxText(new Uint8Array([1, 2, 3]))).rejects.toMatchObject({
      code: "reference_upload_unavailable",
    });
    await expect(extractReferenceText("材料.pdf", new Uint8Array([1, 2, 3]))).rejects.toMatchObject({
      code: "reference_upload_unavailable",
    });
  });
});
