import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import Workflows from "../pages/Workflows";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const api = vi.hoisted(() => ({
  getCompanyTraceSpans: vi.fn(),
  listCompanyTraces: vi.fn(),
}));

const org = vi.hoisted(() => ({
  listCompanyWorkOrders: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/client", () => ({
  getCompanyTraceSpans: api.getCompanyTraceSpans,
  listCompanyTraces: api.listCompanyTraces,
}));

vi.mock("../api/org", () => ({
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

describe("Workflows page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    api.getCompanyTraceSpans.mockResolvedValue({ spans: [] });
    api.listCompanyTraces.mockResolvedValue([]);
    org.listCompanyWorkOrders.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows real operational traces and work orders instead of the local-only workflow editor", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-flow-1",
        mission_intent: "Publish release note",
        status: "Completed",
        track: "green",
      },
    ]);
    api.listCompanyTraces.mockResolvedValue([
      {
        trace_id: "trace-1",
        work_order_id: "wo-flow-1",
        status: "completed",
        started_at_ms: 100,
        ended_at_ms: 220,
      },
    ]);
    api.getCompanyTraceSpans.mockResolvedValue({
      spans: [
        {
          span_id: "span-1",
          parent_span_id: null,
          name: "Publish release note",
          kind: "mission",
          status: "ok",
          started_at_ms: 100,
          ended_at_ms: 220,
        },
      ],
    });

    render(
      <MemoryRouter>
        <Workflows />
      </MemoryRouter>,
    );

    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalledWith("opc-live"));
    expect(screen.getByText("Publish release note", { selector: ".product-row-main" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /tasks/i })).toHaveAttribute("href", "/work-orders");
    expect(screen.queryByText(/not connected to a backend runner/i)).not.toBeInTheDocument();
    expect(screen.getByText("Trace waterfall")).toBeInTheDocument();
  });
});
