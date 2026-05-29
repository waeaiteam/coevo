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
}));

vi.mock("../api/client", () => ({
  getApiBase: () => "http://127.0.0.1:8717",
  updateModelConfig: api.updateModelConfig,
  testModelConnection: api.testModelConnection,
}));

describe("Desktop onboarding", () => {
  beforeEach(() => {
    localStorage.clear();
    api.updateModelConfig.mockReset();
    api.testModelConnection.mockReset();
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

  it("Save & Test Connection marks the model provider as configured after success", async () => {
    api.updateModelConfig.mockResolvedValue({});
    api.testModelConnection.mockResolvedValue({
      model: "gpt-4o",
      latency_ms: 12,
      provider_kind: "OpenAICompatible",
    });

    render(
      <MemoryRouter initialEntries={["/settings/model_provider"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: "Save & Test Connection" }));

    await waitFor(() => expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBe("true"));
    expect(screen.getByRole("button", { name: "Continue to Mission Chat" })).toBeInTheDocument();
  });
});
