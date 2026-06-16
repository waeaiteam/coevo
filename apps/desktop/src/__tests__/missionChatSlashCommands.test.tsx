import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { GovernanceProvider } from "../hooks/useGovernance";
import MissionChat from "../pages/MissionChat";
import { setLanguage } from "../settings/i18n";

const client = vi.hoisted(() => ({
  compileContract: vi.fn(),
  routePlan: vi.fn(),
  modelChat: vi.fn(),
  executeWorkOrder: vi.fn(),
  streamWorkerRunEvents: vi.fn(),
  listWorkerRunEvents: vi.fn(),
  getWorkerRunReflection: vi.fn(),
  listEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedEmployees: vi.fn(),
  seedSkills: vi.fn(),
}));

const org = vi.hoisted(() => ({
  createCompanyConversation: vi.fn(),
  appendCompanyConversationMessage: vi.fn(),
  createCompanyWorkOrder: vi.fn(),
  executeCompanyWorkOrder: vi.fn(),
  cancelCompanyWorkOrder: vi.fn(),
  decideCompanyWorkOrderApproval: vi.fn(),
  getCompanyProfileById: vi.fn(),
  listCompanyConversationMessages: vi.fn(),
  listCompanyConversations: vi.fn(),
  listCompanyEmployees: vi.fn(),
  listCompanyWorkOrders: vi.fn(),
}));

const approvalFlow = vi.hoisted(() => ({
  requestApproval: vi.fn(),
  decideAndResume: vi.fn(),
}));

vi.mock("../api/client", () => ({
  compileContract: client.compileContract,
  routePlan: client.routePlan,
  modelChat: client.modelChat,
  executeWorkOrder: client.executeWorkOrder,
  streamWorkerRunEvents: client.streamWorkerRunEvents,
  listWorkerRunEvents: client.listWorkerRunEvents,
  getWorkerRunReflection: client.getWorkerRunReflection,
  listEmployees: client.listEmployees,
  listSkills: client.listSkills,
  seedEmployees: client.seedEmployees,
  seedSkills: client.seedSkills,
}));

vi.mock("../api/org", () => ({
  createCompanyConversation: org.createCompanyConversation,
  appendCompanyConversationMessage: org.appendCompanyConversationMessage,
  createCompanyWorkOrder: org.createCompanyWorkOrder,
  executeCompanyWorkOrder: org.executeCompanyWorkOrder,
  cancelCompanyWorkOrder: org.cancelCompanyWorkOrder,
  decideCompanyWorkOrderApproval: org.decideCompanyWorkOrderApproval,
  getCompanyProfileById: org.getCompanyProfileById,
  listCompanyConversationMessages: org.listCompanyConversationMessages,
  listCompanyConversations: org.listCompanyConversations,
  listCompanyEmployees: org.listCompanyEmployees,
  listCompanyWorkOrders: org.listCompanyWorkOrders,
}));

vi.mock("../utils/approvalFlow", () => ({
  requestApproval: approvalFlow.requestApproval,
  decideAndResume: approvalFlow.decideAndResume,
}));

vi.mock("../api/bootstrap", () => ({
  ensureWorkspaceDefaults: vi.fn(async () => ({
    selectedAgentIds: ["agent-founder-01"],
    requiredSkillIds: ["skill-mission-draft"],
  })),
}));

vi.mock("../api/companies", () => ({
  getActiveOpcId: () => localStorage.getItem("coevo-opc-id") || "default-opc",
}));

vi.mock("../api/tauri", () => ({
  getTauriInvoke: () => null,
}));

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="current-path">{location.pathname}</div>;
}

function renderMissionChat(path = "/") {
  render(
    <MemoryRouter
      initialEntries={[path]}
      future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
    >
      <GovernanceProvider>
        <LocationProbe />
        <Routes>
          <Route path="/" element={<MissionChat />} />
          <Route path="/conversations/:conversationId" element={<MissionChat />} />
          <Route path="/work-orders" element={<div data-testid="work-orders-route" />} />
          <Route path="*" element={<div data-testid="fallback-route" />} />
        </Routes>
      </GovernanceProvider>
    </MemoryRouter>,
  );
}

function missionStateKey() {
  const opcId = localStorage.getItem("coevo-opc-id") || "default-opc";
  const userId = localStorage.getItem("coevo-user-id") || "default-user";
  return `coevo-missionchat-state:${opcId}:${userId}`;
}

describe("MissionChat slash commands", () => {
  beforeEach(() => {
    localStorage.clear();
    setLanguage("en");
    localStorage.setItem("coevo-user-id", "default-founder");
    localStorage.setItem("coevo-opc-id", "default-opc");

    client.compileContract.mockResolvedValue({
      contract: { mission: "Analyze the README" },
      contract_hash: "a".repeat(64),
      ambiguity_score: 0.12,
      compile_warnings: [],
    });
    client.routePlan.mockResolvedValue({
      plan: { steps: ["read", "summarize"] },
      plan_hash: "b".repeat(64),
    });
    client.modelChat.mockResolvedValue({
      content: "This is a read-only analysis mission.",
      model: "gpt-4o",
      provider_kind: "OpenAICompatible",
    });
    client.executeWorkOrder.mockResolvedValue({ ok: true, run_id: "run-green-1" });
    client.streamWorkerRunEvents.mockImplementation(() => () => undefined);
    client.listWorkerRunEvents.mockResolvedValue([]);
    client.getWorkerRunReflection.mockResolvedValue(null);
    client.listEmployees.mockResolvedValue([]);
    client.listSkills.mockResolvedValue([]);
    client.seedEmployees.mockResolvedValue({ ok: true });
    client.seedSkills.mockResolvedValue({ ok: true });

    org.createCompanyConversation.mockResolvedValue({
      conversation_id: "conv-mission-1",
      title: "Analyze the README",
    });
    org.appendCompanyConversationMessage.mockResolvedValue({ ok: true });
    org.createCompanyWorkOrder.mockResolvedValue({
      ok: true,
      work_order_id: "wo-mission-1",
      approval_id: "approval-1",
      status: "WaitingApproval",
      governance_verdict: {
        effective_track: "yellow",
        effective_tier: "read_only",
        requested_ceiling: "read_only",
        downgraded: true,
        downgrade_reason: "Needs review",
        blocked: false,
        block_reason: null,
        resolved_agent_id: "agent-founder-01",
      },
    });
    org.executeCompanyWorkOrder.mockResolvedValue({ ok: true, run_id: "run-yellow-1", approval_id: "approval-1" });
    org.cancelCompanyWorkOrder.mockResolvedValue({ ok: true, status: "Cancelled" });
    org.decideCompanyWorkOrderApproval.mockResolvedValue({ ok: true, run_id: "run-approve-1", status: "Running" });
    org.getCompanyProfileById.mockResolvedValue({ active_projects: ["Launch Project"] });
    org.listCompanyConversationMessages.mockResolvedValue([]);
    org.listCompanyConversations.mockResolvedValue([]);
    org.listCompanyEmployees.mockResolvedValue([]);
    org.listCompanyWorkOrders.mockResolvedValue([
      { work_order_id: "wo-mission-1", status: "WaitingApproval", approval_id: "approval-1" },
    ]);

    approvalFlow.requestApproval.mockResolvedValue({ approvalId: "approval-1", status: "WaitingApproval", payload: {} });
    approvalFlow.decideAndResume.mockResolvedValue({
      status: "Running",
      runId: "run-approve-1",
      payload: { run_id: "run-approve-1", status: "Running" },
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows the slash menu and executes help without creating a task", async () => {
    renderMissionChat();

    const textbox = screen.getByRole("textbox");
    fireEvent.change(textbox, { target: { value: "/he" } });

    expect(await screen.findByRole("listbox", { name: "Slash commands" })).toBeInTheDocument();
    fireEvent.keyDown(textbox, { key: "Enter" });

    expect(await screen.findByText(/Available slash commands/i)).toBeInTheDocument();
    expect(org.createCompanyWorkOrder).not.toHaveBeenCalled();
  });

  it("supports arrow navigation and escape dismissal inside the slash menu", async () => {
    renderMissionChat();

    const textbox = screen.getByRole("textbox");
    fireEvent.change(textbox, { target: { value: "/" } });

    expect(await screen.findByRole("listbox", { name: "Slash commands" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/status/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.keyDown(textbox, { key: "ArrowDown" });
    expect(screen.getByRole("option", { name: /\/approve/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.keyDown(textbox, { key: "ArrowUp" });
    expect(screen.getByRole("option", { name: /\/status/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.keyDown(textbox, { key: "Escape" });
    await waitFor(() =>
      expect(
        screen.queryByRole("listbox", { name: "Slash commands" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("navigates when /go is executed", async () => {
    renderMissionChat();

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/go work-orders" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/go/i }));

    expect(await screen.findByTestId("current-path")).toHaveTextContent("/work-orders");
  });

  it("runs slash status, run, approve, reject, cancel, and clear commands against current mission state", async () => {
    renderMissionChat();

    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "Analyze the README and summarize the project direction" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(org.createCompanyWorkOrder).toHaveBeenCalledTimes(1));

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/status" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/status/i }));
    expect(await screen.findByText(/Task status/i)).toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/run" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/run/i }));
    await waitFor(() => expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("default-opc", "wo-mission-1", {}));

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/approve looks good" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/approve/i }));
    await waitFor(() =>
      expect(approvalFlow.decideAndResume).toHaveBeenCalledWith("default-opc", "wo-mission-1", {
        approvalId: "approval-1",
        decision: "approve",
        comment: "looks good",
      }),
    );

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/reject needs changes" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/reject/i }));
    await waitFor(() =>
      expect(approvalFlow.decideAndResume).toHaveBeenCalledWith("default-opc", "wo-mission-1", {
        approvalId: "approval-1",
        decision: "reject",
        comment: "needs changes",
      }),
    );

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/cancel" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/cancel/i }));
    await waitFor(() => expect(org.cancelCompanyWorkOrder).toHaveBeenCalledWith("default-opc", "wo-mission-1"));

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/clear" } });
    fireEvent.click(await screen.findByRole("option", { name: /\/clear/i }));
    expect(screen.getByRole("textbox")).toHaveValue("");
  });

  it("rejects unknown slash commands without sending a normal message", async () => {
    renderMissionChat();

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "/nope" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });

    expect(await screen.findByText(/Unknown slash command/i)).toBeInTheDocument();
    expect(org.createCompanyWorkOrder).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox")).toHaveValue("");
  });
});
