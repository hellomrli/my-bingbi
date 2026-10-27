import { describe, expect, it } from "vitest";
import { parseRecipientInference } from "./recipient";

describe("parseRecipientInference", () => {
  it("accepts only names grounded in the user's reply", () => {
    const result = parseRecipientInference(
      JSON.stringify({ recipients: ["市政府"], evidence: "是发给市政府的", question: null }),
      "是发给市政府的",
      "notice",
    );
    expect(result.recipients).toEqual(["市政府"]);
    expect(result.question).toBeNull();
  });

  it("keeps the recipient optional when the model invents a name", () => {
    const result = parseRecipientInference(
      JSON.stringify({ recipients: ["财政局"], evidence: "发给财政局", question: null }),
      "是发给市政府的",
      "notice",
    );
    expect(result.recipients).toEqual([]);
    expect(result.question).toBeTruthy();
  });

  it("does not allow multiple recipients for a request", () => {
    const result = parseRecipientInference(
      JSON.stringify({ recipients: ["市政府", "财政局"], evidence: "给市政府和财政局", question: null }),
      "给市政府和财政局",
      "request",
    );
    expect(result.recipients).toEqual([]);
    expect(result.question).toBeTruthy();
  });
});
