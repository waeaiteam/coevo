import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import BootPage from "../components/BootPage";

describe("BootPage", () => {
  beforeEach(() => {
    localStorage.clear();
    globalThis.fetch = vi.fn().mockResolvedValue({ ok: true });
  });

  afterEach(() => {
    cleanup();
    delete (window as unknown as Record<string, unknown>).__TAURI__;
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
  });

  it("uses the Tauri global invoke and stores the dynamic API base", async () => {
    const invoke = vi.fn().mockResolvedValue("http://127.0.0.1:8718");
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    Object.defineProperty(window, "__TAURI__", {
      value: { core: { invoke } },
      configurable: true,
    });

    render(<BootPage onReady={vi.fn()} />);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("launch_server"));
    expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8718");
    expect(globalThis.fetch).toHaveBeenCalledWith("http://127.0.0.1:8718/health");
  });
});
