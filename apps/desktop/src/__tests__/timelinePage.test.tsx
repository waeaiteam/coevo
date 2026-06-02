import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Timeline from "../pages/Timeline";
import { setLanguage } from "../settings/i18n";

const api = vi.hoisted(() => ({
  getGlobalTimeline: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getGlobalTimeline: api.getGlobalTimeline,
}));

describe("Timeline page", () => {
  beforeEach(() => {
    setLanguage("en");
    api.getGlobalTimeline.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("loads company activity from the global timeline endpoint", async () => {
    api.getGlobalTimeline.mockResolvedValue([
      {
        time_ms: 2,
        type: "WorkerSessionCreated",
        title: "Task run started",
        work_order_id: "wo-1",
        track: "green",
        status: "Completed",
        mission_intent: "Summarize local notes",
        details: { session_id: "session-1" },
      },
      {
        time_ms: 1,
        type: "WorkOrderCreated",
        title: "Task created",
        work_order_id: "wo-1",
        track: "green",
        status: "Completed",
        mission_intent: "Summarize local notes",
      },
    ]);

    render(<Timeline />);

    await waitFor(() => expect(api.getGlobalTimeline).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Task started")).toBeInTheDocument();
    expect(screen.getAllByText(/Summarize local notes/).length).toBeGreaterThan(0);
    expect(screen.queryByText("Task records will appear here after you create and run work.")).not.toBeInTheDocument();
  });

  it("shows a friendly empty state when there is no company activity yet", async () => {
    render(<Timeline />);

    await waitFor(() => expect(api.getGlobalTimeline).toHaveBeenCalledTimes(1));
    expect(screen.getByText("Task records will appear here after you create and run work.")).toBeInTheDocument();
  });
});
