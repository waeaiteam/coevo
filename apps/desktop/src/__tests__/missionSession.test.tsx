import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { GovernanceProvider } from "../hooks/useGovernance";
import MissionChat from "../pages/MissionChat";
import Sidebar from "../components/Sidebar";
import { setLanguage } from "../settings/i18n";
import { missionStateKey, readActiveConversationId } from "../utils/missionSession";

const client = vi.hoisted(() => ({
  compileContract: vi.fn(),
  routePlan: vi.fn(),
  executeWorkOrder: vi.fn(),
  modelChat: vi.fn(),
  streamWorkerRunEvents: vi.fn(() => () => {}),
}));

const org = vi.hoisted(() => ({
  appendCompanyConversationMessage: vi.fn(),
  createCompanyConversation: vi.fn(),
  createCompanyWorkOrder: vi.fn(),
  dispatchPlan: vi.fn(),
  executeCompanyWorkOrder: vi.fn(),
  getCompanyProfileById: vi.fn(),
  listCompanyConversationMessages: vi.fn(),
  listCompanyConversations: vi.fn(),
  listCompanyEmployees: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
}));

const bootstrap = vi.hoisted(() => ({
  ensureWorkspaceDefaults: vi.fn(),
}));

vi.mock("../api/client", () => ({
  compileContract: client.compileContract,
  routePlan: client.routePlan,
  executeWorkOrder: client.executeWorkOrder,
  modelChat: client.modelChat,
  streamWorkerRunEvents: client.streamWorkerRunEvents,
}));

vi.mock("../api/org", () => ({
  appendCompanyConversationMessage: org.appendCompanyConversationMessage,
  createCompanyConversation: org.createCompanyConversation,
  createCompanyWorkOrder: org.createCompanyWorkOrder,
  dispatchPlan: org.dispatchPlan,
  executeCompanyWorkOrder: org.executeCompanyWorkOrder,
  getCompanyProfileById: org.getCompanyProfileById,
  listCompanyConversationMessages: org.listCompanyConversationMessages,
  listCompanyConversations: org.listCompanyConversations,
  listCompanyEmployees: org.listCompanyEmployees,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

vi.mock("../api/bootstrap", () => ({
  ensureWorkspaceDefaults: bootstrap.ensureWorkspaceDefaults,
}));

function renderMissionChat(path = "/") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/"
          element={
            <GovernanceProvider>
              <MissionChat />
            </GovernanceProvider>
          }
        />
        <Route
          path="/conversations/:conversationId"
          element={
            <GovernanceProvider>
              <MissionChat />
            </GovernanceProvider>
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}

function renderMissionShell(path = "/") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <GovernanceProvider>
        <div>
          <Sidebar />
          <Routes>
            <Route path="/" element={<MissionChat />} />
            <Route path="/conversations/:conversationId" element={<MissionChat />} />
          </Routes>
        </div>
      </GovernanceProvider>
    </MemoryRouter>,
  );
}

describe("mission session behavior", () => {
  beforeEach(() => {
    cleanup();
    localStorage.clear();
    setLanguage("en");
    localStorage.setItem("coevo-opc-id", "opc-a");
    localStorage.setItem("coevo-user-id", "founder-a");
    org.listCompanyEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", display_name: "Founder", department: "FounderOffice", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);
    org.getCompanyProfileById.mockResolvedValue({ active_projects: ["Alpha"] });
    org.listCompanyConversations.mockResolvedValue([]);
    org.listCompanyConversationMessages.mockResolvedValue([]);
    org.createCompanyConversation.mockResolvedValue({ conversation_id: "conv-a-1", title: "Alpha task" });
    org.appendCompanyConversationMessage.mockResolvedValue({ ok: true });
    org.createCompanyWorkOrder.mockResolvedValue({
      work_order_id: "wo-a-1",
      status: "Planned",
      governance_verdict: {
        effective_track: "green",
        effective_tier: "read_only",
        requested_ceiling: "read_only",
        downgraded: false,
        blocked: false,
        resolved_agent_id: "agent-founder-01",
      },
    });
    bootstrap.ensureWorkspaceDefaults.mockResolvedValue({
      selectedAgentIds: ["agent-founder-01"],
      requiredSkillIds: ["skill-mission-draft"],
      seededEmployees: false,
      seededSkills: false,
    });
    client.compileContract.mockResolvedValue({
      contract: { mission: "Alpha task" },
      contract_hash: "a".repeat(64),
    });
    client.routePlan.mockResolvedValue({ plan_hash: "b".repeat(64) });
    client.modelChat.mockResolvedValue({ content: "Alpha summary" });
    client.executeWorkOrder.mockResolvedValue({ run_id: "run-a-1" });
    org.executeCompanyWorkOrder.mockResolvedValue({ run_id: "run-a-1", status: "Running" });
    org.listCompanyWorkOrders.mockResolvedValue([]);
    org.dispatchPlan.mockResolvedValue(null);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("submits conversations through the active company scope", async () => {
    renderMissionChat();

    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "Prepare the Alpha company follow-up" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(org.createCompanyConversation).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(org.createCompanyWorkOrder).toHaveBeenCalledTimes(1));

    expect(org.listCompanyEmployees).toHaveBeenCalledWith("opc-a");
    expect(org.getCompanyProfileById).toHaveBeenCalledWith("opc-a");
    expect(org.createCompanyConversation).toHaveBeenCalledWith(
      "opc-a",
      expect.objectContaining({ opc_id: "opc-a", user_id: "founder-a" }),
    );
    expect(org.appendCompanyConversationMessage).toHaveBeenCalledWith(
      "opc-a",
      "conv-a-1",
      expect.objectContaining({ role: "user" }),
    );
    expect(org.createCompanyWorkOrder).toHaveBeenCalledWith(
      "opc-a",
      expect.objectContaining({ opc_id: "opc-a", conversation_id: "conv-a-1" }),
    );
    expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-a", "wo-a-1", {});
    expect(readActiveConversationId("opc-a", "founder-a")).toBe("conv-a-1");
    expect(client.streamWorkerRunEvents).toHaveBeenCalledTimes(1);
  });

  it("does not restore another company's active conversation", async () => {
    localStorage.setItem("coevo-opc-id", "opc-b");
    localStorage.setItem("coevo-user-id", "founder-b");
    localStorage.setItem(missionStateKey("opc-b", "founder-b"), JSON.stringify({
      messages: [{ role: "user", text: "Beta plan" }],
      conversation_id: "conv-b-1",
      last_work_order_id: "wo-b-1",
    }));
    localStorage.setItem("coevo-opc-id", "opc-a");
    localStorage.setItem("coevo-user-id", "founder-a");

    renderMissionChat();

    expect(await screen.findByRole("heading", { name: "What should your AI employees help with today?" })).toBeInTheDocument();
    expect(screen.queryByText("Beta plan")).not.toBeInTheDocument();
    expect(readActiveConversationId("opc-a", "founder-a")).toBe("");
  });

  it("sidebar new chat clears only the current company mission session", async () => {
    localStorage.setItem(missionStateKey("opc-a", "founder-a"), JSON.stringify({
      messages: [{ role: "user", text: "Alpha stale task" }],
      conversation_id: "conv-a-1",
      last_work_order_id: "wo-a-1",
    }));
    localStorage.setItem(missionStateKey("opc-b", "founder-a"), JSON.stringify({
      messages: [{ role: "user", text: "Beta keep" }],
      conversation_id: "conv-b-1",
      last_work_order_id: "wo-b-1",
    }));
    org.listCompanyConversations.mockResolvedValue([
      { conversation_id: "conv-a-1", title: "Alpha stale task", updated_at_ms: Date.now() },
    ]);

    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>,
    );

    await screen.findByText("Alpha stale task");
    fireEvent.click(screen.getByRole("link", { name: "New Chat" }));

    expect(localStorage.getItem(missionStateKey("opc-a", "founder-a"))).toBeNull();
    expect(localStorage.getItem(missionStateKey("opc-b", "founder-a"))).not.toBeNull();
    expect(readActiveConversationId("opc-a", "founder-a")).toBe("");
  });

  it("sidebar new chat clears the mounted mission view on the home route", async () => {
    localStorage.setItem(missionStateKey("opc-a", "founder-a"), JSON.stringify({
      messages: [{ role: "user", text: "Alpha stale body" }],
      conversation_id: "conv-a-1",
      last_work_order_id: "wo-a-1",
    }));
    org.listCompanyConversations.mockResolvedValue([
      { conversation_id: "conv-a-1", title: "Alpha stale thread", updated_at_ms: Date.now() },
    ]);

    renderMissionShell();

    expect(await screen.findByText("Alpha stale body")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: "New Chat" }));

    await waitFor(() => {
      expect(screen.queryByText("Alpha stale body")).not.toBeInTheDocument();
    });
    expect(await screen.findByRole("heading", { name: "What should your AI employees help with today?" })).toBeInTheDocument();
    expect(localStorage.getItem(missionStateKey("opc-a", "founder-a"))).toBeNull();
    expect(readActiveConversationId("opc-a", "founder-a")).toBe("");
  });
});
