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
    expect(await screen.findByText("Task completed and recorded in local timeline and memory.")).toBeInTheDocument();
    expect(screen.getByText(/Memory/)).toHaveTextContent("tm-1");
    await waitFor(() => expect(api.getWorkOrderTimeline).toHaveBeenCalledWith("wo-green"));
    expect(screen.getByText("Processing completed")).toBeInTheDocument();
  });

  it("submits Yellow work for approval instead of implying direct execution", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow" }),
    ]);
    api.executeWorkOrder.mockResolvedValue({ ok: true, status: "WaitingApproval" });

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Submit for Approval" }));

    await waitFor(() => expect(api.executeWorkOrder).toHaveBeenCalledWith("wo-yellow", {}));
    expect(screen.getByText(/Waiting confirmation/)).toBeInTheDocument();
  });

  it("explains that blocked work has no execution timeline", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red" }),
    ]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    await waitFor(() => expect(api.getWorkOrderTimeline).toHaveBeenCalledWith("wo-red"));
    expect(screen.getByText(/no execution timeline will be produced/)).toBeInTheDocument();
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

  it("exports an audit package from the audit export endpoint", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Export Audit" }));

    await waitFor(() => expect(api.getWorkOrderAuditExport).toHaveBeenCalledWith("wo-green"));
    expect(document.body.textContent).toContain("Export Audit");
    expect(document.body.textContent).toContain("coevo.audit_export.v1");
  });

  it("does not submit stale feedback after selecting a different task", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-a", mission_intent: "Analyze onboarding feedback", track: "green" }),
      workOrder({ work_order_id: "wo-b", mission_intent: "Draft customer notification", track: "yellow" }),
    ]);

    render(<WorkOrders />);

    const feedbackInput = await screen.findByPlaceholderText("Feedback...");
    fireEvent.change(feedbackInput, { target: { value: "looks good" } });
    fireEvent.click(screen.getByRole("button", { name: /Draft customer notification/i }));
    fireEvent.click(screen.getByRole("button", { name: "Feedback" }));

    expect(api.submitWorkOrderFeedback).not.toHaveBeenCalled();
  });

  it("clears the feedback input after successful submit", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-a", mission_intent: "Analyze onboarding feedback", track: "green" }),
    ]);

    render(<WorkOrders />);

    const feedbackInput = await screen.findByPlaceholderText("Feedback...");
    fireEvent.change(feedbackInput, { target: { value: "approved with notes" } });
    fireEvent.click(screen.getByRole("button", { name: "Feedback" }));

    await waitFor(() => expect(api.submitWorkOrderFeedback).toHaveBeenCalledWith("wo-a", "approved with notes"));
    expect(feedbackInput).toHaveValue("");
  });

  it("presents a founder-readable Task Center with selected task details, next action, timeline, and audit actions", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze onboarding feedback", track: "green", status: "Completed" }),
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft customer notification", track: "yellow", status: "WaitingApproval" }),
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red", status: "Planned" }),
    ]);
    api.getWorkOrderTimeline.mockResolvedValue([{ type: "ApprovalRequested", label: "Human approval requested" }]);

    render(<WorkOrders />);

    expect(await screen.findByText("Task Center")).toBeInTheDocument();
    expect(screen.getByText("3 all tasks")).toBeInTheDocument();
    expect(screen.getByText("1 needs your confirmation")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Draft customer notification/i }));

    expect(screen.getByText("Selected Task Details")).toBeInTheDocument();
    expect(screen.getAllByText("Next action").length).toBeGreaterThan(0);
    expect(screen.getByText("Confirmation required")).toBeInTheDocument();
    expect(screen.getByText("Waiting for your confirmation")).toBeInTheDocument();
    expect(screen.getAllByText("Assigned AI Employees").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Submit for Approval" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Export Audit" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "View Timeline" }));
    await waitFor(() => expect(api.getWorkOrderTimeline).toHaveBeenCalledWith("wo-yellow"));
    expect(screen.getAllByText("Task Timeline").length).toBeGreaterThan(0);
    expect(screen.getByText("Human approval requested")).toBeInTheDocument();
  });

  it("shows failed timeline events as founder-readable attention items", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Failed" }),
    ]);
    api.getWorkOrderTimeline.mockResolvedValue([
      {
        type: "LifecycleError",
        title: "LifecycleError",
        details: {
          event_id: "evt-1",
          payload_json: JSON.stringify({ status: "Failed", error: "Internal error: missing field `input`" }),
        },
      },
    ]);

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    expect(await screen.findByText("Previous run needed attention")).toBeInTheDocument();
    expect(screen.getByText("Blocked")).toBeInTheDocument();
    expect(screen.queryByText("LifecycleError")).not.toBeInTheDocument();
  });

  it("shows friendly execute errors and avoids raw `ok` output", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);
    api.executeWorkOrder.mockRejectedValue(new Error("ok"));

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    expect(await screen.findByText(/Model returned an invalid response/)).toBeInTheDocument();
    expect(screen.getByText(/Execution failed: Failed/)).toBeInTheDocument();
    expect(screen.queryByText(/^Error: ok$/)).not.toBeInTheDocument();
  });

  it("hides raw technical fields by default and keeps them under advanced details", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Planned" }),
    ]);

    render(<WorkOrders />);

    expect(await screen.findByText("Task Center")).toBeInTheDocument();
    expect(screen.getByText("Advanced settings")).toBeInTheDocument();
    expect(screen.queryByText(/^green$/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/^Planned$/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Advanced settings"));
    expect(await screen.findByText("Internal task ID")).toBeInTheDocument();
    expect(screen.getByText("wo-green")).toBeInTheDocument();
    expect(screen.getAllByText("agent-founder-01").length).toBeGreaterThan(0);
  });

  it("marks task as failed and offers a non-Red retry after execution error", async () => {
    api.listWorkOrders
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Planned" }),
      ])
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Failed" }),
      ]);
    api.executeWorkOrder.mockRejectedValue(new Error("MODEL_ROUTE_UNAVAILABLE: deepseek route failed"));

    render(<WorkOrders />);
    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    await waitFor(() => expect(screen.getAllByText("Needs attention").length).toBeGreaterThan(0));
    expect(screen.getByText("Model execution is unavailable right now. Please check model settings and try again.")).toBeInTheDocument();
    expect(screen.queryByText(/MODEL_ROUTE_UNAVAILABLE/)).not.toBeInTheDocument();
    await waitFor(() => expect(api.listWorkOrders).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("button", { name: /^Execute$/ })).not.toBeInTheDocument();
    const runAgainButton = screen.getByRole("button", { name: "Run again" });

    api.executeWorkOrder.mockResolvedValue({
      ok: true,
      status: "Completed",
      summary: "Recovered on retry.",
    });
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Completed" }),
    ]);
    fireEvent.click(runAgainButton);

    await waitFor(() => expect(api.executeWorkOrder).toHaveBeenLastCalledWith("wo-green", { rerun: true }));
  });
});
