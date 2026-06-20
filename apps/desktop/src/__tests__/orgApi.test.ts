import { beforeEach, describe, expect, it, vi } from "vitest";

const client = vi.hoisted(() => ({
  get: vi.fn(),
  post: vi.fn(),
  put: vi.fn(),
  decideWorkOrderApproval: vi.fn(),
  listEmployees: vi.fn(),
  listMemory: vi.fn(),
  listWorkOrders: vi.fn(),
  listConversations: vi.fn(),
  createConversation: vi.fn(),
  listConversationMessages: vi.fn(),
  appendConversationMessage: vi.fn(),
  getCompanyProfile: vi.fn(),
  getAgentGrowth: vi.fn(),
  approveSkillProposal: vi.fn(),
}));

vi.mock("../api/client", () => client);

async function loadOrg() {
  vi.resetModules();
  vi.doMock("../api/client", () => client);
  return import("../api/org");
}

describe("company scoped API isolation", () => {
  beforeEach(() => {
    for (const fn of Object.values(client)) fn.mockReset();
  });

  it("does not fall back to legacy /opc work orders when company listing fails", async () => {
    client.get.mockRejectedValue(new Error("company db unavailable"));
    const { listCompanyWorkOrders } = await loadOrg();

    await expect(listCompanyWorkOrders("opc-alpha")).rejects.toThrow("company db unavailable");
    expect(client.listWorkOrders).not.toHaveBeenCalled();
  });

  it("passes the company opc id as request-scoped header metadata", async () => {
    client.get.mockResolvedValue([]);
    const { listCompanyWorkOrders } = await loadOrg();

    await listCompanyWorkOrders("opc-alpha");

    expect(client.get).toHaveBeenCalledWith("/companies/opc-alpha/work-orders", { opcId: "opc-alpha" });
  });

  it("does not execute legacy /opc work orders when company execution fails", async () => {
    client.post.mockRejectedValue(new Error("company execution unavailable"));
    const { executeCompanyWorkOrder } = await loadOrg();

    await expect(executeCompanyWorkOrder("opc-alpha", "wo-1", {})).rejects.toThrow("company execution unavailable");
    expect(client.post).toHaveBeenCalledTimes(1);
    expect(client.post.mock.calls[0][0]).toBe("/companies/opc-alpha/work-orders/wo-1/execute");
  });

  it("does not approve through legacy /opc when company approval fails", async () => {
    client.post.mockRejectedValue(new Error("company approval unavailable"));
    const { decideCompanyWorkOrderApproval } = await loadOrg();

    await expect(
      decideCompanyWorkOrderApproval("opc-alpha", "wo-1", {
        approval_id: "approval-1",
        decision: "approve",
      }),
    ).rejects.toThrow("company approval unavailable");
    expect(client.decideWorkOrderApproval).not.toHaveBeenCalled();
  });

  it("does not fall back to global memory when company memory fails", async () => {
    client.get.mockRejectedValue(new Error("company memory unavailable"));
    const { listCompanyMemory } = await loadOrg();

    await expect(listCompanyMemory("opc-alpha")).rejects.toThrow("company memory unavailable");
    expect(client.listMemory).not.toHaveBeenCalled();
  });

  it("does not fall back to legacy employees or profile when company reads fail", async () => {
    client.get.mockRejectedValue(new Error("company read unavailable"));
    const { listCompanyEmployees, getCompanyProfileById } = await loadOrg();

    await expect(listCompanyEmployees("opc-alpha")).rejects.toThrow("company read unavailable");
    await expect(getCompanyProfileById("opc-alpha")).rejects.toThrow("company read unavailable");
    expect(client.listEmployees).not.toHaveBeenCalled();
    expect(client.getCompanyProfile).not.toHaveBeenCalled();
  });

  it("does not fall back to legacy conversations when company conversation APIs fail", async () => {
    client.get.mockRejectedValue(new Error("company conversation unavailable"));
    client.post.mockRejectedValue(new Error("company conversation unavailable"));
    const {
      listCompanyConversations,
      createCompanyConversation,
      listCompanyConversationMessages,
      appendCompanyConversationMessage,
    } = await loadOrg();

    await expect(listCompanyConversations("opc-alpha")).rejects.toThrow("company conversation unavailable");
    await expect(createCompanyConversation("opc-alpha", { title: "A" })).rejects.toThrow("company conversation unavailable");
    await expect(listCompanyConversationMessages("opc-alpha", "conv-1")).rejects.toThrow("company conversation unavailable");
    await expect(appendCompanyConversationMessage("opc-alpha", "conv-1", { content: "hi" })).rejects.toThrow("company conversation unavailable");
    expect(client.listConversations).not.toHaveBeenCalled();
    expect(client.createConversation).not.toHaveBeenCalled();
    expect(client.listConversationMessages).not.toHaveBeenCalled();
    expect(client.appendConversationMessage).not.toHaveBeenCalled();
  });

  it("does not fall back to legacy work-order mutation or audit endpoints", async () => {
    client.get.mockRejectedValue(new Error("company audit unavailable"));
    client.post.mockRejectedValue(new Error("company mutation unavailable"));
    const {
      cancelCompanyWorkOrder,
      submitCompanyWorkOrderFeedback,
      getCompanyWorkOrderAuditExport,
      listCompanyAuditEvents,
    } = await loadOrg();

    await expect(cancelCompanyWorkOrder("opc-alpha", "wo-1")).rejects.toThrow("company mutation unavailable");
    await expect(submitCompanyWorkOrderFeedback("opc-alpha", "wo-1", "ok")).rejects.toThrow("company mutation unavailable");
    await expect(getCompanyWorkOrderAuditExport("opc-alpha", "wo-1")).rejects.toThrow("company audit unavailable");
    await expect(listCompanyAuditEvents("opc-alpha")).rejects.toThrow("company audit unavailable");
    expect(client.post).toHaveBeenCalledTimes(2);
    expect(client.get).toHaveBeenCalledTimes(2);
  });
});