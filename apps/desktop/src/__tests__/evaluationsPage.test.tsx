import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Evaluations from "../pages/Evaluations";
import { setLanguage } from "../settings/i18n";
import { useEvaluationStore } from "../stores/evaluationStore";

const api = vi.hoisted(() => ({
  getAgentGrowth: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getAgentGrowth: api.getAgentGrowth,
}));

describe("Evaluations page", () => {
  beforeEach(() => {
    setLanguage("en");
    localStorage.clear();
    useEvaluationStore.setState({
      evaluators: [],
      jobs: {},
      activeJobId: null,
    });
    api.getAgentGrowth.mockResolvedValue(null);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("removes the placeholder prompt evaluator and exposes configurable Custom RPC fields", async () => {
    render(<Evaluations />);

    await waitFor(() =>
      expect(screen.getByRole("option", { name: /Code Quality Evaluator/i })).toBeInTheDocument(),
    );

    expect(
      screen.queryByRole("option", { name: /Prompt Quality Evaluator/i }),
    ).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: screen.getByRole("option", { name: /Custom RPC Evaluator/i }).getAttribute("value") },
    });

    expect(await screen.findByLabelText(/Endpoint/i)).toHaveValue("");
    expect(screen.getByLabelText(/Auth Token/i)).toHaveValue("");
    expect(screen.getByRole("button", { name: /Run check/i })).toBeDisabled();
  });

  it("persists Custom RPC configuration and uses it for evaluation runs", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        score: 88,
        passed: true,
        details: "Configured endpoint responded",
      }),
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<Evaluations />);

    await waitFor(() =>
      expect(screen.getByRole("option", { name: /Custom RPC Evaluator/i })).toBeInTheDocument(),
    );

    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: screen.getByRole("option", { name: /Custom RPC Evaluator/i }).getAttribute("value") },
    });

    fireEvent.change(await screen.findByLabelText(/Endpoint/i), {
      target: { value: "https://rpc.example.test/evaluate" },
    });
    fireEvent.change(screen.getByLabelText(/Auth Token/i), {
      target: { value: "token-123" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save RPC Config/i }));

    expect(localStorage.getItem("coevo-custom-rpc-evaluator-config")).toContain(
      "https://rpc.example.test/evaluate",
    );

    fireEvent.click(screen.getByRole("button", { name: /Run check/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "https://rpc.example.test/evaluate",
        expect.objectContaining({
          method: "POST",
          headers: expect.objectContaining({
            Authorization: "Bearer token-123",
          }),
        }),
      ),
    );

    await waitFor(() => expect(screen.getByText("custom_score")).toBeInTheDocument());
  });
});
