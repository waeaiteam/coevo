import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { GovernanceProvider } from "../hooks/useGovernance";
import MissionChat from "../pages/MissionChat";
import Layout from "../components/Layout";
import { setLanguage } from "../settings/i18n";

const INTERNAL_TERMS =
  /\b(WorkOrder|Governance|RiskGate|Track|AgentSubHarness|ReAct|GovernGate|sandbox|Policy|token|harness|executor|traceparent)\b/i;

const api = vi.hoisted(() => ({
  getHealth: vi.fn(),
  compileContract: vi.fn(),
  routePlan: vi.fn(),
  createWorkOrder: vi.fn(),
  listConversations: vi.fn(),
  createConversation: vi.fn(),
  listConversationMessages: vi.fn(),
  appendConversationMessage: vi.fn(),
  modelChat: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getHealth: api.getHealth,
  compileContract: api.compileContract,
  routePlan: api.routePlan,
  createWorkOrder: api.createWorkOrder,
  listConversations: api.listConversations,
  createConversation: api.createConversation,
  listConversationMessages: api.listConversationMessages,
  appendConversationMessage: api.appendConversationMessage,
  modelChat: api.modelChat,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

function renderWorkbench() {
  render(
    <MemoryRouter>
      <GovernanceProvider>
        <MissionChat />
      </GovernanceProvider>
    </MemoryRouter>
  );
}

describe("customer-facing desktop workbench", () => {
  beforeEach(() => {
    localStorage.clear();
    setLanguage("zh");
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
    api.createWorkOrder.mockResolvedValue({
      ok: true,
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
    api.listConversations.mockResolvedValue([]);
    api.createConversation.mockResolvedValue({
      conversation_id: "conv-customer-1",
      title: "整理本周客户线索",
    });
    api.listConversationMessages.mockResolvedValue([]);
    api.appendConversationMessage.mockResolvedValue({ ok: true });
    api.modelChat.mockResolvedValue({
      content: "我会先整理客户线索，再生成跟进清单。",
      model: "deepseek-v4-flash",
      provider_kind: "DeepSeek",
    });
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);
    api.listSkills.mockResolvedValue([
      { skill_id: "skill-mission-draft", status: "Active", owner_agent_id: "agent-founder-01" },
    ]);
    api.seedEmployees.mockResolvedValue({ ok: true });
    api.seedSkills.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows a polished public workbench without internal governance terminology", () => {
    renderWorkbench();

    expect(screen.getByRole("heading", { name: "今天让 AI 员工帮你做什么？" })).toBeInTheDocument();
    expect(screen.getByPlaceholderText("例如：整理本周客户线索，并生成跟进清单")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "发送" })).toBeInTheDocument();
    expect(screen.getByLabelText("自主度")).toBeInTheDocument();
    expect(screen.getByLabelText("指派员工")).toBeInTheDocument();
    expect(screen.getByLabelText("模型")).toBeInTheDocument();
    expect(screen.getByText("今日进展")).toBeInTheDocument();
    expect(screen.getByText("交付物")).toBeInTheDocument();
    expect(screen.getByText("数据保存在本机")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(INTERNAL_TERMS);
  });

  it("turns a real user task into a customer-readable task update", async () => {
    renderWorkbench();

    fireEvent.change(screen.getByPlaceholderText("例如：整理本周客户线索，并生成跟进清单"), {
      target: { value: "整理本周客户线索，并生成明天的跟进清单" },
    });
    fireEvent.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(api.createWorkOrder).toHaveBeenCalledTimes(1));
    expect(api.createWorkOrder.mock.calls[0][0]).toMatchObject({
      governance_proposal: {
        autonomy_ceiling: "read_only",
        model_preference: "standard",
        assigned_agent_id: null,
      },
    });
    expect(screen.getByText("任务已创建，正在准备给你确认的执行方案。")).toBeInTheDocument();
    expect(screen.getByText(/我会先整理客户线索/)).toBeInTheDocument();
    expect(screen.getByText("治理裁定")).toBeInTheDocument();
    expect(screen.getByText("执行时间线")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(INTERNAL_TERMS);
  });

  it("uses public navigation labels and keeps advanced controls behind settings", () => {
    render(
      <MemoryRouter>
        <GovernanceProvider>
          <Layout />
        </GovernanceProvider>
      </MemoryRouter>
    );

    expect(screen.getByRole("link", { name: /工作台/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /AI 员工/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /任务/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /客户/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /文件/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /成果/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /高级设置/ })).toBeInTheDocument();
  });
});
