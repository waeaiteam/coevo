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

  it("submits Yellow work for approval instead of implying direct execution", async () => {
    api.listWorkOrders.mockResolvedValue([
      workOrder({ work_order_id: "wo-yellow", mission_intent: "Draft announcement", track: "yellow" }),
    ]);
    api.executeWorkOrder.mockResolvedValue({ ok: true, status: "WaitingApproval" });

    render(<WorkOrders />);

    fireEvent.click(await screen.findByRole("button", { name: "Submit for Approval" }));

    await waitFor(() => expect(api.executeWorkOrder).toHaveBeenCalledWith("wo-yellow", {}));
    expect(screen.getByText(/Submit for Approval:.*WaitingApproval/)).toBeInTheDocument();
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
