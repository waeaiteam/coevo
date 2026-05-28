import { describe, it, expect, vi } from "vitest";
import { getHealth, runDemo, headers } from "../api/client";

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
      json: () => Promise.resolve({ status: "ok", version: "1.0.0" }),
    });
    const result = await getHealth();
    expect(result.status).toBe("ok");
  });

  it("runDemo constructs green request", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
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
});
