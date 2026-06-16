// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import TaskDetail from "../pages/TaskDetail";
import { extractWorkOrderResult } from "../utils/workOrderResult";

const org = vi.hoisted(() => ({
  getCompanyWorkOrderTimeline: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: () => "opc-live",
}));

vi.mock("../api/org", () => ({
  getCompanyWorkOrderTimeline: org.getCompanyWorkOrderTimeline,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

function renderTaskDetail(path = "/tasks/wo-1") {
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/tasks/:workOrderId" element={<TaskDetail />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("TaskDetail result surface", () => {
  beforeEach(() => {
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-1",
        mission_intent: "Hello, what can you do?",
        track: "green",
        status: "Completed",
        selected_agents: ["agent-founder-01"],
      },
    ]);
    org.getCompanyWorkOrderTimeline.mockResolvedValue([
      {
        time_ms: 1000,
        type: "ContentDelta",
        title: "ContentDelta",
        details: {
          run_id: "run-older",
          event_seq: 1,
          payload_json: JSON.stringify({ delta: "Old run text." }),
        },
      },
      {
        time_ms: 1100,
        type: "Usage",
        title: "Usage",
        details: {
          run_id: "run-older",
          event_seq: 299,
          payload_json: JSON.stringify({
            prompt_tokens: 1900,
            completion_tokens: 221,
            total_tokens: 2121,
          }),
        },
      },
      {
        time_ms: 2000,
        type: "ContentDelta",
        title: "ContentDelta",
        details: {
          run_id: "run-latest",
          event_seq: 1,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              proposal: { message: "Hello! I am the Founder Assistant.\n\nHow can I assist you today?" },
            }),
          }),
        },
      },
      {
        time_ms: 2100,
        type: "Usage",
        title: "Usage",
        details: {
          run_id: "run-latest",
          event_seq: 2,
          payload_json: JSON.stringify({
            prompt_tokens: 2132,
            completion_tokens: 223,
            total_tokens: 2355,
          }),
        },
      },
      {
        time_ms: 2200,
        type: "Done",
        title: "Done",
        details: {
          run_id: "run-latest",
          event_seq: 3,
          payload_json: JSON.stringify({ finish_reason: "stop" }),
        },
      },
    ]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a completed-state explanation and renders the latest final result", async () => {
    renderTaskDetail();

    expect(await screen.findByText("Completed")).toBeInTheDocument();
    expect(screen.getByText("The task has completed and the latest result is ready to review.")).toBeInTheDocument();
    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-1"));
    expect(await screen.findByText("Hello! I am the Founder Assistant.")).toBeInTheDocument();
    expect(await screen.findByText("How can I assist you today?")).toBeInTheDocument();
    expect(await screen.findByText("2,355 tokens")).toBeInTheDocument();
    expect(screen.queryByText("Old run text.")).not.toBeInTheDocument();
  });

  it("does not expose structured ask_human payloads in the extracted final result", () => {
    const result = extractWorkOrderResult([
      {
        time_ms: 2000,
        type: "ContentDelta",
        details: {
          run_id: "run-latest",
          event_seq: 1,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              ask_human: {
                message: "Please confirm the production rollout.",
                reason: "Approval required before deployment.",
              },
            }),
          }),
        },
      },
    ]);

    expect(result.finalText).toBe("Please confirm the production rollout.");
    expect(result.finalText).not.toContain("ask_human");
    expect(result.finalText).not.toContain("Approval required before deployment.");
  });
});
