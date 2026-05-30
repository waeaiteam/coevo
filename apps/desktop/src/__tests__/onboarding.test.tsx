import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import FirstRun from "../components/FirstRun";
import Settings from "../pages/Settings";
import { t } from "../settings/i18n";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";

const api = vi.hoisted(() => ({
  createMemory: vi.fn(),
  updateCompanyProfile: vi.fn(),
  updateModelConfig: vi.fn(),
  updateUserProfile: vi.fn(),
  testModelConnection: vi.fn(),
  discoverModels: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

vi.mock("../api/client", () => ({
  createMemory: api.createMemory,
  getApiBase: () => "http://127.0.0.1:8717",
  updateCompanyProfile: api.updateCompanyProfile,
  updateModelConfig: api.updateModelConfig,
  updateUserProfile: api.updateUserProfile,
  testModelConnection: api.testModelConnection,
  discoverModels: api.discoverModels,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

describe("Desktop onboarding", () => {
  beforeEach(() => {
    localStorage.clear();
    api.createMemory.mockReset();
    api.updateCompanyProfile.mockReset();
    api.updateModelConfig.mockReset();
    api.updateUserProfile.mockReset();
    api.testModelConnection.mockReset();
    api.discoverModels.mockReset();
    api.listEmployees.mockReset();
    api.seedEmployees.mockReset();
    api.listSkills.mockReset();
    api.seedSkills.mockReset();
    api.createMemory.mockResolvedValue({ ok: true });
    api.updateCompanyProfile.mockResolvedValue({ ok: true });
    api.updateUserProfile.mockResolvedValue({ ok: true });
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
      { agent_id: "agent-research-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);
    api.listSkills.mockResolvedValue([{ skill_id: "skill-mission-draft", status: "Active" }]);
    api.seedEmployees.mockResolvedValue({ ok: true, total: 2 });
    api.seedSkills.mockResolvedValue({ ok: true, total: 1 });
  });

  afterEach(() => {
    cleanup();
  });

  it("FirstRun starts with Create OPC instead of raw model setup", () => {
    const oldQuickStart = ["Quick Start", "with Mock"].join(" ");
    const oldMockCopy = new RegExp(["Mock mode", "uses"].join(" "), "i");

    render(
      <MemoryRouter>
        <FirstRun onDone={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.queryByText(oldQuickStart)).not.toBeInTheDocument();
    expect(screen.queryByText(oldMockCopy)).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Create your OPC/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Continue to Company Foundation/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Configure Model Provider|Enter API Key/i })).not.toBeInTheDocument();
  });

  it("FirstRun persists company foundation before opening model setup", async () => {
    const onDone = vi.fn();
    render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<FirstRun onDone={onDone} />} />
          <Route path="/settings/model_provider" element={<div>Model Provider Settings</div>} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.change(screen.getByLabelText(/OPC name/i), {
      target: { value: "WAE AI Team" },
    });
    fireEvent.change(screen.getByLabelText(/Owner name/i), {
      target: { value: "Wae" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Continue to Company Foundation/i }));

    expect(await screen.findByRole("heading", { name: /Company Foundation/i })).toBeInTheDocument();
    await waitFor(() => expect(api.seedEmployees).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(api.seedSkills).toHaveBeenCalledTimes(1));

    fireEvent.change(screen.getByLabelText(/Company mission/i), {
      target: { value: "Build governed desktop agents for product work." },
    });
    fireEvent.change(screen.getByLabelText(/Industry \/ domain/i), {
      target: { value: "AI product operations" },
    });
    fireEvent.change(screen.getByLabelText(/Operating principles/i), {
      target: { value: "Keep Red Track blocked\nRequire audit evidence" },
    });
    fireEvent.change(screen.getByLabelText(/Alpha posture/i), {
      target: { value: "conservative" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Create OPC and continue/i }));

    await waitFor(() => expect(api.updateUserProfile).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(api.updateCompanyProfile).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(api.createMemory).toHaveBeenCalledTimes(1));

    const opcId = localStorage.getItem("coevo-opc-id");
    expect(localStorage.getItem("coevo-opc-name")).toBe("WAE AI Team");
    expect(localStorage.getItem("coevo-user-name")).toBe("Wae");
    expect(localStorage.getItem("coevo-tenant-id")).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(opcId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(api.updateUserProfile).toHaveBeenCalledWith(expect.objectContaining({
      user_id: "default-founder",
      display_name: "Wae",
      preferred_language: "en",
      risk_preference: "conservative",
      default_mission_mode: "read_only",
    }));
    expect(api.updateCompanyProfile).toHaveBeenCalledWith(expect.objectContaining({
      opc_id: opcId,
      founder_user_id: "default-founder",
      name: "WAE AI Team",
      mission: "Build governed desktop agents for product work.",
      operating_principles: ["Keep Red Track blocked", "Require audit evidence"],
      policy_profile: "alpha-conservative",
    }));
    expect(api.createMemory).toHaveBeenCalledWith(expect.objectContaining({
      scope: "Company",
      owner_id: opcId,
      title: "Operating Principles",
      source: "first-run",
      provenance: `first-run:${opcId}:company-foundation`,
      cognitive_layer: "Suggestion",
    }));

    expect(await screen.findByRole("heading", { name: /Model Provider handoff/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Open Model Provider/i }));

    expect(onDone).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Model Provider Settings")).toBeInTheDocument();
  });

  it("FirstRun does not write API keys to localStorage while creating company foundation", async () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<FirstRun onDone={vi.fn()} />} />
          <Route path="/settings/model_provider" element={<div>Model Provider Settings</div>} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /Continue to Company Foundation/i }));
    fireEvent.click(await screen.findByRole("button", { name: /Create OPC and continue/i }));

    await waitFor(() => expect(api.createMemory).toHaveBeenCalledTimes(1));
    expect(JSON.stringify(localStorage)).not.toMatch(/api[_-]?key|sk-live-secret/i);
    expect(localStorage.getItem("coevo-settings") || "").not.toMatch(/api[_-]?key|sk-live-secret/i);
  });

  it("fails fast in Company Foundation when bootstrap seed/list fails", async () => {
    api.seedEmployees.mockRejectedValue(new Error("seed employees failed"));

    render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<FirstRun onDone={vi.fn()} />} />
          <Route path="/settings/model_provider" element={<div>Model Provider Settings</div>} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /Continue to Company Foundation/i }));
    expect(await screen.findByText(/seed employees failed/i)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /Model Provider handoff/i })).not.toBeInTheDocument();

    expect(api.updateUserProfile).not.toHaveBeenCalled();
    expect(api.updateCompanyProfile).not.toHaveBeenCalled();
    expect(api.createMemory).not.toHaveBeenCalled();
  });

  it("Model Providers does not expose the mock provider option", () => {
    const oldProviderLabel = ["Mock / Local", "Test Provider"].join(" ");

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.queryByRole("option", { name: oldProviderLabel })).not.toBeInTheDocument();
  });

  it("Policy Engine settings does not expose the mock option", () => {
    render(
      <MemoryRouter initialEntries={["/settings/policy_engine"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.queryByRole("option", { name: "Mock" })).not.toBeInTheDocument();
  });

  it("normalizes an old stored mock provider to OpenAI Compatible", () => {
    localStorage.setItem(
      "coevo-settings",
      JSON.stringify({
        model_provider: {
          provider: "mock",
          base_url: "",
          api_key: "",
          default_model: "mock-model",
          fast_model: "mock-model",
          reasoning_model: "mock-model",
          structured_output_model: "mock-model",
          max_tokens: 4096,
          temperature: 0.7,
          request_timeout_ms: 30000,
          max_cost_per_task_usd: 0,
        },
      })
    );

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getAllByRole("combobox")[0]).toHaveValue("openai");
    expect(screen.queryByRole("option", { name: /Mock/i })).not.toBeInTheDocument();
    expect(screen.getByLabelText(/Default Model/i)).toHaveValue("gpt-4o");
    expect(screen.getByLabelText(/Fast Model/i)).toHaveValue("gpt-4o-mini");
  });

  it("Test / Discover Models marks the model provider as configured after success and populates model select", async () => {
    api.updateModelConfig.mockResolvedValue({});
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAICompatible",
    });
    api.discoverModels.mockResolvedValue({
      models: [
        { id: "gpt-4o", display_name: "gpt-4o", max_output_tokens: 16384 },
        { id: "gpt-4o-mini", display_name: "gpt-4o-mini", max_output_tokens: 16384 },
        { id: "o3-mini", display_name: "o3-mini", max_output_tokens: 100000 },
      ],
    });
    api.listEmployees
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }]);
    api.listSkills
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ skill_id: "skill-mission-draft", status: "Active" }]);
    api.seedEmployees.mockResolvedValue({ ok: true });
    api.seedSkills.mockResolvedValue({ ok: true });

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Test / Discover Models" }));

    await waitFor(() => expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBe("true"));
    expect(api.discoverModels).toHaveBeenCalledTimes(1);
    expect(screen.getAllByRole("option", { name: "gpt-4o-mini" }).length).toBeGreaterThan(0);
    expect(screen.getByLabelText(/Default Model/i)).toHaveValue("gpt-4o");
    expect(screen.getByLabelText(/Fast Model/i)).toHaveValue("gpt-4o-mini");
    expect(screen.getByLabelText(/Reasoning Model/i)).toHaveValue("o3-mini");
    expect(screen.getByLabelText(/Structured Output Model/i)).toHaveValue("gpt-4o");
    expect(api.updateModelConfig).toHaveBeenCalledWith(expect.objectContaining({
      default_model: "gpt-4o",
      fast_model: "gpt-4o-mini",
      reasoning_model: "o3-mini",
      structured_output_model: "gpt-4o",
    }));
    expect(api.seedEmployees).toHaveBeenCalledTimes(1);
    expect(api.seedSkills).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Continue to Mission Chat" })).toBeInTheDocument();
  });

  it("provider selection autofills the default base URL in Advanced", () => {
    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.change(screen.getByLabelText(/^Provider$/i), { target: { value: "anthropic" } });
    fireEvent.click(screen.getByRole("button", { name: /Advanced/i }));

    expect(screen.getByLabelText(/Base URL/i)).toHaveValue("https://api.anthropic.com/v1");
  });

  it("does not write API keys to localStorage while testing and discovering models", async () => {
    api.updateModelConfig.mockResolvedValue({});
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAI",
    });
    api.discoverModels.mockResolvedValue({ models: [{ id: "gpt-4o", display_name: "gpt-4o" }] });
    api.listEmployees.mockResolvedValue([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }]);
    api.listSkills.mockResolvedValue([{ skill_id: "skill-mission-draft", status: "Active" }]);

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.change(screen.getByLabelText(/API Key/i), { target: { value: "sk-live-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Test / Discover Models" }));

    await waitFor(() => expect(api.updateModelConfig).toHaveBeenCalledTimes(1));
    expect(JSON.stringify(localStorage)).not.toContain("sk-live-secret");
    expect(localStorage.getItem("coevo-settings") || "").not.toContain("sk-live-secret");
  });

  it("does not mark the model provider configured when workspace bootstrap fails", async () => {
    api.updateModelConfig.mockResolvedValue({});
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAICompatible",
    });
    api.listEmployees.mockRejectedValue(new Error("database unavailable"));

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Test / Discover Models" }));

    await waitFor(() => expect(screen.getByText(/Workspace bootstrap failed/i)).toBeInTheDocument());
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBeNull();
  });

  it("does not mark the model provider configured when model config save fails", async () => {
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAICompatible",
    });
    api.updateModelConfig.mockRejectedValue(new Error("config rejected"));

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Test / Discover Models" }));

    await waitFor(() => expect(screen.getByText(/config rejected/i)).toBeInTheDocument());
    expect(api.testModelConnection).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBeNull();
  });

  it("does not mark the model provider configured when test connection fails", async () => {
    api.testModelConnection.mockRejectedValue(new Error("connection failed"));

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Test / Discover Models" }));

    await waitFor(() => expect(screen.getByText(/connection failed/i)).toBeInTheDocument());
    expect(api.updateModelConfig).not.toHaveBeenCalled();
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBeNull();
  });

  it("Model Provider hides advanced transport and token fields by default", () => {
    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByText("OpenAI")).toBeInTheDocument();
    expect(screen.queryByText(t("settings.base_url"))).not.toBeInTheDocument();
    expect(screen.queryByText(t("settings.max_tokens"))).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Advanced/i }));

    expect(screen.getByText(t("settings.base_url"))).toBeInTheDocument();
    expect(screen.getByText(t("settings.max_tokens"))).toBeInTheDocument();
  });
});
