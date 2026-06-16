import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import RiskGate from "../pages/RiskGate";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const org = vi.hoisted(() => ({
  listCompanyAuditEvents: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/org", () => ({
  listCompanyAuditEvents: org.listCompanyAuditEvents,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

describe("RiskGate page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    org.listCompanyAuditEvents.mockResolvedValue([]);
    org.listCompanyWorkOrders.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows real approval and audit data from work-order governance records", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-risk-1",
        mission_intent: "Send customer update",
        status: "WaitingApproval",
        track: "yellow",
      },
    ]);
    org.listCompanyAuditEvents.mockResolvedValue([
      {
        id: "audit-1",
        event_type: "work_order.approval_requested",
        tenant_id: "tenant-1",
        event_data_json: JSON.stringify({ work_order_id: "wo-risk-1", decision: "pending" }),
        recorded_at_ms: 1000,
      },
    ]);

    render(
      <MemoryRouter>
        <RiskGate />
      </MemoryRouter>,
    );

    await waitFor(() => expect(org.listCompanyAuditEvents).toHaveBeenCalledWith("opc-live", expect.any(Object)));
    expect(screen.getByText("Send customer update", { selector: ".product-row-main" })).toBeInTheDocument();
    expect(screen.getByText("WaitingApproval", { selector: ".mono-chip" })).toBeInTheDocument();
    expect(screen.getByText(/approval_requested/i)).toBeInTheDocument();
    expect(screen.queryByText(/Rule-first, score-second/i)).not.toBeInTheDocument();
  });
});
