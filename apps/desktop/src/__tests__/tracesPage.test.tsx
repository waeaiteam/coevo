import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Traces from "../pages/Traces";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const api = vi.hoisted(() => ({
  listCompanyTraces: vi.fn(),
  getCompanyTraceSpans: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/client", () => ({
  listCompanyTraces: api.listCompanyTraces,
  getCompanyTraceSpans: api.getCompanyTraceSpans,
}));

describe("Traces page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    api.listCompanyTraces.mockResolvedValue([]);
    api.getCompanyTraceSpans.mockResolvedValue({ spans: [] });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("loads traces from the company trace endpoints instead of synthesizing demo spans", async () => {
    api.listCompanyTraces.mockResolvedValue([
      {
        trace_id: "run-1",
        work_order_id: "wo-1",
        status: "completed",
        started_at_ms: 100,
        ended_at_ms: 250,
      },
    ]);
    api.getCompanyTraceSpans.mockResolvedValue({
      spans: [
        {
          span_id: "root",
          parent_span_id: null,
          name: "Task replay",
          kind: "mission",
          status: "ok",
          started_at_ms: 100,
          ended_at_ms: 250,
        },
      ],
    });

    render(<Traces />);

    await waitFor(() => expect(api.listCompanyTraces).toHaveBeenCalledWith("opc-live"));
    expect(screen.getByText("wo-1")).toBeInTheDocument();
    await waitFor(() => expect(api.getCompanyTraceSpans).toHaveBeenCalledWith("opc-live", "run-1"));
    expect(await screen.findByText("Task replay")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Show sample/i })).not.toBeInTheDocument();
  });

  it("keeps the trace list empty until the backend returns real records", async () => {
    render(<Traces />);

    await waitFor(() => expect(api.listCompanyTraces).toHaveBeenCalledTimes(1));
    expect(screen.getByText("No task records captured yet.")).toBeInTheDocument();
  });
});
