// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import { DraftWizard } from "./DraftWizard";

function renderWizard(overrides: Partial<ComponentProps<typeof DraftWizard>> = {}) {
  const onFinish = vi.fn();
  const onClose = vi.fn();
  const onComplete = vi.fn();
  const result = render(
    <DraftWizard
      open
      initialDocType="request"
      templates={[]}
      canDraft
      onComplete={onComplete}
      onClose={onClose}
      onFinish={onFinish}
      {...overrides}
    />,
  );
  return { onFinish, onClose, onComplete, rerender: result.rerender };
}

function clickNext() {
  fireEvent.click(screen.getByRole("button", { name: /下一步/ }));
}

afterEach(() => cleanup());

describe("DraftWizard", () => {
  it("collects the brief step by step and hands it to the pipeline", () => {
    const { onFinish } = renderWizard();

    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "申请购置应急排水设备经费" } });
    fireEvent.change(screen.getByLabelText(/主送机关/), { target: { value: "×××人民政府" } });
    clickNext();

    fireEvent.change(screen.getByLabelText(/关键事实/), {
      target: { value: "拟购置 3 台\n经费 15 万元" },
    });
    fireEvent.change(screen.getByLabelText(/完成时限/), { target: { value: "2026 年 9 月 30 日" } });
    clickNext();

    expect(screen.getByLabelText("起草材料预览").textContent).toContain("【文种】请示");
    expect(screen.getByLabelText("起草材料预览").textContent).toContain("【事由】申请购置应急排水设备经费");

    fireEvent.click(screen.getByRole("button", { name: /交给 AI 起草/ }));
    expect(onFinish).toHaveBeenCalledTimes(1);
    expect(onFinish.mock.calls[0][0]).toMatchObject({
      docType: "request",
      subject: "申请购置应急排水设备经费",
      recipient: "×××人民政府",
      facts: "拟购置 3 台\n经费 15 万元",
      deadline: "2026 年 9 月 30 日",
    });
  });

  it("blocks the brief step until the required fields are filled", () => {
    renderWizard();
    clickNext();
    expect(screen.getByRole("alert").textContent).toContain("发文事由");

    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "开展汛前检查" } });
    clickNext();
    expect(screen.getByRole("alert").textContent).toContain("主送机关");

    fireEvent.change(screen.getByLabelText(/主送机关/), { target: { value: "×××单位" } });
    clickNext();
    expect(screen.getByLabelText(/关键事实/)).toBeTruthy();
  });

  it("does not require a recipient for a minutes document", () => {
    renderWizard({ initialDocType: "minutes" });
    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "汛前检查部署" } });
    clickNext();

    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByLabelText(/关键事实/)).toBeTruthy();
  });

  it("shows generation progress and closes onto the result", async () => {
    const { onFinish, onClose, onComplete, rerender } = renderWizard();
    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "开展汛前检查" } });
    fireEvent.change(screen.getByLabelText(/主送机关/), { target: { value: "×××单位" } });
    clickNext();
    clickNext();
    fireEvent.click(screen.getByRole("button", { name: /交给 AI 起草/ }));

    expect(onFinish).toHaveBeenCalledTimes(1);
    expect(screen.getByText("材料已提交，正在整理并起草")).toBeTruthy();
    expect(screen.getByLabelText("生成进度")).toBeTruthy();

    rerender(
      <DraftWizard
        open
        initialDocType="request"
        templates={[]}
        canDraft
        drafting
        stage="draft"
        completedStages={["doctype", "facts"]}
        onComplete={onComplete}
        onClose={onClose}
        onFinish={onFinish}
      />,
    );
    expect(screen.getByText(/正在起草/)).toBeTruthy();

    rerender(
      <DraftWizard
        open
        initialDocType="request"
        templates={[]}
        canDraft
        drafting={false}
        stage={null}
        completedStages={["doctype", "facts", "retrieve", "outline", "draft", "deai", "lint", "correct", "factcheck", "score"]}
        onComplete={onComplete}
        onClose={onClose}
        onFinish={onFinish}
      />,
    );
    await waitFor(() => expect(onComplete).toHaveBeenCalledTimes(1));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not submit without a model but offers provider settings", () => {
    renderWizard({ canDraft: false });
    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "开展汛前检查" } });
    fireEvent.change(screen.getByLabelText(/主送机关/), { target: { value: "×××单位" } });
    clickNext();
    clickNext();

    expect(screen.getByRole("button", { name: /先配置模型服务/ })).toBeTruthy();
    expect(screen.getByText(/当前没有可用的桌面模型服务/)).toBeTruthy();
  });

  it("closes with Escape", () => {
    const { onClose } = renderWizard();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("selects a scenario template and carries it into the brief", () => {
    const templates = [
      {
        id: "notice",
        name: "通知（标准）",
        docType: "notice" as const,
        scenario: null,
        description: null,
        isDefault: true,
      },
      {
        id: "notice-meeting",
        name: "会议通知",
        docType: "notice" as const,
        scenario: "召开会议，明确时间地点和参会人员",
        description: null,
        isDefault: false,
      },
    ];
    const { onFinish } = renderWizard({ initialDocType: "notice", templates });

    fireEvent.change(screen.getByLabelText(/发文事由/), { target: { value: "召开防汛工作会议" } });
    fireEvent.change(screen.getByLabelText(/主送机关/), { target: { value: "×××单位" } });
    clickNext();
    fireEvent.click(screen.getByText("补充材料与模板（可选）"));
    fireEvent.change(screen.getByLabelText("场景模板"), { target: { value: "notice-meeting" } });
    expect(screen.getByText("召开会议，明确时间地点和参会人员")).toBeTruthy();
    clickNext();

    expect(screen.getByLabelText("起草材料预览").textContent).toContain("【场景模板】会议通知");
    fireEvent.click(screen.getByRole("button", { name: /交给 AI 起草/ }));
    expect(onFinish.mock.calls[0][0]).toMatchObject({
      docType: "notice",
      templateId: "notice-meeting",
      templateName: "会议通知",
    });
  });
});
