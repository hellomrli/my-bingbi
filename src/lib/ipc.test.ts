// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import { createEmptyDocument } from "./document";
import {
  applyUpdate,
  checkForUpdate,
  defaultAppConfig,
  defaultKbConfig,
  deleteDocument,
  exportDocx,
  hasKbFnsToken,
  hasProviderSecret,
  listPackSummaries,
  listProviderModels,
  listProviderProfiles,
  listDocuments,
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

  it("does not allow model calls to bypass the Rust security boundary", async () => {
    await expect(testProviderConnection("ollama")).rejects.toMatchObject({ code: "model_runtime_unavailable" });
    await expect(listProviderModels("anthropic")).rejects.toMatchObject({ code: "model_runtime_unavailable" });
  });

  it("keeps DOCX export behind the desktop boundary", async () => {
    await expect(exportDocx("doc-browser")).rejects.toMatchObject({ code: "export_unavailable" });
  });
});
