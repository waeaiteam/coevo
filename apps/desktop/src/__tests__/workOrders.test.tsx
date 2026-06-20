import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import WorkOrders from "../pages/WorkOrders";

const api = vi.hoisted(() => ({
  cancelWorkOrder: vi.fn(),
  decideWorkOrderApproval: vi.fn(),
  executeWorkOrder: vi.fn(),
  getWorkOrderAuditExport: vi.fn(),
  getWorkOrderTimeline: vi.fn(),
  listWorkOrders: vi.fn(),
  submitWorkOrderFeedback: vi.fn(),
}));

const org = vi.hoisted(() => ({
  cancelCompanyWorkOrder: vi.fn(),
  decideCompanyWorkOrderApproval: vi.fn(),
  executeCompanyWorkOrder: vi.fn(),
  getCompanyWorkOrderAuditExport: vi.fn(),
  getCompanyWorkOrderTimeline: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
  submitCompanyWorkOrderFeedback: vi.fn(),
}));

vi.mock("../api/client", () => ({
  cancelWorkOrder: api.cancelWorkOrder,
  decideWorkOrderApproval: api.decideWorkOrderApproval,
  executeWorkOrder: api.executeWorkOrder,
  getWorkOrderAuditExport: api.getWorkOrderAuditExport,
  getWorkOrderTimeline: api.getWorkOrderTimeline,
  listWorkOrders: api.listWorkOrders,
  submitWorkOrderFeedback: api.submitWorkOrderFeedback,
}));

vi.mock("../api/org", () => ({
  cancelCompanyWorkOrder: org.cancelCompanyWorkOrder,
  decideCompanyWorkOrderApproval: org.decideCompanyWorkOrderApproval,
  executeCompanyWorkOrder: org.executeCompanyWorkOrder,
  getCompanyWorkOrderAuditExport: org.getCompanyWorkOrderAuditExport,
  getCompanyWorkOrderTimeline: org.getCompanyWorkOrderTimeline,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
  submitCompanyWorkOrderFeedback: org.submitCompanyWorkOrderFeedback,
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: () => "opc-live",
}));

function renderWorkOrders() {
  render(
    <MemoryRouter>
      <WorkOrders />
    </MemoryRouter>,
  );
}

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
    api.decideWorkOrderApproval.mockResolvedValue({ ok: true });
    api.executeWorkOrder.mockResolvedValue({ ok: true, status: "Completed" });
    api.getWorkOrderAuditExport.mockResolvedValue({ schema_version: "coevo.audit_export.v1" });
    api.getWorkOrderTimeline.mockResolvedValue([]);
    api.listWorkOrders.mockResolvedValue([]);
    api.submitWorkOrderFeedback.mockResolvedValue({ ok: true });

    org.cancelCompanyWorkOrder.mockResolvedValue({ ok: true });
    org.decideCompanyWorkOrderApproval.mockResolvedValue({ ok: true });
    org.executeCompanyWorkOrder.mockResolvedValue({ ok: true, status: "Completed" });
    org.getCompanyWorkOrderAuditExport.mockResolvedValue({ schema_version: "coevo.audit_export.v1" });
    org.getCompanyWorkOrderTimeline.mockResolvedValue([]);
    org.listCompanyWorkOrders.mockResolvedValue([]);
    org.submitCompanyWorkOrderFeedback.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("disables Red Track execution in the UI", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red" }),
    ]);

    renderWorkOrders();

    const button = await screen.findByRole("button", { name: "Execute (Blocked)" });
    expect(button).toBeDisabled();
  });

  it("loads tasks through the active company-scoped work-order API", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([]);
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-company", mission_intent: "Canonical company task" }),
    ]);

    renderWorkOrders();

    expect(await screen.findAllByText("Canonical company task")).not.toHaveLength(0);
    expect(org.listCompanyWorkOrders).toHaveBeenCalledWith("opc-live");
    expect(api.listWorkOrders).not.toHaveBeenCalled();
  });

  it("executes and reloads timeline through the company-scoped work-order API", async () => {
    const planned = workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Planned" });
    const completed = workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Completed" });
    org.listCompanyWorkOrders.mockResolvedValue([planned, completed]);
    org.listCompanyWorkOrders
      .mockResolvedValueOnce([planned])
      .mockResolvedValueOnce([completed]);
    org.executeCompanyWorkOrder.mockResolvedValue({
      ok: true,
      status: "Completed",
      summary: "WorkerHarness Completed execution.",
      worker_runs: [{ run_id: "run-1", status: "Completed" }],
    });
    org.getCompanyWorkOrderTimeline.mockResolvedValue([{ type: "LifecycleEnd", title: "Completed" }]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    await waitFor(() =>
      expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-live", "wo-green", {}),
    );
    await waitFor(() =>
      expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-green"),
    );
  });

  it("does not show ordinary Execute for completed rows", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-done", mission_intent: "Analyze README", status: "Completed" }),
    ]);

    renderWorkOrders();

    expect(await screen.findByRole("link", { name: "View Result" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run again" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Execute$/ })).not.toBeInTheDocument();
  });

  it("keeps completed red rows blocked from runnable re-execution", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({
        work_order_id: "wo-red-completed",
        mission_intent: "Delete production data",
        track: "red",
        status: "Completed",
      }),
    ]);

    renderWorkOrders();

    const runAgainButton = await screen.findByRole("button", { name: "Run again" });
    expect(runAgainButton).toBeDisabled();
  });

  it("renders Green execution result inline and auto-loads the row timeline", async () => {
    org.listCompanyWorkOrders
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Planned" }),
      ])
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", status: "Completed" }),
      ]);
    org.executeCompanyWorkOrder.mockResolvedValue({
      ok: true,
      status: "Completed",
      summary: "WorkerHarness Completed execution.",
      memory_ids: ["tm-1"],
      worker_runs: [{ run_id: "run-1", status: "Completed" }],
      worker_steps: [{ step_id: "s-1" }, { step_id: "s-2" }],
      tool_calls: [{ tool_id: "file-readonly", success: true }],
    });
    org.getCompanyWorkOrderTimeline.mockResolvedValue([{ type: "LifecycleEnd", title: "Completed" }]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    await waitFor(() => expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-live", "wo-green", {}));
    expect(await screen.findByText("Task completed and recorded in local timeline and memory.")).toBeInTheDocument();
    expect(screen.getByText(/Memory/)).toHaveTextContent("tm-1");
    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-green"));
    expect(screen.getByText("Processing completed")).toBeInTheDocument();
  });

  it("submits Yellow work for approval instead of implying direct execution", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow" }),
    ]);
    org.executeCompanyWorkOrder.mockResolvedValue({ ok: true, status: "WaitingApproval" });

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Submit for Approval" }));

    await waitFor(() => expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-live", "wo-yellow", {}));
    expect(screen.getByText(/Waiting confirmation/)).toBeInTheDocument();
  });

  it("explains that blocked work has no execution timeline", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red" }),
    ]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-red"));
    expect(screen.getByText(/no execution timeline will be produced/)).toBeInTheDocument();
  });

  it("renders timeline loading errors", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);
    org.getCompanyWorkOrderTimeline.mockRejectedValue(new Error("timeline unavailable"));

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    await waitFor(() => expect(screen.getByText(/Timeline error: timeline unavailable/)).toBeInTheDocument());
  });

  it("refreshes a missing approval id from the timeline before deciding", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow", status: "WaitingApproval" }),
    ]);
    org.getCompanyWorkOrderTimeline.mockResolvedValue([
      {
        type: "ApprovalRequired",
        details: {
          payload_json: JSON.stringify({
            approval_id: "approval-123",
            reason: "Need confirmation before proceeding",
            action_digest: "digest-123",
          }),
        },
      },
    ]);
    org.decideCompanyWorkOrderApproval.mockResolvedValue({ ok: true, approval_receipt: "approval-123" });
    org.executeCompanyWorkOrder.mockResolvedValue({ ok: true, status: "Running", run_id: "run-approval-123" });

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));
    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-yellow"));
    fireEvent.click(await screen.findByRole("button", { name: /Approval Required/i }));
    fireEvent.click(await screen.findByRole("button", { name: /Approve/i }));

    await waitFor(() =>
      expect(org.decideCompanyWorkOrderApproval).toHaveBeenCalledWith("opc-live", "wo-yellow", {
        approval_id: "approval-123",
        decision: "approve",
        comment: "",
      }),
    );
    await waitFor(() =>
      expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-live", "wo-yellow", {
        caller_identity_proof: "approval-123",
      }),
    );
  });

  it("uses a top-level approval id from the refreshed timeline before deciding", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow", status: "WaitingApproval" }),
    ]);
    org.getCompanyWorkOrderTimeline.mockResolvedValue([
      {
        type: "ApprovalRequired",
        approval_id: "approval-456",
        details: {
          action_urn: "urn:coevo:work-order:wo-yellow:execute",
          approval_mode: "NEGATIVE_CONSENT",
        },
      },
    ]);
    org.decideCompanyWorkOrderApproval.mockResolvedValue({ ok: true, approval_receipt: "approval-456" });

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));
    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-yellow"));
    fireEvent.click(await screen.findByRole("button", { name: /Approval Required/i }));
    fireEvent.click(await screen.findByRole("button", { name: /Approve/i }));

    await waitFor(() =>
      expect(org.decideCompanyWorkOrderApproval).toHaveBeenCalledWith("opc-live", "wo-yellow", {
        approval_id: "approval-456",
        decision: "approve",
        comment: "",
      }),
    );
  });

  it("exports an audit package from the audit export endpoint", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Export Audit" }));

    await waitFor(() => expect(org.getCompanyWorkOrderAuditExport).toHaveBeenCalledWith("opc-live", "wo-green"));
    expect(document.body.textContent).toContain("Export Audit");
    expect(document.body.textContent).toContain("coevo.audit_export.v1");
  });

  it("surfaces cancelled work as non-runnable founder-readable state after cancel", async () => {
    org.listCompanyWorkOrders
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Planned" }),
      ])
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Cancelled" }),
      ]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(org.cancelCompanyWorkOrder).toHaveBeenCalledWith("opc-live", "wo-green"));
    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalledTimes(2));
    expect(screen.getByText("This task was cancelled")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Execute$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Run again" })).not.toBeInTheDocument();
  });

  it("does not submit stale feedback after selecting a different task", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-a", mission_intent: "Analyze onboarding feedback", track: "green" }),
      workOrder({ work_order_id: "wo-b", mission_intent: "Draft customer notification", track: "yellow" }),
    ]);

    renderWorkOrders();

    const feedbackInput = await screen.findByPlaceholderText("Feedback...");
    fireEvent.change(feedbackInput, { target: { value: "looks good" } });
    fireEvent.click(screen.getByRole("button", { name: /Draft customer notification/i }));
    fireEvent.click(screen.getByRole("button", { name: "Feedback" }));

    expect(org.submitCompanyWorkOrderFeedback).not.toHaveBeenCalled();
  });

  it("clears the feedback input after successful submit", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-a", mission_intent: "Analyze onboarding feedback", track: "green" }),
    ]);

    renderWorkOrders();

    const feedbackInput = await screen.findByPlaceholderText("Feedback...");
    fireEvent.change(feedbackInput, { target: { value: "approved with notes" } });
    fireEvent.click(screen.getByRole("button", { name: "Feedback" }));

    await waitFor(() => expect(org.submitCompanyWorkOrderFeedback).toHaveBeenCalledWith("opc-live", "wo-a", "approved with notes"));
    expect(feedbackInput).toHaveValue("");
  });

  it("presents a founder-readable Task Center with selected task details, next action, timeline, and audit actions", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze onboarding feedback", track: "green", status: "Completed" }),
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft customer notification", track: "yellow", status: "WaitingApproval" }),
      workOrder({ work_order_id: "wo-red", mission_intent: "Delete production data", track: "red", status: "Planned" }),
    ]);
    org.getCompanyWorkOrderTimeline.mockResolvedValue([{ type: "ApprovalRequested", label: "Human approval requested" }]);

    renderWorkOrders();

    expect(await screen.findByText("Today's Work")).toBeInTheDocument();
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
    await waitFor(() => expect(org.getCompanyWorkOrderTimeline).toHaveBeenCalledWith("opc-live", "wo-yellow"));
    expect(screen.getAllByText("Task Timeline").length).toBeGreaterThan(0);
    expect(screen.getByText("Human approval requested")).toBeInTheDocument();
  });

  it("shows failed timeline events as founder-readable attention items", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Failed" }),
    ]);
    org.getCompanyWorkOrderTimeline.mockResolvedValue([
      {
        type: "LifecycleError",
        title: "LifecycleError",
        details: {
          event_id: "evt-1",
          payload_json: JSON.stringify({ status: "Failed", error: "Internal error: missing field `input`" }),
        },
      },
    ]);

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "View Timeline" }));

    expect(await screen.findByText("Previous run needed attention")).toBeInTheDocument();
    expect(screen.getByText("Paused")).toBeInTheDocument();
    expect(screen.queryByText("LifecycleError")).not.toBeInTheDocument();
  });

  it("shows friendly execute errors and avoids raw `ok` output", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green" }),
    ]);
    org.executeCompanyWorkOrder.mockRejectedValue(new Error("ok"));

    renderWorkOrders();

    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    expect(await screen.findByText(/Model returned an invalid response/)).toBeInTheDocument();
    expect(screen.getByText(/Execution failed: Failed/)).toBeInTheDocument();
    expect(screen.queryByText(/^Error: ok$/)).not.toBeInTheDocument();
  });

  it("hides raw technical fields by default and keeps them under advanced details", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Planned" }),
    ]);

    renderWorkOrders();

    expect(await screen.findByText("Today's Work")).toBeInTheDocument();
    expect(screen.getByText("Advanced settings")).toBeInTheDocument();
    expect(screen.queryByText(/^green$/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/^Planned$/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Advanced settings"));
    expect(await screen.findByText("Internal task ID")).toBeInTheDocument();
    expect(screen.getByText("wo-green")).toBeInTheDocument();
    expect(screen.getAllByText("agent-founder-01").length).toBeGreaterThan(0);
  });

  it("marks task as failed and offers a non-Red retry after execution error", async () => {
    org.listCompanyWorkOrders
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Planned" }),
      ])
      .mockResolvedValueOnce([
        workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Failed" }),
      ]);
    org.executeCompanyWorkOrder.mockRejectedValue(new Error("MODEL_ROUTE_UNAVAILABLE: deepseek route failed"));

    renderWorkOrders();
    fireEvent.click(await screen.findByRole("button", { name: "Execute" }));

    await waitFor(() => expect(screen.getAllByText("Needs attention").length).toBeGreaterThan(0));
    expect(screen.getByText("Model execution is unavailable right now. Please check model settings and try again.")).toBeInTheDocument();
    expect(screen.queryByText(/MODEL_ROUTE_UNAVAILABLE/)).not.toBeInTheDocument();
    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("button", { name: /^Execute$/ })).not.toBeInTheDocument();
    const runAgainButton = screen.getByRole("button", { name: "Run again" });

    org.executeCompanyWorkOrder.mockResolvedValue({
      ok: true,
      status: "Completed",
      summary: "Recovered on retry.",
    });
    org.listCompanyWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-green", mission_intent: "Analyze README", track: "green", status: "Completed" }),
    ]);
    fireEvent.click(runAgainButton);

    await waitFor(() => expect(org.executeCompanyWorkOrder).toHaveBeenLastCalledWith("opc-live", "wo-green", { rerun: true }));
  });
});
