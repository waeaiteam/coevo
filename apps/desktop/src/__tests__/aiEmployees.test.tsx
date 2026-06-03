import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import AIEmployees from "../pages/AIEmployees";
import { setLanguage } from "../settings/i18n";

function renderPage() {
  return render(
    <MemoryRouter>
      <AIEmployees />
    </MemoryRouter>,
  );
}

const api = vi.hoisted(() => ({
  getAgentMemory: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getAgentMemory: api.getAgentMemory,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
}));

describe("AI Employee passports", () => {
  beforeEach(() => {
    setLanguage("en");
    api.listEmployees.mockResolvedValue([
      {
        agent_id: "agent-founder-01",
        display_name: "Founder Chief of Staff",
        department: "founder_office",
        role: "FounderOffice",
        lifecycle_status: "active",
        risk_ceiling: 0.3,
        allowed_cognitive_layers: ["Hypothesis", "Suggestion"],
        allowed_action_modes: ["DRAFT_ONLY"],
        tool_scopes: ["urn:coevo:tool:read"],
        passport: {
          passport_id: "passport-agent-founder-01",
          issued_by: "coevo-seed",
          roles: ["FounderOffice"],
          capabilities: ["analysis", "planning"],
          restrictions: ["no production write", "no financial transfer"],
        },
        permission_boundary: {
          max_risk_score: 0.3,
          can_access_network: false,
          can_access_filesystem: false,
          can_call_external_executor: false,
          can_propose_skill: true,
          can_write_decision: false,
          can_write_fact: false,
        },
      },
    ]);
    api.seedEmployees.mockResolvedValue({ ok: true, inserted: 1, total: 1 });
    api.getAgentMemory.mockResolvedValue({
      agent_id: "agent-founder-01",
      working_preferences: "Read company rules before every mission.",
      learned_constraints: ["Never bypass Red Track."],
      recurring_failures: ["Over-scoped task briefs"],
      successful_patterns: ["Ask for approval before Yellow execution"],
      recent_tasks: ["Draft onboarding summary"],
      performance_notes: "Strong at founder-facing synthesis.",
      skill_usage_stats: "analysis: 4, planning: 3",
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a founder-readable passport, memory, and permission boundary for a selected employee", async () => {
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: /Founder Chief of Staff/i }));

    await waitFor(() => expect(api.getAgentMemory).toHaveBeenCalledWith("agent-founder-01"));
    expect(screen.getByText("AI Employee Passport")).toBeInTheDocument();
    expect(screen.getByText("passport-agent-founder-01")).toBeInTheDocument();
    expect(screen.getByText("Capabilities")).toBeInTheDocument();
    expect(screen.getByText("analysis")).toBeInTheDocument();
    expect(screen.getByText("no production write")).toBeInTheDocument();
    expect(screen.getAllByText(/Role: Founder Office/).length).toBeGreaterThan(0);
    expect(document.body.textContent).not.toContain("Role: FounderOffice");
    expect(screen.getByText("Default view")).toBeInTheDocument();
    expect(screen.getByText("Advanced identity details")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Advanced identity details"));
    expect(screen.getByText("Permission Boundary")).toBeInTheDocument();
    expect(screen.getByText("Max risk 0.3")).toBeInTheDocument();
    expect(screen.getByText("Network blocked")).toBeInTheDocument();
    expect(screen.getByText("Agent Memory")).toBeInTheDocument();
    expect(screen.getByText("Read company rules before every mission.")).toBeInTheDocument();
    expect(screen.getByText("Draft onboarding summary")).toBeInTheDocument();
  });

  it("preserves the seed action for initializing the default employee team", async () => {
    api.listEmployees.mockResolvedValueOnce([]).mockResolvedValueOnce([
      {
        agent_id: "agent-founder-01",
        display_name: "Founder Chief of Staff",
        department: "founder_office",
        lifecycle_status: "active",
        risk_ceiling: 0.3,
      },
    ]);

    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "Seed 10 AI Employees" }));

    await waitFor(() => expect(api.seedEmployees).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Inserted: 1, Total: 1")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Founder Chief of Staff/i })).toBeInTheDocument();
  });

  it("shows a neutral empty state when an employee has no memory yet", async () => {
    const notFound = Object.assign(new Error("not found"), { status: 404 });
    api.getAgentMemory.mockRejectedValue(notFound);

    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: /Founder Chief of Staff/i }));

    expect(await screen.findByText("No agent memory recorded yet.")).toBeInTheDocument();
    expect(screen.queryByText(/Memory unavailable/i)).not.toBeInTheDocument();
  });

  it("localizes passport and memory labels in Chinese mode", async () => {
    setLanguage("zh");
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: /Founder Chief of Staff/i }));

    expect(await screen.findByText("AI 员工护照")).toBeInTheDocument();
    expect(screen.getByText("护照 ID")).toBeInTheDocument();
    expect(screen.getByText("能力")).toBeInTheDocument();
    expect(screen.getByText("安全边界: 0.3")).toBeInTheDocument();
    expect(screen.getByText("网络 已阻断")).toBeInTheDocument();
    expect(screen.getByText("工作偏好")).toBeInTheDocument();
    expect(screen.getAllByText("近期任务").length).toBeGreaterThan(0);

    expect(document.body.textContent).not.toMatch(/Passport ID|Capabilities|Network blocked|Working preference|Recent tasks/);
  });
});
