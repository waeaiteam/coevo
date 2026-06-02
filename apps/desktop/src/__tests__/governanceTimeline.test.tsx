import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { setLanguage } from "../settings/i18n";

const spans: TimelineSpan[] = [
  {
    id: "span-model-1",
    type: "ModelCall",
    label: "模型思考",
    round: 2,
    durationMs: 1530,
    tokens: 128,
    costUsd: 0.0123,
    trust: "native",
    gate: { outcome: "need_approval", reason: "需要确认", action_digest: "digest-123" },
    overlays: ["need_approval", "hypothesis_downgraded"],
    thought: "先整理数据，再给出建议。",
    proposal: { action: "write_summary" },
    confidence: 0.82,
    usage: { prompt_tokens: 100, completion_tokens: 28, total_tokens: 128 },
    input: { mission: "整理客户线索" },
    output: { ok: true },
  },
  {
    id: "span-external-1",
    type: "CallExecutor",
    label: "外部员工回报",
    round: 2,
    trust: "external",
    gate: { outcome: "allow" },
  },
];

describe("GovernanceTimeline", () => {
  afterEach(cleanup);

  it("renders span waterfall rows with usage, cost, badges, and expandable details", () => {
    setLanguage("zh");
    render(<GovernanceTimeline spans={spans} title="执行时间线" />);

    expect(screen.getByText("执行时间线")).toBeInTheDocument();
    expect(screen.getByText("模型整理")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /模型整理/ }));
    expect(screen.getAllByText(/(round|轮次)\s+2/).length).toBeGreaterThan(0);
    expect(screen.getByText(/128\s+(用量|usage)/)).toBeInTheDocument();
    expect(screen.getByText(/\$0\.0123/)).toBeInTheDocument();
    expect(screen.getAllByText(/(待确认|待审批|Needs approval)/).length).toBeGreaterThan(0);

    expect(screen.getByText("先整理数据，再给出建议。")).toBeInTheDocument();
    expect(screen.getByText(/write_summary/)).toBeInTheDocument();
    expect(screen.getByText(/(Hypothesis downgraded|Hypothesis 降级|假设降级|假设已降级)/)).toBeInTheDocument();
  });

  it("emits inline approval decisions with comments", () => {
    setLanguage("zh");
    const approve = vi.fn();
    const reject = vi.fn();
    render(<GovernanceTimeline spans={spans} onApprove={approve} onReject={reject} />);

    fireEvent.click(screen.getByRole("button", { name: /模型整理/ }));
    fireEvent.change(screen.getByPlaceholderText(/(批注意见，可留空|可选批注|可选备注|Optional comment)/), {
      target: { value: "可以继续" },
    });
    fireEvent.click(screen.getByRole("button", { name: "批准" }));

    expect(approve).toHaveBeenCalledWith(spans[0], "可以继续");
    expect(reject).not.toHaveBeenCalled();
  });
});
