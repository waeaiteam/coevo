import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import MeetingRoom from "../pages/MeetingRoom";
import PerformanceBoard from "../pages/PerformanceBoard";
import OperatingReports from "../pages/OperatingReports";
import CostManagement from "../pages/CostManagement";
import { setLanguage } from "../settings/i18n";

const org = vi.hoisted(() => ({
  listMeetings: vi.fn(),
  getMeeting: vi.fn(),
  startMeeting: vi.fn(),
  listKpi: vi.fn(),
  listReports: vi.fn(),
  getReport: vi.fn(),
  generateReport: vi.fn(),
  getCost: vi.fn(),
  setCostQuota: vi.fn(),
}));

const client = vi.hoisted(() => ({
  listEmployees: vi.fn(),
  getAgentGrowth: vi.fn(),
}));

const companies = vi.hoisted(() => ({ getActiveOpcId: vi.fn() }));

vi.mock("../api/org", () => org);
vi.mock("../api/client", () => client);
vi.mock("../api/companies", () => companies);

function renderAt(path: string, pattern: string, element: JSX.Element) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path={pattern} element={element} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("org ecosystem surfaces", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-1");
    org.listMeetings.mockResolvedValue([
      { meeting_id: "mtg-1", topic: "Ship the redesign?", status: "completed", created_at_ms: 1 },
    ]);
    org.getMeeting.mockResolvedValue({
      meeting_id: "mtg-1", topic: "Ship the redesign?", status: "completed",
      agenda: "Decide go / no-go.",
      transcript: [
        { agent_id: "agent-product-01", stance: "for", text: "Ship it Thursday." },
        { agent_id: "agent-risk-01", stance: "against", text: "No rollback path." },
      ],
      resolution_md: "## Decision\nShip behind a flag.",
      responsibility_anchor: "agent-product-01",
      created_at_ms: 1,
    });
    org.startMeeting.mockResolvedValue({ meeting_id: "mtg-2", status: "running" });
    org.listKpi.mockResolvedValue([
      { work_order_id: "wo-1", scores: { completion: 88, speed: 76, clarity: 92, compliance: 95 }, reviewer: "agent-founder-01", created_at_ms: 2 },
    ]);
    org.listReports.mockResolvedValue([{ report_id: "rep-1", period: "daily", created_at_ms: 3 }]);
    org.getReport.mockResolvedValue({
      report_id: "rep-1", period: "daily", report_md: "# Daily briefing\nAll good.",
      kpi_summary: [{ department: "product", score: 88 }],
      token_usage: [{ department: "product", tokens: 142000, cost_usd: 1.84 }],
      alerts: [{ severity: "warning", message: "Research usage is high." }],
      created_at_ms: 3,
    });
    org.generateReport.mockResolvedValue({ report_id: "rep-2" });
    org.getCost.mockResolvedValue({
      by_department: [
        { dept: "product", tokens: 142000, cost_usd: 1.84, quota: 300000 },
        { dept: "research", tokens: 318900, cost_usd: 4.12, quota: 250000 },
      ],
      total: 5.96,
    });
    org.setCostQuota.mockResolvedValue({ ok: true });
    client.listEmployees.mockResolvedValue([
      { agent_id: "agent-pm-01", display_name: "Product Lead", department: "product", lifecycle_status: "active", reputation: 0.62 },
    ]);
    client.getAgentGrowth.mockResolvedValue({
      agent_id: "agent-pm-01", current_score: 62, direction: "improving",
      total_tasks: 5, success_rate: 80, avg_latency_ms: 1800, total_usage: 1540, trend: [], pending_improvements: [],
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("meeting room shows the debate and resolution of a meeting", async () => {
    renderAt("/companies/opc-1/meetings", "/companies/:opcId/meetings", <MeetingRoom />);

    expect(await screen.findByText("Ship it Thursday.")).toBeInTheDocument();
    expect(screen.getByText("No rollback path.")).toBeInTheDocument();
    expect(screen.getByText("Debate")).toBeInTheDocument();
    expect(screen.getByText("Resolution")).toBeInTheDocument();
    expect(screen.getAllByText(/agent-product-01/).length).toBeGreaterThan(0);
  });

  it("meeting room can raise a new topic", async () => {
    renderAt("/companies/opc-1/meetings", "/companies/:opcId/meetings", <MeetingRoom />);

    await screen.findByText("Debate");
    fireEvent.change(screen.getByPlaceholderText("What should the team decide?"), { target: { value: "New direction?" } });
    fireEvent.click(screen.getByRole("button", { name: /Start meeting/i }));

    await waitFor(() => expect(org.startMeeting).toHaveBeenCalledTimes(1));
    expect(org.startMeeting.mock.calls[0][1]).toMatchObject({ topic: "New direction?", close_mode: "chair" });
  });

  it("performance board shows KPI scores for the selected employee", async () => {
    renderAt("/companies/opc-1/performance", "/companies/:opcId/performance", <PerformanceBoard />);

    await waitFor(() => expect(org.listKpi).toHaveBeenCalledWith("opc-1", "agent-pm-01"));
    expect(await screen.findByText("Latest scores")).toBeInTheDocument();
    expect(screen.getByText("Promotion status")).toBeInTheDocument();
  });

  it("operating briefings render the read-out, scores, and alerts", async () => {
    renderAt("/companies/opc-1/reports", "/companies/:opcId/reports", <OperatingReports />);

    await waitFor(() => expect(org.getReport).toHaveBeenCalled());
    expect(await screen.findByText("Department scores")).toBeInTheDocument();
    expect(screen.getByText("Research usage is high.")).toBeInTheDocument();
  });

  it("cost management shows budgets and can save a new quota", async () => {
    renderAt("/companies/opc-1/cost", "/companies/:opcId/cost", <CostManagement />);

    await waitFor(() => expect(org.getCost).toHaveBeenCalledWith("opc-1"));
    expect(await screen.findByText("Total cost")).toBeInTheDocument();
    const saveButtons = screen.getAllByRole("button", { name: /Save budget/i });
    fireEvent.click(saveButtons[0]);
    await waitFor(() => expect(org.setCostQuota).toHaveBeenCalledTimes(1));
  });
});
