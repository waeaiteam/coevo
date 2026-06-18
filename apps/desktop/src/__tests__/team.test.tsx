import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import Team from "../pages/Team";
import { setLanguage } from "../settings/i18n";
import { setAdvancedMode } from "../settings/appMode";

const api = vi.hoisted(() => ({
  listEmployees: vi.fn(),
}));

vi.mock("../api/client", () => ({
  listEmployees: api.listEmployees,
  // AIEmployees (rendered in advanced mode) imports more; provide no-op stubs.
  createEmployee: vi.fn(),
  getAgentMemory: vi.fn(),
  seedEmployees: vi.fn(),
}));

function renderTeam() {
  return render(
    <MemoryRouter>
      <Team />
    </MemoryRouter>,
  );
}

describe("Team page org chart", () => {
  beforeEach(() => {
    setLanguage("en");
    setAdvancedMode(false);
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-secretary-01", display_name: "Secretary", department: "FounderOffice", role: "Secretary", lifecycle_status: "active", supervisor_agent_id: null },
      { agent_id: "agent-pm-01", display_name: "Product Manager", department: "Product", role: "Product", lifecycle_status: "active", supervisor_agent_id: "agent-founder-01" },
      { agent_id: "sub-agent-pm-01-abcd", display_name: "PM Helper", department: "Product", role: "Product", lifecycle_status: "active", supervisor_agent_id: "agent-pm-01" },
    ]);
  });

  afterEach(() => {
    cleanup();
    setAdvancedMode(false);
    vi.restoreAllMocks();
  });

  it("shows the secretary as the dispatcher node, not inside a department", async () => {
    renderTeam();
    // The secretary node renders with the Secretary badge and dispatcher description.
    expect(await screen.findByText("Understands your request and dispatches it to the right department.")).toBeInTheDocument();
    const secretaryLink = screen.getByRole("link", { name: /Secretary/i });
    expect(secretaryLink).toHaveAttribute("href", "/employees/agent-secretary-01");
  });

  it("nests a head's helper as a sub-agent under the head", async () => {
    renderTeam();
    expect(await screen.findByText("PM Helper")).toBeInTheDocument();
    // The helper reports to the Product Manager (a non-founder head) → tagged Sub-agent.
    const subBadges = screen.getAllByText("Sub-agent");
    expect(subBadges.length).toBeGreaterThan(0);
    // The department head itself reports to the founder, not tagged as a sub-agent.
    const pmRow = screen.getByText("Product Manager").closest("a");
    expect(pmRow && within(pmRow).queryByText("Sub-agent")).toBeNull();
  });
});
