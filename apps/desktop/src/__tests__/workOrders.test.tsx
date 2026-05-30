import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import WorkOrders from "../pages/WorkOrders";

const api = vi.hoisted(() => ({
  cancelWorkOrder: vi.fn(),
  executeWorkOrder: vi.fn(),
  getWorkOrderAuditExport: vi.fn(),
  getWorkOrderTimeline: vi.fn(),
  listWorkOrders: vi.fn(),
  submitWorkOrderFeedback: vi.fn(),
}));

vi.mock("../api/client", () => ({
  cancelWorkOrder: api.cancelWorkOrder,
  executeWorkOrder: api.executeWorkOrder,
  getWorkOrderAuditExport: api.getWorkOrderAuditExport,
  getWorkOrderTimeline: api.getWorkOrderTimeline,
  listWorkOrders: api.listWorkOrders,
  submitWorkOrderFeedback: api.submitWorkOrderFeedback,
}));

function workOrder(overrides: Record<string, unknown>) {
  return {
    work_order_id: "wo-1",
    mission_intent: "Analyze README",
    track: "green",
    status: "Planned",
    contract_hash: "a".repeat(64),
    selected_agents: ["agent-founder-01"],
    selected_executors: [],
    required_skills: ["skill-mission-draft"],
    ...overrides,
  };
}

describe("WorkOrders", () => {
  beforeEach(() => {
    api.cancelWorkOrder.mockResolvedValue({ ok: true });
    api.executeWorkOrder.mockResolvedValue({ ok: true, status: "Completed" });
    api.getWorkOrderAuditExport.mockResolvedValue({ schema_version: "coevo.audit_export.v1" });
    api.getWorkOrderTimeline.mockResolvedValue([]);
    api.listWorkOrders.mockResolvedValue([]);
    api.submitWorkOrderFeedback.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("disables Red Track execution in the UI", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red" }),
    ]);

    render(<WorkOrders />);

    const button = await screen.findByRole("button", { name: "Execute (Blocked)" });
    expect(button).toBeDisabled();
  });

  it("does not show ordinary Execute for completed rows", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-done", mission_intent: "Analyze README", status: "Completed" }),
    ]);

    render(<WorkOrders />);

    expect(await screen.findByRole("button", { name: "View Result" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run again" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Execute$/ })).not.toBeInTheDocument();
  });

  it("keeps completed red rows blocked from runnable re-execution", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({
        work_order_id: "wo-red-completed",
        mission_intent: "Delete production data",
        track: "red",
        status: "Completed",
      }),
    ]);

    render(<WorkOrders />);

    const runAgainButton = await screen.findByRole("button", { name: "Run again" });
    expect(runAgainButton).toBeDisabled();
  });

  it("renders Green execution result inline and auto-loads the row timeline", async () => {
    api.listWorkOrders
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Planned" }),
      ])
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Completed" }),
      ]);
    api.executeWorkOrder.mockResolvedValue({
      ok: true,
      status: "Completed",
      summary: "WorkerHarness Completed execution.",
      memory_ids: ["tm-1"],
      worker_runs: [{ run_id: "run-1", status: "Completed" }],
      worker_steps: [{ step_id: "s-1" }, { step_id: "s-2" }],
      tool_calls: [{ tool_id: "file-readonly", success: true }],
    });
    api.getWorkOrderTimeline.mockResolvedValue([{ type: "LifecycleEnd", title: "Completed" }]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    await waitFor(() => expect(api.executeWorkOrder).toHaveBeenCalledWith("wo-green", {}));
    expect(await screen.findByText("WorkerHarness Completed execution.")).toBeInTheDocument();
    expect(screen.getByText(/Memory/)).toHaveTextContent("tm-1");
    await waitFor(() => expect(api.getWorkOrderTimeline).toHaveBeenCalledWith("wo-green"));
    expect(screen.getByText("LifecycleEnd")).toBeInTheDocument();
  });

  it("submits Yellow work for approval instead of implying direct execution", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow" }),
    ]);
    api.executeWorkOrder.mockResolvedValue({ ok: true, status: "WaitingApproval" });

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Submit for Approval" }));

    await waitFor(() => expect(api.executeWorkOrder).toHaveBeenCalledWith("wo-yellow", {}));
    expect(screen.getByText(/WaitingApproval/)).toBeInTheDocument();
  });

  it("explains that Red work has no execution timeline in Alpha", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red" }),
    ]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    await waitFor(() => expect(api.getWorkOrderTimeline).toHaveBeenCalledWith("wo-red"));
    expect(screen.getByText(/No execution timeline will be produced/)).toBeInTheDocument();
  });

  it("renders timeline loading errors", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);
    api.getWorkOrderTimeline.mockRejectedValue(new Error("timeline unavailable"));

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    await waitFor(() => expect(screen.getByText(/Timeline error: timeline unavailable/)).toBeInTheDocument());
  });

  it("exports a WorkOrder audit package from the audit export endpoint", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Export Audit" }));

    await waitFor(() => expect(api.getWorkOrderAuditExport).toHaveBeenCalledWith("wo-green"));
    expect(document.body.textContent).toContain("Export Audit");
    expect(document.body.textContent).toContain("coevo.audit_export.v1");
  });
});
