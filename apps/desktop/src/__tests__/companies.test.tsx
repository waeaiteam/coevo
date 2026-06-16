import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import MyCompany from "../pages/MyCompany";
import Office from "../pages/Office";
import { setLanguage } from "../settings/i18n";

const companies = vi.hoisted(() => ({
  listCompanies: vi.fn(),
  createCompany: vi.fn(),
  deleteCompany: vi.fn(),
  getActiveOpcId: vi.fn(),
  setActiveOpcId: vi.fn(),
}));

const client = vi.hoisted(() => ({
  listEmployees: vi.fn(),
}));

vi.mock("../api/companies", () => ({
  listCompanies: companies.listCompanies,
  createCompany: companies.createCompany,
  deleteCompany: companies.deleteCompany,
  getActiveOpcId: companies.getActiveOpcId,
  setActiveOpcId: companies.setActiveOpcId,
}));

vi.mock("../api/client", () => ({
  listEmployees: client.listEmployees,
}));

describe("multi-company drilldown", () => {
  beforeEach(() => {
    setLanguage("en");
    companies.getActiveOpcId.mockReturnValue("opc-1");
    companies.listCompanies.mockResolvedValue([
      { opc_id: "opc-1", name: "Northwind Studio", mission: "Ship delight", employee_count: 4, created_at_ms: 0, dir: "~/.coevo/opc-1" },
      { opc_id: "opc-2", name: "Second Co", mission: "", employee_count: 0, created_at_ms: 1, dir: "~/.coevo/opc-2" },
    ]);
    companies.createCompany.mockResolvedValue({ opc_id: "opc-new", name: "Fresh Co", mission: "", employee_count: 0, created_at_ms: 2, dir: "~/.coevo/opc-new" });
    companies.deleteCompany.mockResolvedValue({ ok: true });
    client.listEmployees.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("lists the founder's companies and marks the current one", async () => {
    render(<MemoryRouter><MyCompany /></MemoryRouter>);

    expect(await screen.findByRole("heading", { name: "Northwind Studio" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Second Co" })).toBeInTheDocument();
    expect(screen.getByText("Current")).toBeInTheDocument();
    expect(screen.getByText("4 AI employees")).toBeInTheDocument();
  });

  it("creates a new company through the create flow", async () => {
    render(<MemoryRouter><MyCompany /></MemoryRouter>);

    fireEvent.click(await screen.findByRole("button", { name: /New company/i }));
    fireEvent.change(screen.getByPlaceholderText("e.g. Northwind Studio"), { target: { value: "Fresh Co" } });
    fireEvent.click(screen.getByRole("button", { name: /^Create company$/i }));

    await waitFor(() => expect(companies.createCompany).toHaveBeenCalledTimes(1));
    expect(companies.createCompany).toHaveBeenCalledWith({ name: "Fresh Co", mission: undefined });
    expect(companies.setActiveOpcId).toHaveBeenCalledWith("opc-new");
  });

  it("requires confirmation before deleting a company", async () => {
    render(<MemoryRouter><MyCompany /></MemoryRouter>);

    await screen.findByRole("heading", { name: "Second Co" });
    const deleteButtons = screen.getAllByRole("button", { name: /^Delete company$/i });
    fireEvent.click(deleteButtons[0]);

    expect(companies.deleteCompany).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Yes, delete/i }));

    await waitFor(() => expect(companies.deleteCompany).toHaveBeenCalledTimes(1));
  });

  it("renders the office as an org chart with founder and department nodes", async () => {
    client.listEmployees.mockResolvedValue([
      { agent_id: "agent-pm-01", display_name: "Product Lead", department: "product", lifecycle_status: "active" },
      { agent_id: "agent-eng-01", display_name: "Builder", department: "engineering", lifecycle_status: "idle" },
    ]);

    render(
      <MemoryRouter initialEntries={["/companies/opc-1/office"]}>
        <Routes>
          <Route path="/companies/:opcId/office" element={<Office />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(await screen.findByText("Product Lead")).toBeInTheDocument();
    expect(screen.getByText("Builder")).toBeInTheDocument();
    expect(screen.getByText("Founder")).toBeInTheDocument();
    expect(screen.getByText("Product")).toBeInTheDocument();
    expect(screen.getByText("Engineering")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Product Lead/i })).toHaveAttribute("href", "/employees/agent-pm-01");
  });

  it("invites hiring when the office is empty", async () => {
    render(
      <MemoryRouter initialEntries={["/companies/opc-1/office"]}>
        <Routes>
          <Route path="/companies/:opcId/office" element={<Office />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(await screen.findByText("No employees in this company yet. Hire some from the talent market.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /talent market/i })).toHaveAttribute("href", "/market");
  });
});
