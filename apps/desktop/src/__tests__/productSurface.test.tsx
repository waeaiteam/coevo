import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import Dashboard from "../pages/Dashboard";
import ExternalExecutors from "../pages/ExternalExecutors";
import Settings from "../pages/Settings";
import App from "../App";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";

const api = vi.hoisted(() => ({
  listExecutors: vi.fn(),
  registerExecutor: vi.fn(),
  disableExecutor: vi.fn(),
  executorHealth: vi.fn(),
  executorDryRun: vi.fn(),
  listWorkOrders: vi.fn(),
  discoverModels: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getApiBase: () => "http://127.0.0.1:8717",
  listExecutors: api.listExecutors,
  registerExecutor: api.registerExecutor,
  disableExecutor: api.disableExecutor,
  executorHealth: api.executorHealth,
  executorDryRun: api.executorDryRun,
  listWorkOrders: api.listWorkOrders,
  discoverModels: api.discoverModels,
}));

vi.mock("../components/BootPage", () => ({
  default: ({ onReady }: { onReady: () => void }) => (
    <button onClick={onReady}>Boot Ready</button>
  ),
}));

describe("ordinary user product surface", () => {
  beforeEach(() => {
    localStorage.clear();
    api.listExecutors.mockResolvedValue([]);
    api.listWorkOrders.mockResolvedValue([]);
    api.registerExecutor.mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("Dashboard does not expose demo action controls", () => {
    localStorage.setItem("coevo-opc-name", "WAE AI Team");
    localStorage.setItem("coevo-user-name", "Wae");
    localStorage.setItem("coevo-opc-id", "opc-123");

    render(<Dashboard />);

    expect(screen.queryByText("Demo Actions")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "WAE AI Team" })).toBeInTheDocument();
    expect(screen.getByText("Owner")).toBeInTheDocument();
    expect(screen.getByText("Wae")).toBeInTheDocument();
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

    await waitFor(() => expect(screen.getByRole("link", { name: /New Chat/i })).toBeInTheDocument());
    expect(screen.getByRole("link", { name: /^OPC$/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /WorkOrders/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Audit/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Settings/i })).toBeInTheDocument();

    expect(screen.queryByRole("link", { name: /AI Employees/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^Skills$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Executors/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^Contracts$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^Plans$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Risk Gate/i })).not.toBeInTheDocument();
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
});
