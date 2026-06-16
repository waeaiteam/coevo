import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StreamDisplay } from "../components/WorkerStreamView";
import type { StreamController } from "../hooks/useWorkerStream";

describe("Worker stream public surface", () => {
  it("renders reasoning when it arrives from the worker stream", () => {
    const stream: StreamController = {
      state: "completed",
      content: "Final customer-facing summary.",
      reasoning: "internal rationale",
      toolCalls: [],
      toolExecutions: [],
      usage: null,
      error: null,
      reconnecting: false,
      reconnectAttempt: 0,
      start: vi.fn(),
      stop: vi.fn(),
      retry: vi.fn(),
    };

    render(<StreamDisplay stream={stream} />);

    expect(screen.getByText("Thought process")).toBeInTheDocument();
    expect(screen.getByText("internal rationale")).toBeInTheDocument();
    expect(screen.getByText("Final customer-facing summary.")).toBeInTheDocument();
  });
});
