import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import EmployeeOffice from "../pages/EmployeeOffice";
import { setLanguage } from "../settings/i18n";

const api = vi.hoisted(() => ({
  listPromptVersions: vi.fn(),
  updateEmployee: vi.fn(),
  updateEmployeePrompt: vi.fn(),
  deleteEmployee: vi.fn(),
  getAgentGrowth: vi.fn(),
  approveSkillProposal: vi.fn(),
  modelChat: vi.fn(),
  runPlayground: vi.fn(),
}));

const org = vi.hoisted(() => ({
  getCompanyEmployee: vi.fn(),
}));

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

vi.mock("../api/client", () => ({
  listPromptVersions: api.listPromptVersions,
  updateEmployee: api.updateEmployee,
  updateEmployeePrompt: api.updateEmployeePrompt,
  deleteEmployee: api.deleteEmployee,
  getAgentGrowth: api.getAgentGrowth,
  approveSkillProposal: api.approveSkillProposal,
  modelChat: api.modelChat,
  runPlayground: api.runPlayground,
}));

vi.mock("../api/org", () => ({
  getCompanyEmployee: org.getCompanyEmployee,
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

function renderOffice(agentId = "agent-pm-01") {
  return render(
    <MemoryRouter initialEntries={[`/employees/${agentId}`]}>
      <Routes>
        <Route path="/employees/:agentId" element={<EmployeeOffice />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("Employee office (5 areas)", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-1");
    org.getCompanyEmployee.mockResolvedValue({
      agent_id: "agent-pm-01",
      display_name: "Product Lead",
      department: "product",
      lifecycle_status: "active",
      reputation: 0.62,
      system_prompt: "You are a product manager.",
    });
    api.listPromptVersions.mockResolvedValue([]);
    api.getAgentGrowth.mockResolvedValue({
      agent_id: "agent-pm-01", current_score: 62, direction: "improving",
      total_tasks: 5, completed_tasks: 4, failed_tasks: 1, success_rate: 80,
      avg_latency_ms: 1800, total_usage: 1540, total_cost_usd: 0.03,
      trend: [], pending_improvements: [],
    });
    api.modelChat.mockResolvedValue({ content: "hi", model: "gpt-4o" });
    api.runPlayground.mockRejectedValue(new Error("not ready"));
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows the employee summary and all five office tabs", async () => {
    renderOffice();

    expect(await screen.findByRole("heading", { name: "Product Lead" })).toBeInTheDocument();
    expect(screen.getByText("agent-pm-01")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Instructions/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Test bench/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Quality/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Replay/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Growth/i })).toBeInTheDocument();
    // Reputation surfaced from 0.62 -> 62
    expect(screen.getByText(/Reputation 62/)).toBeInTheDocument();
  });

  it("loads the employee through the company-scoped endpoint (company isolation)", async () => {
    renderOffice();

    await screen.findByRole("heading", { name: "Product Lead" });
    expect(org.getCompanyEmployee).toHaveBeenCalledWith("opc-1", "agent-pm-01");
  });

  it("opens on the instructions tab with the prompt editor", async () => {
    renderOffice();

    await screen.findByRole("heading", { name: "Product Lead" });
    expect(screen.getByText("System prompt")).toBeInTheDocument();
  });

  it("switches to the test bench (branded playground, multi-model compare)", async () => {
    renderOffice();

    fireEvent.click(await screen.findByRole("button", { name: /Test bench/i }));

    expect(screen.getByText("Test input")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Run comparison/i })).toBeInTheDocument();
  });

  it("switches to the growth tab and loads real growth data", async () => {
    renderOffice();

    fireEvent.click(await screen.findByRole("button", { name: /Growth/i }));

    await waitFor(() => expect(api.getAgentGrowth).toHaveBeenCalledWith("agent-pm-01"));
  });

  it("never shows the word Loop anywhere in the office", async () => {
    renderOffice();

    await screen.findByRole("heading", { name: "Product Lead" });
    expect(document.body.textContent).not.toMatch(/\bLoop\b/);
  });
});
