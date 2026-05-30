import { describe, it, expect, vi } from "vitest";
import { getHealth, headers, ApiError, get, post, createWorkOrder, getWorkOrderAuditExport, getWorkOrderTimeline, routePlan, testModelConnection } from "../api/client";

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

  it("getWorkOrderTimeline calls the WorkOrder timeline endpoint", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve([{ type: "WorkerRunCreated" }]),
    });

    const result = await getWorkOrderTimeline("wo-123");

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/work-orders/wo-123/timeline",
      expect.any(Object)
    );
    expect(result).toHaveLength(1);
  });

  it("getWorkOrderAuditExport calls the WorkOrder audit export endpoint", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ schema_version: "coevo.audit_export.v1" }),
    });

    const result = await getWorkOrderAuditExport("wo-123");

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/work-orders/wo-123/audit-export",
      expect.any(Object)
    );
    expect(result.schema_version).toBe("coevo.audit_export.v1");
  });

  it("routePlan sends the persisted contract hash anchor", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ plan_hash: "b".repeat(64) }),
    });

    await routePlan({ mcl_version: "1.0" }, ["agent-founder-01"], "a".repeat(64));

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/router/route",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          contract_hash: "a".repeat(64),
          contract: { mcl_version: "1.0" },
          agent_ids: ["agent-founder-01"],
        }),
      })
    );
  });

  it("testModelConnection can test a candidate config without relying on persisted config", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ model: "gpt-4o" }),
    });

    await testModelConnection({ provider_id: "desktop", kind: "OpenAICompatible" });

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/models/test",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ config: { provider_id: "desktop", kind: "OpenAICompatible" } }),
      })
    );
  });

  it("testModelConnection uses an empty body for the active persisted config path", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ model: "gpt-4o" }),
    });

    await testModelConnection();

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/models/test",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({}),
      })
    );
  });

  it("createWorkOrder request does not carry client-side governance decisions", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ work_order_id: "wo-123", track: "green" }),
    });

    await createWorkOrder({
      contract_hash: "a".repeat(64),
      plan_hash: "b".repeat(64),
      user_id: "default-founder",
      opc_id: "default-opc",
      mission_intent: "Analyze the README",
      selected_agents: ["agent-founder-01"],
      selected_executors: [],
      required_skills: ["skill-mission-draft"],
    });

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/work-orders",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          contract_hash: "a".repeat(64),
          plan_hash: "b".repeat(64),
          user_id: "default-founder",
          opc_id: "default-opc",
          mission_intent: "Analyze the README",
          selected_agents: ["agent-founder-01"],
          selected_executors: [],
          required_skills: ["skill-mission-draft"],
        }),
      })
    );
  });
});
