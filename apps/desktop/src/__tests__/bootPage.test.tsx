import { cleanup, render, waitFor } from "@testing-library/react";
import React from "react";
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

  it("does not invoke launch_server twice under React StrictMode", async () => {
    const invoke = vi.fn().mockResolvedValue("http://127.0.0.1:8718");
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    Object.defineProperty(window, "__TAURI__", {
      value: { core: { invoke } },
      configurable: true,
    });

    render(
      <React.StrictMode>
        <BootPage onReady={vi.fn()} />
      </React.StrictMode>
    );

    await waitFor(() => expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8718"));
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("uses saved Developer API Base in web mode", async () => {
    localStorage.setItem("coevo-settings", JSON.stringify({
      developer: { api_base_url: "http://127.0.0.1:8727" },
    }));

    render(<BootPage onReady={vi.fn()} />);

    await waitFor(() => expect(localStorage.getItem("coevo-api-base")).toBe("http://127.0.0.1:8727"));
    expect(globalThis.fetch).toHaveBeenCalledWith("http://127.0.0.1:8727/health");
  });
});
