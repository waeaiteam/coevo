import { describe, it, expect, vi } from "vitest";
import { getHealth, runDemo, headers, ApiError, get, post } from "../api/client";

describe("API Client", () => {
  it("constructs coevo metadata headers", () => {
    const h = headers();
    expect(h["Content-Type"]).toBe("application/json");
    expect(h["x-coevo-tenant-id"]).toBe("desktop-tenant");
    expect(h["x-coevo-actor-role"]).toBe("Admin");
    expect(h["x-coevo-contract-hash"]).toHaveLength(64);
    expect(h["x-coevo-policy-version"]).toHaveLength(64);
    expect(h["x-coevo-execution-plan-hash"]).toHaveLength(64);
    expect(h["traceparent"]).toMatch(/^00-[a-f0-9]{32}-[a-f0-9]{16}-01$/);
  });

  it("getHealth returns ok from mock", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true, status: 200,
      json: () => Promise.resolve({ status: "ok", version: "1.0.0" }),
    });
    const result = await getHealth();
    expect(result.status).toBe("ok");
  });

  it("runDemo constructs green request", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true, status: 200,
      json: () =>
        Promise.resolve({
          track: "green",
          contract_hash: "a".repeat(64),
          plan_hash: "b".repeat(64),
          traceparent: "00-aaa-bbb-01",
          ambiguity_score: 0.2,
          warnings: [],
          entries_created: ["key@v1"],
          elapsed_ms: 42,
        }),
    });
    const result = await runDemo("green");
    expect(result.track).toBe("green");
    expect(result.contract_hash).toHaveLength(64);
    expect(result.entries_created).toHaveLength(1);
  });

  it("post throws ApiError on 400", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: false, status: 400,
      json: () => Promise.resolve({ error: "MissingApiKey" }),
    });
    await expect(post("/opc/models/test", {})).rejects.toThrow(ApiError);
    try { await post("/opc/models/test", {}); } catch(e) {
      expect(e instanceof ApiError).toBe(true);
      expect((e as ApiError).status).toBe(400);
    }
  });

  it("get throws ApiError on 403", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: false, status: 403,
      json: () => Promise.resolve({ error: "Forbidden" }),
    });
    await expect(get("/opc/work-orders/x/execute")).rejects.toThrow(ApiError);
  });
});