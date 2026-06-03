import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import TalentMarket from "../pages/TalentMarket";
import { setLanguage } from "../settings/i18n";

const api = vi.hoisted(() => ({
  createEmployee: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
}));

vi.mock("../api/client", () => ({
  createEmployee: api.createEmployee,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
}));

function renderPage() {
  return render(
    <MemoryRouter>
      <TalentMarket />
    </MemoryRouter>,
  );
}

describe("AI Talent Market", () => {
  beforeEach(() => {
    setLanguage("en");
    api.listEmployees.mockResolvedValue([]);
    api.createEmployee.mockResolvedValue({ agent_id: "agent-product-abcd" });
    api.seedEmployees.mockResolvedValue({ ok: true, inserted: 8, total: 8 });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("presents ready-made roles the founder can hire", async () => {
    renderPage();

    expect(await screen.findByRole("heading", { name: "AI Talent Market" })).toBeInTheDocument();
    expect(screen.getByText("Ready-made roles")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /Hire$/i }).length).toBeGreaterThan(0);
  });

  it("hiring a role creates an employee through the existing endpoint", async () => {
    renderPage();

    const hireButtons = await screen.findAllByRole("button", { name: /^Hire$/i });
    fireEvent.click(hireButtons[0]);

    await waitFor(() => expect(api.createEmployee).toHaveBeenCalledTimes(1));
    const payload = api.createEmployee.mock.calls[0][0];
    expect(payload).toHaveProperty("agent_id");
    expect(payload).toHaveProperty("department");
    expect(payload.lifecycle_status).toBe("active");
  });

  it("hiring the starter team seeds the default company team", async () => {
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "Hire the starter team" }));

    await waitFor(() => expect(api.seedEmployees).toHaveBeenCalledTimes(1));
  });

  it("marks roles already on the team", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-product-01", department: "product", lifecycle_status: "active" },
    ]);

    renderPage();

    expect(await screen.findByText("On the team")).toBeInTheDocument();
  });
});
