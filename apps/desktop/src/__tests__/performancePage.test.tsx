import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Performance from "../pages/Performance";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const api = vi.hoisted(() => ({
  listCompanyTraces: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/client", () => ({
  listCompanyTraces: api.listCompanyTraces,
}));

describe("Performance page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    api.listCompanyTraces.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("loads live company trace rows and derives the summary metrics from them", async () => {
    api.listCompanyTraces.mockResolvedValue([
      {
        trace_id: "trace-1",
        status: "ok",
        started_at_ms: 100,
        ended_at_ms: 200,
        total_tokens: 120,
        total_cost_usd: 0.25,
      },
      {
        trace_id: "trace-2",
        status: "error",
        started_at_ms: 200,
        ended_at_ms: 400,
        total_tokens: 80,
        total_cost_usd: 0.5,
      },
      {
        trace_id: "trace-3",
        status: "ok",
        duration_ms: 400,
        total_tokens: 30,
        total_cost_usd: 0.1,
      },
    ]);

    render(<Performance />);

    await waitFor(() => expect(api.listCompanyTraces).toHaveBeenCalledWith("opc-live"));
    expect(await screen.findByText("3")).toBeInTheDocument();
    expect(screen.getByText("200ms")).toBeInTheDocument();
    expect(screen.getAllByText("400ms")).toHaveLength(2);
    expect(screen.getByText("$0.8500")).toBeInTheDocument();
    expect(screen.getByText("33%")).toBeInTheDocument();
    expect(screen.queryByText("No activity yet. Once your employees handle tasks, a summary appears here.")).not.toBeInTheDocument();
  });

  it("shows an empty state when the backend returns no company traces", async () => {
    render(<Performance />);

    await waitFor(() => expect(api.listCompanyTraces).toHaveBeenCalledTimes(1));
    expect(screen.getByText("No activity yet. Once your employees handle tasks, a summary appears here.")).toBeInTheDocument();
  });
});
