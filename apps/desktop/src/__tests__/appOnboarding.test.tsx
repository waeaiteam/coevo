import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Outlet } from "react-router-dom";
import App from "../App";
import { MODEL_PROVIDER_CONFIGURED_KEY } from "../settings/onboarding";

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
  });

  afterEach(() => {
    cleanup();
  });

  it("does not show FirstRun after boot when the model provider is configured", async () => {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>
    );

    screen.getByRole("button", { name: "Boot Ready" }).click();

    await waitFor(() => expect(screen.getByText("Mission Chat Ready")).toBeInTheDocument());
    expect(screen.queryByText("Welcome to coevo")).not.toBeInTheDocument();
  });
});
