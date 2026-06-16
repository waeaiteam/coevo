import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import MissionChat from "../pages/MissionChat";
import WorkOrders from "../pages/WorkOrders";
import { GovernanceProvider } from "../hooks/useGovernance";

const client = vi.hoisted(() => ({
  listWorkOrders: vi.fn(),
  getWorkOrderTimeline: vi.fn(),
  getWorkOrderAuditExport: vi.fn(),
  cancelWorkOrder: vi.fn(),
  executeWorkOrder: vi.fn(),
  submitWorkOrderFeedback: vi.fn(),
}));

const org = vi.hoisted(() => ({
  listCompanyEmployees: vi.fn(),
  getCompanyProfileById: vi.fn(),
  listCompanyConversations: vi.fn(),
  listCompanyConversationMessages: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
}));

vi.mock("../api/client", () => ({
  listWorkOrders: client.listWorkOrders,
  getWorkOrderTimeline: client.getWorkOrderTimeline,
  getWorkOrderAuditExport: client.getWorkOrderAuditExport,
  cancelWorkOrder: client.cancelWorkOrder,
  executeWorkOrder: client.executeWorkOrder,
  submitWorkOrderFeedback: client.submitWorkOrderFeedback,
}));

vi.mock("../api/org", () => ({
  listCompanyEmployees: org.listCompanyEmployees,
  getCompanyProfileById: org.getCompanyProfileById,
  listCompanyConversations: org.listCompanyConversations,
  listCompanyConversationMessages: org.listCompanyConversationMessages,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

function renderMissionChat(path = "/") {
  render(
    <MemoryRouter initialEntries={[path]}>
      <GovernanceProvider>
        <Routes>
          <Route path="/" element={<MissionChat />} />
          <Route path="/conversations/:conversationId" element={<MissionChat />} />
        </Routes>
      </GovernanceProvider>
    </MemoryRouter>,
  );
}

describe("Result flow coherence", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("coevo-opc-id", "opc-live");
    localStorage.setItem("coevo-user-id", "founder-live");
    localStorage.setItem(
      "coevo-missionchat-state:opc-live:founder-live",
      JSON.stringify({
        messages: [
          { role: "user", text: "Hello, what can you do?" },
          { role: "system", text: "Task completed." },
        ],
        last_work_order_id: "wo-1",
        conversation_id: "conv-1",
      }),
    );
    localStorage.setItem("coevo-active-conversation-id:opc-live:founder-live", "conv-1");

    org.listCompanyEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", display_name: "Founder Assistant", department: "FounderOffice" },
    ]);
    org.getCompanyProfileById.mockResolvedValue({ active_projects: [] });
    org.listCompanyConversations.mockResolvedValue([
      { conversation_id: "conv-1", title: "Hello, what can you do?" },
    ]);
    org.listCompanyConversationMessages.mockResolvedValue([
      { role: "user", content: "Hello, what can you do?" },
      { role: "assistant", content: "Task completed.", linked_work_order_id: "wo-1" },
    ]);
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-1",
        mission_intent: "Hello, what can you do?",
        track: "green",
        status: "Completed",
        selected_agents: ["agent-founder-01"],
        selected_executors: [],
        required_skills: [],
        contract_hash: "a".repeat(64),
      },
    ]);

    client.listWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-1",
        mission_intent: "Hello, what can you do?",
        track: "green",
        status: "Completed",
        selected_agents: ["agent-founder-01"],
        selected_executors: [],
        required_skills: [],
        contract_hash: "a".repeat(64),
      },
    ]);
    client.getWorkOrderTimeline.mockResolvedValue([]);
    client.getWorkOrderAuditExport.mockResolvedValue({ schema_version: "coevo.audit_export.v1" });
    client.cancelWorkOrder.mockResolvedValue({ ok: true });
    client.executeWorkOrder.mockResolvedValue({ ok: true });
    client.submitWorkOrderFeedback.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a completed-task CTA on the mission page instead of start-task language", async () => {
    renderMissionChat();

    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalled());
    expect(await screen.findByText("Result is ready")).toBeInTheDocument();
    expect(screen.getByText("This task has already completed. Open it to review the final answer and execution record.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Task" })).toHaveAttribute("href", "/tasks/wo-1");
    expect(screen.getByRole("link", { name: "View Result" })).toHaveAttribute("href", "/tasks/wo-1");
    expect(screen.queryByRole("link", { name: "Start Task" })).not.toBeInTheDocument();
  });

  it("uses navigation wording for ready tasks instead of implying direct execution", async () => {
    org.listCompanyWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-1",
        mission_intent: "Hello, what can you do?",
        track: "green",
        status: "Planned",
        selected_agents: ["agent-founder-01"],
        selected_executors: [],
        required_skills: [],
        contract_hash: "a".repeat(64),
      },
    ]);

    client.listWorkOrders.mockResolvedValue([
      {
        work_order_id: "wo-1",
        mission_intent: "Hello, what can you do?",
        track: "green",
        status: "Planned",
        selected_agents: ["agent-founder-01"],
        selected_executors: [],
        required_skills: [],
        contract_hash: "a".repeat(64),
      },
    ]);

    renderMissionChat();

    await waitFor(() => expect(org.listCompanyWorkOrders).toHaveBeenCalled());
    expect(await screen.findByText("Ready to start")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Task" })).toHaveAttribute("href", "/tasks/wo-1");
    expect(screen.getByRole("link", { name: "Open Task Center" })).toHaveAttribute("href", "/work-orders");
    expect(screen.queryByRole("link", { name: "Start Task" })).not.toBeInTheDocument();
  });

  it("routes View Result to the task detail page", async () => {
    render(
      <MemoryRouter>
        <WorkOrders />
      </MemoryRouter>,
    );

    const resultLink = await screen.findByRole("link", { name: "View Result" });
    expect(resultLink).toHaveAttribute("href", "/tasks/wo-1");
    expect(screen.getByRole("button", { name: "View Timeline" })).toBeInTheDocument();
  });
});
