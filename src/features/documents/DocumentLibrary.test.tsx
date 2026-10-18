// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DocumentSummary } from "../../lib/store";
import { DocumentLibrary } from "./DocumentLibrary";

function summary(id: string, title: string, docType: DocumentSummary["docType"]): DocumentSummary {
  return {
    id,
    title,
    docType,
    revision: 1,
    status: "draft",
    versionCount: 1,
    createdAt: 1,
    updatedAt: 1,
  };
}

const documents = [
  summary("d1", "关于购置排水设备的请示", "request"),
  summary("d2", "关于开展安全检查的通知", "notice"),
];

afterEach(() => cleanup());

describe("DocumentLibrary", () => {
  it("filters documents by title and type", () => {
    render(
      <DocumentLibrary
        open
        documents={documents}
        currentDocumentId="d1"
        loading={false}
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onOpen={vi.fn()}
        onHistory={vi.fn()}
        onSessions={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText("关于购置排水设备的请示")).toBeTruthy();
    const search = screen.getByLabelText("搜索文档");

    fireEvent.change(search, { target: { value: "请示" } });
    expect(screen.getByText("关于购置排水设备的请示")).toBeTruthy();
    expect(screen.queryByText("关于开展安全检查的通知")).toBeNull();

    fireEvent.change(search, { target: { value: "通知" } });
    expect(screen.queryByText("关于购置排水设备的请示")).toBeNull();
    expect(screen.getByText("关于开展安全检查的通知")).toBeTruthy();

    fireEvent.change(search, { target: { value: "不存在" } });
    expect(screen.getByText("没有匹配的文档")).toBeTruthy();
  });
});
