import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { GovernanceProvider } from "../hooks/useGovernance";
import Layout from "../components/Layout";
import MissionChat from "../pages/MissionChat";
import { setLanguage } from "../settings/i18n";

const INTERNAL_TERMS =
  /\b(WorkOrder|Governance|RiskGate|Track|AgentSubHarness|ReAct|GovernGate|sandbox|Policy|token|harness|executor|traceparent)\b/i;

const api = vi.hoisted(() => ({
  getHealth: vi.fn(),
  compileContract: vi.fn(),
  routePlan: vi.fn(),
  streamWorkerRunEvents: vi.fn(() => () => undefined),
  listConversations: vi.fn(),
  createConversation: vi.fn(),
  getCompanyProfile: vi.fn(),
  listConversationMessages: vi.fn(),
  appendConversationMessage: vi.fn(),
  modelChat: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

const org = vi.hoisted(() => ({
  createCompanyConversation: vi.fn(),
  appendCompanyConversationMessage: vi.fn(),
  createCompanyWorkOrder: vi.fn(),
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
  getHealth: api.getHealth,
  compileContract: api.compileContract,
  routePlan: api.routePlan,
  streamWorkerRunEvents: api.streamWorkerRunEvents,
  listConversations: api.listConversations,
  createConversation: api.createConversation,
  getCompanyProfile: api.getCompanyProfile,
  listConversationMessages: api.listConversationMessages,
  appendConversationMessage: api.appendConversationMessage,
  modelChat: api.modelChat,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

vi.mock("../api/org", () => ({
  createCompanyConversation: org.createCompanyConversation,
  appendCompanyConversationMessage: org.appendCompanyConversationMessage,
  createCompanyWorkOrder: org.createCompanyWorkOrder,
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

function renderWorkbench() {
  render(
    <MemoryRouter>
      <GovernanceProvider>
        <MissionChat />
      </GovernanceProvider>
    </MemoryRouter>,
  );
}

describe("customer-facing desktop workbench", () => {
  beforeEach(() => {
    localStorage.clear();
    setLanguage("zh");
    localStorage.setItem("coevo-user-id", "default-founder");
    localStorage.setItem("coevo-opc-id", "default-opc");

    api.getHealth.mockResolvedValue({ status: "ok", version: "1.0.0" });
    api.compileContract.mockResolvedValue({
      contract: { mission: "整理客户线索" },
      contract_hash: "a".repeat(64),
      ambiguity_score: 0.1,
      compile_warnings: [],
    });
    api.routePlan.mockResolvedValue({
      plan: { steps: ["整理", "汇总"] },
      plan_hash: "b".repeat(64),
    });
    api.listConversations.mockResolvedValue([]);
    api.createConversation.mockResolvedValue({ conversation_id: "conv-customer-1", title: "整理本周客户线索" });
    api.getCompanyProfile.mockResolvedValue({ active_projects: ["客户项目"] });
    api.listConversationMessages.mockResolvedValue([]);
    api.appendConversationMessage.mockResolvedValue({ ok: true });
    api.modelChat.mockResolvedValue({
      content: "我会先整理客户线索，再生成跟进清单。",
      model: "deepseek-v4-flash",
      provider_kind: "DeepSeek",
    });
    api.listEmployees.mockResolvedValue([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }]);
    api.listSkills.mockResolvedValue([{ skill_id: "skill-mission-draft", status: "Active", owner_agent_id: "agent-founder-01" }]);
    api.seedEmployees.mockResolvedValue({ ok: true });
    api.seedSkills.mockResolvedValue({ ok: true });

    bootstrap.ensureWorkspaceDefaults.mockResolvedValue({
      selectedAgentIds: ["agent-founder-01"],
      requiredSkillIds: ["skill-mission-draft"],
    });
    org.createCompanyConversation.mockResolvedValue({ conversation_id: "conv-customer-1", title: "整理本周客户线索" });
    org.appendCompanyConversationMessage.mockResolvedValue({ ok: true });
    org.createCompanyWorkOrder.mockResolvedValue({
      work_order_id: "wo-customer-1",
      status: "Planned",
      governance_verdict: {
        effective_track: "green",
        effective_tier: "read_only",
        requested_ceiling: "read_only",
        downgraded: false,
        downgrade_reason: null,
        blocked: false,
        block_reason: null,
        resolved_agent_id: "agent-founder-01",
      },
    });
    org.executeCompanyWorkOrder.mockResolvedValue({ ok: true, run_id: "run-customer-1" });
    org.getCompanyProfileById.mockResolvedValue({ active_projects: ["客户项目"] });
    org.listCompanyConversationMessages.mockResolvedValue([]);
    org.listCompanyConversations.mockResolvedValue([]);
    org.listCompanyEmployees.mockResolvedValue([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }]);
    org.listCompanyWorkOrders.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a polished public workbench without internal governance terminology", async () => {
    renderWorkbench();

    expect(screen.getByRole("heading", { name: "今天让 AI 员工帮你做什么？" })).toBeInTheDocument();
    expect(screen.getByPlaceholderText("例如：整理本周客户线索，并生成跟进清单")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "发送" })).toBeInTheDocument();
    expect(screen.getByLabelText("自主度")).toBeInTheDocument();
    expect(screen.getByLabelText("指派员工")).toBeInTheDocument();
    expect(screen.getByLabelText("模型")).toBeInTheDocument();
    expect(screen.getByText("今日进展")).toBeInTheDocument();
    expect(screen.getByText("我的公司")).toBeInTheDocument();
    expect(screen.getByText("项目")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("agent-founder-01")).toBeInTheDocument());
    expect(document.body.textContent).not.toMatch(INTERNAL_TERMS);
  });

  it("turns a real user task into a customer-readable task update", async () => {
    renderWorkbench();

    fireEvent.change(screen.getByPlaceholderText("例如：整理本周客户线索，并生成跟进清单"), {
      target: { value: "整理本周客户线索，并生成明天的跟进清单" },
    });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(org.createCompanyWorkOrder).toHaveBeenCalledTimes(1));
    expect(org.createCompanyWorkOrder.mock.calls[0][1]).toMatchObject({
      governance_proposal: {
        autonomy_ceiling: "read_only",
        model_preference: "standard",
        assigned_agent_id: null,
      },
    });
    await waitFor(() =>
      expect(screen.getByText("任务已创建，正在准备给你确认的执行方案。")).toBeInTheDocument(),
    );
    expect(screen.getByText(/我会先整理客户线索/)).toBeInTheDocument();
    expect(screen.getByText("安全状态")).toBeInTheDocument();
    expect(screen.getByText("打开任务")).toBeInTheDocument();
    expect(screen.getByText("执行时间线")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(INTERNAL_TERMS);
  });

  it("uses public navigation labels and keeps advanced controls behind settings", async () => {
    render(
      <MemoryRouter>
        <GovernanceProvider>
          <Layout />
        </GovernanceProvider>
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: "新对话" })).toHaveAttribute("href", "/");
    expect(screen.getByRole("link", { name: "我的公司" })).toHaveAttribute("href", "/company");
    expect(screen.getByRole("link", { name: "项目" })).toHaveAttribute("href", "/projects");
    expect(screen.getByRole("link", { name: "任务" })).toHaveAttribute("href", "/work-orders");
    expect(screen.getByRole("link", { name: "时间线" })).toHaveAttribute("href", "/timeline");
    expect(screen.getByRole("link", { name: "设置" })).toHaveAttribute("href", "/settings/general");
    await waitFor(() => expect(api.getHealth).toHaveBeenCalled());
  });
});
