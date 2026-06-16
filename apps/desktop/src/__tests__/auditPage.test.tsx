import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Audit from "../pages/Audit";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const org = vi.hoisted(() => ({
  listCompanyAuditEvents: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/org", () => ({
  listCompanyAuditEvents: org.listCompanyAuditEvents,
}));

describe("Audit page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    org.listCompanyAuditEvents.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("loads company audit events and shows the selected event payload", async () => {
    org.listCompanyAuditEvents.mockResolvedValue([
      {
        id: "audit-2",
        event_type: "worker.tool.end",
        contract_hash: "contract-2",
        agent_id: "agent-pm-01",
        traceparent: null,
        tenant_id: "opc-live",
        event_data_json: JSON.stringify({
          work_order_id: "wo-2",
          run_id: "run-2",
          tool_id: "urn:mcp:files:read",
          success: true,
        }),
        recorded_at_ms: 1710000000100,
      },
      {
        id: "audit-1",
        event_type: "worker.governance",
        contract_hash: "contract-1",
        agent_id: "agent-founder-01",
        traceparent: null,
        tenant_id: "opc-live",
        event_data_json: JSON.stringify({
          work_order_id: "wo-1",
          run_id: "run-1",
          round: 1,
        }),
        recorded_at_ms: 1710000000000,
      },
    ]);

    render(<Audit />);

    await waitFor(() =>
      expect(org.listCompanyAuditEvents).toHaveBeenCalledWith("opc-live", { limit: 100 }),
    );
    expect(screen.getAllByText("worker.tool.end").length).toBeGreaterThan(0);
    expect(screen.getByText("wo-2")).toBeInTheDocument();
    expect(await screen.findByText(/urn:mcp:files:read/)).toBeInTheDocument();
    expect(screen.getByText("agent-pm-01")).toBeInTheDocument();
  });
});
