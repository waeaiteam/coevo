import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import FirstRun from "../components/FirstRun";
import Settings from "../pages/Settings";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";

const api = vi.hoisted(() => ({
  updateModelConfig: vi.fn(),
  testModelConnection: vi.fn(),
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getApiBase: () => "http://127.0.0.1:8717",
  updateModelConfig: api.updateModelConfig,
  testModelConnection: api.testModelConnection,
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

describe("Desktop onboarding", () => {
  beforeEach(() => {
    localStorage.clear();
    api.updateModelConfig.mockReset();
    api.testModelConnection.mockReset();
    api.listEmployees.mockReset();
    api.seedEmployees.mockReset();
    api.listSkills.mockReset();
    api.seedSkills.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("FirstRun does not show the mock quick start path", () => {
    const oldQuickStart = ["Quick Start", "with Mock"].join(" ");
    const oldMockCopy = new RegExp(["Mock mode", "uses"].join(" "), "i");

    render(
      <MemoryRouter>
        <FirstRun onDone={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.queryByText(oldQuickStart)).not.toBeInTheDocument();
    expect(screen.queryByText(oldMockCopy)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Configure Model Provider|Enter API Key/i })).toBeInTheDocument();
  });

  it("FirstRun opens model provider settings", async () => {
    const onDone = vi.fn();
    render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<FirstRun onDone={onDone} />} />
          <Route path="/settings/model_provider" element={<div>Model Provider Settings</div>} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /Configure Model Provider|Enter API Key/i }));

    expect(onDone).toHaveBeenCalled();
    expect(await screen.findByText("Model Provider Settings")).toBeInTheDocument();
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

    expect(screen.getByRole("combobox")).toHaveValue("openai-compatible");
    expect(screen.queryByRole("option", { name: /Mock/i })).not.toBeInTheDocument();
  });

  it("Save & Test Connection marks the model provider as configured after success", async () => {
    api.updateModelConfig.mockResolvedValue({});
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAICompatible",
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

    fireEvent.click(screen.getByRole("button", { name: "Save & Test Connection" }));

    await waitFor(() => expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBe("true"));
    expect(api.seedEmployees).toHaveBeenCalledTimes(1);
    expect(api.seedSkills).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Continue to Mission Chat" })).toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "Save & Test Connection" }));

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

    fireEvent.click(screen.getByRole("button", { name: "Save & Test Connection" }));

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

    fireEvent.click(screen.getByRole("button", { name: "Save & Test Connection" }));

    await waitFor(() => expect(screen.getByText(/connection failed/i)).toBeInTheDocument());
    expect(api.updateModelConfig).not.toHaveBeenCalled();
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBeNull();
  });
});
