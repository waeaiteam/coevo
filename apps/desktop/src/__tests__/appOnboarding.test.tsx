import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Outlet } from "react-router-dom";
import App from "../App";
import * as companiesApi from "../api/companies";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";

const api = vi.hoisted(() => ({
  getModelConfig: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getModelConfig: api.getModelConfig,
}));

vi.mock("../components/BootPage", () => ({
  default: ({ onReady }: { onReady: () => void }) => (
    <button onClick={onReady}>Boot Ready</button>
  ),
}));

vi.mock("../pages/MissionChat", () => ({
  default: () => <div>Mission Chat Ready</div>,
}));

vi.mock("../components/Layout", () => ({
  default: () => <Outlet />,
}));

describe("App onboarding gate", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
    api.getModelConfig.mockResolvedValue({
      kind: "DeepSeek",
      has_api_key: true,
      default_model: "deepseek-chat",
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("does not show FirstRun after boot when the model provider is configured", async () => {
    const ensureActiveCompany = vi
      .spyOn(companiesApi, "ensureActiveCompany")
      .mockResolvedValue({
        opc_id: "opc-live-001",
        name: "Live Co",
        mission: "Ship",
        employee_count: 1,
        created_at_ms: 1,
        dir: "~/.coevo/opc-live-001",
      });
    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>
    );

    screen.getByRole("button", { name: "Boot Ready" }).click();

    await waitFor(() => expect(screen.getByText("Mission Chat Ready")).toBeInTheDocument());
    expect(screen.queryByText("Welcome to coevo")).not.toBeInTheDocument();
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBe("true");
    expect(ensureActiveCompany).toHaveBeenCalledTimes(1);
  });

  it("supports the /mission deep link for the New Task workspace", async () => {
    render(
      <MemoryRouter initialEntries={["/mission"]}>
        <App />
      </MemoryRouter>
    );

    screen.getByRole("button", { name: "Boot Ready" }).click();

    await waitFor(() => expect(screen.getByText("Mission Chat Ready")).toBeInTheDocument());
  });

  it("reopens FirstRun when localStorage says configured but the fresh backend has no active provider", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");
    api.getModelConfig.mockRejectedValue(new Error("MODEL_PROVIDER_NOT_CONFIGURED"));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>
    );

    screen.getByRole("button", { name: "Boot Ready" }).click();

    await waitFor(() => expect(screen.getByText("Create your AI company")).toBeInTheDocument());
    expect(screen.queryByText("Mission Chat Ready")).not.toBeInTheDocument();
    expect(localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY)).toBeNull();
  });
});
