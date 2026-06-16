import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import Plans from "../pages/Plans";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  getActiveOpcId: vi.fn(),
}));

const org = vi.hoisted(() => ({
  listCompanyWorkOrders: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: companies.getActiveOpcId,
}));

vi.mock("../api/org", () => ({
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

describe("Plans page", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-live");
    org.listCompanyWorkOrders.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders persisted work orders instead of the empty execution plan placeholder", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-plan-1",
        mission_intent: "Draft launch plan",
        status: "Planned",
        track: "yellow",
        selected_agents: ["agent-founder-01"],
        required_skills: ["skill-mission-draft"],
      },
    ]);

    render(
      <MemoryRouter>
        <Plans />
      </MemoryRouter>,
    );

    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalledWith("opc-live"));
    expect(screen.getByText("Draft launch plan", { selector: ".product-row-main" })).toBeInTheDocument();
    expect(screen.getByText("Planned", { selector: ".mono-chip" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /tasks/i })).toHaveAttribute("href", "/work-orders");
    expect(screen.queryByText(/Route a compiled contract/i)).not.toBeInTheDocument();
  });
});
