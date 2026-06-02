import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import App from "../App";
import Dashboard from "../pages/Dashboard";
import ExternalExecutors from "../pages/ExternalExecutors";
import Settings from "../pages/Settings";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";
import { setLanguage } from "../settings/i18n";

const MOJIBAKE_PATTERN = /[\uFFFD\u6D93\u93BC\u7481\u59AF\u6E1A]/;

const api = vi.hoisted(() => ({
  getHealth: vi.fn(),
  listExecutors: vi.fn(),
  registerExecutor: vi.fn(),
  disableExecutor: vi.fn(),
  executorHealth: vi.fn(),
  executorDryRun: vi.fn(),
  listWorkOrders: vi.fn(),
  getGlobalTimeline: vi.fn(),
  listMemory: vi.fn(),
  listConversations: vi.fn(),
  getCompanyProfile: vi.fn(),
  getUserProfile: vi.fn(),
  getModelConfig: vi.fn(),
  discoverModels: vi.fn(),
  testModelConnection: vi.fn(),
  updateModelConfig: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getApiBase: () => "http://127.0.0.1:8717",
  getHealth: api.getHealth,
  listExecutors: api.listExecutors,
  registerExecutor: api.registerExecutor,
  disableExecutor: api.disableExecutor,
  executorHealth: api.executorHealth,
  executorDryRun: api.executorDryRun,
  listWorkOrders: api.listWorkOrders,
  getGlobalTimeline: api.getGlobalTimeline,
  listMemory: api.listMemory,
  listConversations: api.listConversations,
  getCompanyProfile: api.getCompanyProfile,
  getUserProfile: api.getUserProfile,
  getModelConfig: api.getModelConfig,
  discoverModels: api.discoverModels,
  testModelConnection: api.testModelConnection,
  updateModelConfig: api.updateModelConfig,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

vi.mock("../components/BootPage", () => ({
  default: ({ onReady }: { onReady: () => void }) => (
    <button onClick={onReady}>Boot Ready</button>
  ),
}));

describe("ordinary user product surface", () => {
  beforeEach(() => {
    localStorage.clear();
    setLanguage("en");
    api.getHealth.mockResolvedValue({ status: "ok", version: "1.0.0" });
    api.listExecutors.mockResolvedValue([]);
    api.listWorkOrders.mockResolvedValue([]);
    api.getGlobalTimeline.mockResolvedValue([]);
    api.listMemory.mockResolvedValue([]);
    api.listConversations.mockResolvedValue([]);
    api.getCompanyProfile.mockResolvedValue({ active_projects: [] });
    api.getUserProfile.mockResolvedValue({ active_projects: [] });
    api.getModelConfig.mockResolvedValue({ kind: "DeepSeek", has_api_key: true });
    api.registerExecutor.mockResolvedValue({ ok: true });
    api.testModelConnection.mockResolvedValue({ model: "gpt-4o", latency_ms: 9, provider_kind: "OpenAI" });
    api.updateModelConfig.mockResolvedValue({ ok: true });
    api.discoverModels.mockResolvedValue({ models: [] });
    api.listEmployees.mockResolvedValue([]);
    api.listSkills.mockResolvedValue([{ skill_id: "skill-mission-draft", status: "Active" }]);
    api.seedEmployees.mockResolvedValue({ ok: true });
    api.seedSkills.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("OPC console exposes every retired top-level feature through Advanced Console", () => {
    localStorage.setItem("coevo-opc-name", "WAE AI Team");
    localStorage.setItem("coevo-user-name", "Wae");
    localStorage.setItem("coevo-opc-id", "opc-123");

    render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>
    );

    const advanced = screen.getByRole("region", { name: /Advanced Console/i });
    [
      "Founder Profile",
      "Company Memory",
      "AI Employees",
      "Skills",
      "External Executors",
      "Contracts",
      "Plans",
      "Cognitive Customs",
      "Risk Gate",
      "Resolution",
      "Data Management",
      "Developer Mode",
      "Policy Engine",
      "Privacy",
      "Model Provider",
      "Language & Appearance",
    ].forEach((label) => {
      expect(within(advanced).getByRole("link", { name: new RegExp(`^${label}\\b`, "i") })).toBeInTheDocument();
    });
  });

  it("Dashboard does not expose demo action controls", () => {
    localStorage.setItem("coevo-opc-name", "WAE AI Team");
    localStorage.setItem("coevo-user-name", "Wae");
    localStorage.setItem("coevo-opc-id", "opc-123");

    render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>
    );

    expect(screen.queryByText("Demo Actions")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "WAE AI Team" })).toBeInTheDocument();
    expect(screen.getByText("Owner")).toBeInTheDocument();
    expect(screen.getByText("Wae")).toBeInTheDocument();
  });

  it("Dashboard presents the live OPC company space for ordinary founders", async () => {
    localStorage.setItem("coevo-opc-name", "WAE AI Team");
    localStorage.setItem("coevo-user-name", "Wae");
    localStorage.setItem("coevo-opc-id", "opc-123");
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", display_name: "Founder Chief of Staff", department: "FounderOffice", lifecycle_status: "Active", risk_ceiling: 0.3 },
      { agent_id: "agent-risk-01", display_name: "Risk Reviewer", department: "Governance", lifecycle_status: "Active", risk_ceiling: 0.6 },
    ]);
    api.listMemory.mockResolvedValue([
      { memory_id: "mem-1", title: "Company operating rules", scope: "Company", status: "Active" },
      { memory_id: "mem-2", title: "Launch positioning", scope: "Company", status: "Active" },
    ]);
    api.listWorkOrders.mockResolvedValue([
      { work_order_id: "wo-green", mission_intent: "Analyze onboarding feedback", track: "green", status: "Completed" },
      { work_order_id: "wo-yellow", mission_intent: "Draft customer notification", track: "yellow", status: "WaitingApproval" },
      { work_order_id: "wo-red", mission_intent: "Delete production data", track: "red", status: "Planned" },
    ]);
    api.listConversations.mockResolvedValue([
      { conversation_id: "conv-1", title: "Onboarding feedback", updated_at_ms: 1700000000000 },
    ]);

    render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>
    );

    expect(screen.getByText("Company Operating Room")).toBeInTheDocument();
    expect(await screen.findByText("2 active AI employees")).toBeInTheDocument();
    expect(screen.getByText("2 company memories")).toBeInTheDocument();
    expect(screen.getByText("3 tasks")).toBeInTheDocument();
    expect(screen.getByText("1 conversation")).toBeInTheDocument();
    expect(screen.getByText("Founder Chief of Staff")).toBeInTheDocument();
    expect(screen.getByText("Risk Reviewer")).toBeInTheDocument();
    expect(screen.getByText("Company operating rules")).toBeInTheDocument();
    expect(screen.getByText("Draft customer notification")).toBeInTheDocument();
    expect(screen.getByText("Waiting confirmation")).toBeInTheDocument();
    expect(screen.getByText("Safety pause active")).toBeInTheDocument();
    expect(screen.getByText("Needs confirmation")).toBeInTheDocument();
    expect(screen.getByText("Paused by safety rules")).toBeInTheDocument();
    expect(screen.getByText("Safety & confirmation")).toBeInTheDocument();
    expect(screen.getByText("Safety rules")).toBeInTheDocument();
    expect(screen.getByText("Onboarding feedback")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Manage employees/i })).toHaveAttribute("href", "/employees");
    expect(screen.getByRole("link", { name: /Open task center/i })).toHaveAttribute("href", "/work-orders");
  });

  it("does not expose a Demos route in the ordinary desktop app", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");

    render(
      <MemoryRouter initialEntries={["/demos"]}>
        <App />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Boot Ready" }));

    await waitFor(() => expect(screen.queryByText("Welcome to coevo")).not.toBeInTheDocument());
    expect(screen.queryByText("Demo Scenarios")).not.toBeInTheDocument();
  });

  it("primary sidebar exposes only the core OPC workflow", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");

    render(
      <MemoryRouter initialEntries={["/"]}>
        <App />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Boot Ready" }));

    expect(await screen.findByRole("link", { name: /New Chat/i })).toHaveAttribute("href", "/");
    expect(screen.getByText("Recent Chats")).toBeInTheDocument();
    const nav = await screen.findByRole("navigation", { name: /Primary/i });
    expect(within(nav).getByRole("link", { name: /My Company/i })).toHaveAttribute("href", "/company");
    expect(within(nav).getByRole("link", { name: /Projects/i })).toHaveAttribute("href", "/projects");
    expect(within(nav).getByRole("link", { name: /^Tasks$/i })).toBeInTheDocument();
    expect(within(nav).getByRole("link", { name: /Timeline/i })).toHaveAttribute("href", "/timeline");
    expect(within(nav).getByRole("link", { name: /^Settings$/i })).toHaveAttribute("href", "/settings/general");

    expect(within(nav).queryByRole("link", { name: /Workbench/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /AI Employees/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /Clients/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /^Files$/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /^Outcomes$/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /^Skills$/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /Executors/i })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: /Risk Gate/i })).not.toBeInTheDocument();
  });

  it("routes the Timeline entry to the company timeline page", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");

    render(
      <MemoryRouter initialEntries={["/timeline"]}>
        <App />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Boot Ready" }));

    expect(await screen.findByRole("heading", { name: /Timeline/i })).toBeInTheDocument();
    expect(screen.getByText(/Company activity and task history/i)).toBeInTheDocument();
  });

  it("Settings Advanced exposes all advanced setting sections", () => {
    render(
      <MemoryRouter initialEntries={["/settings/general"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    const advanced = screen.getByRole("group", { name: /Advanced/i });
    [
      "Runtime controls",
      "Approval & safety",
      "Risk rules",
      "Memory rules",
      "Policy rules",
      "Privacy & Data",
      "Developer Mode",
    ].forEach((label) => {
      expect(within(advanced).getByRole("link", { name: new RegExp(label, "i") })).toBeInTheDocument();
    });
  });

  it("Developer Mode does not expose demo reset controls", () => {
    render(
      <MemoryRouter initialEntries={["/settings/developer"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.queryByText("Reset Demo Data")).not.toBeInTheDocument();
    expect(screen.getByText("Reset Local UI State")).toBeInTheDocument();
  });

  it("advanced settings panels expose real controls instead of blank cards", () => {
    const cases = [
      ["/settings/agent_runtime", "Maximum AI employees per task"],
      ["/settings/governance", "Auto-start low-risk work"],
      ["/settings/risk_gate", "Green threshold"],
      ["/settings/cognitive_customs", "Default memory TTL"],
      ["/settings/privacy", "Log retention days"],
    ];

    for (const [route, label] of cases) {
      cleanup();
      render(
        <MemoryRouter initialEntries={[route]}>
          <Routes>
            <Route path="/settings/*" element={<Settings />} />
          </Routes>
        </MemoryRouter>
      );

      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it("Developer Mode reset clears local UI runtime state", () => {
    localStorage.setItem("coevo-settings", JSON.stringify({ appearance: { theme: "dark" } }));
    localStorage.setItem("coevo-theme", "dark");
    localStorage.setItem("coevo-api-base", "http://127.0.0.1:8718");

    render(
      <MemoryRouter initialEntries={["/settings/developer"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /^Reset$/ }));

    expect(localStorage.getItem("coevo-settings")).toBeNull();
    expect(localStorage.getItem("coevo-theme")).toBeNull();
    expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8718");
    expect(screen.getByText("Local UI state reset")).toBeInTheDocument();
  });

  it("Developer Mode saves API Base changes through the visible save bar", () => {
    render(
      <MemoryRouter initialEntries={["/settings/developer"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.change(screen.getByDisplayValue("http://127.0.0.1:8717"), {
      target: { value: "http://127.0.0.1:8727" },
    });

    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save Changes" }));

    expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8727");
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  it("External Executor registration does not create a mock executor by default", async () => {
    render(<ExternalExecutors />);

    fireEvent.click(screen.getByRole("button", { name: "+ Register" }));
    fireEvent.change(screen.getByPlaceholderText("Display Name"), {
      target: { value: "Read-only Local Files" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Register" }));

    await waitFor(() => expect(api.registerExecutor).toHaveBeenCalledTimes(1));
    expect(api.registerExecutor).toHaveBeenCalledWith(expect.objectContaining({
      capabilities: ["read"],
      runtime_endpoint: "",
    }));
    expect(api.registerExecutor.mock.calls[0][0].capabilities).not.toContain("mock");
  });

  it("Chinese language switch immediately rerenders core navigation and Settings title without mojibake", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");
    render(
      <MemoryRouter initialEntries={["/settings/appearance"]}>
        <App />
      </MemoryRouter>
    );
    fireEvent.click(screen.getByRole("button", { name: "Boot Ready" }));

    fireEvent.change(await screen.findByLabelText(/Language/i), { target: { value: "zh" } });

    expect(await screen.findByRole("link", { name: "新对话" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "我的公司" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "项目" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "任务" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "时间线" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "设置" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "语言与外观" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "中文" })).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(MOJIBAKE_PATTERN);
  });

  it("English language switch immediately rerenders core navigation and Settings title", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");
    setLanguage("zh");
    localStorage.setItem("coevo-settings", JSON.stringify({ appearance: { language: "zh" } }));
    render(
      <MemoryRouter initialEntries={["/settings/appearance"]}>
        <App />
      </MemoryRouter>
    );
    fireEvent.click(screen.getByRole("button", { name: "Boot Ready" }));

    fireEvent.change(await screen.findByLabelText("语言"), { target: { value: "en" } });

    expect(await screen.findByRole("link", { name: /New Chat/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /My Company/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Projects/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^Tasks$/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Timeline/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^Settings$/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Language & Appearance/ })).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("新对话");
  });
});
