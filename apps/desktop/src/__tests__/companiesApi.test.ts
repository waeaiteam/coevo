import { beforeEach, describe, expect, it, vi } from "vitest";

const client = vi.hoisted(() => ({
  get: vi.fn(),
  post: vi.fn(),
  del: vi.fn(),
  getCompanyProfile: vi.fn(),
  listEmployees: vi.fn(),
  listMemory: vi.fn(),
  listWorkOrders: vi.fn(),
}));

vi.mock("../api/client", () => ({
  get: client.get,
  post: client.post,
  del: client.del,
  getCompanyProfile: client.getCompanyProfile,
  listEmployees: client.listEmployees,
  listMemory: client.listMemory,
  listWorkOrders: client.listWorkOrders,
}));

async function loadCompanies() {
  vi.resetModules();
  vi.doMock("../api/client", () => ({
    get: client.get,
    post: client.post,
    del: client.del,
    getCompanyProfile: client.getCompanyProfile,
    listEmployees: client.listEmployees,
    listMemory: client.listMemory,
    listWorkOrders: client.listWorkOrders,
  }));
  return import("../api/companies");
}

describe("company mutation fallback", () => {
  beforeEach(() => {
    localStorage.clear();
    client.get.mockReset();
    client.post.mockReset();
    client.del.mockReset();
    client.getCompanyProfile.mockReset();
    client.listEmployees.mockReset();
    client.listMemory.mockReset();
    client.listWorkOrders.mockReset();
  });

  it("does not create a local company when the backend create call fails", async () => {
    client.post.mockRejectedValue(new Error("backend unavailable"));

    const { createCompany } = await loadCompanies();

    await expect(createCompany({ name: "Fresh Co" })).rejects.toThrow("backend unavailable");
    expect(localStorage.getItem("coevo-local-companies")).toBeNull();
  });

  it("does not remove the active company from local storage when backend delete fails", async () => {
    localStorage.setItem("coevo-local-companies", JSON.stringify([
      {
        opc_id: "opc-1",
        name: "Northwind Studio",
        mission: "",
        employee_count: 4,
        created_at_ms: 1,
        dir: "~/.coevo/opc-1",
      },
    ]));
    localStorage.setItem("coevo-opc-id", "opc-1");
    client.del.mockRejectedValue(new Error("backend unavailable"));

    const { deleteCompany } = await loadCompanies();

    await expect(deleteCompany("opc-1")).rejects.toThrow("backend unavailable");
    expect(JSON.parse(localStorage.getItem("coevo-local-companies") || "[]")).toHaveLength(1);
    expect(localStorage.getItem("coevo-opc-id")).toBe("opc-1");
  });
});
