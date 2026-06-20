import { beforeEach, describe, it, expect, vi } from "vitest";
import { getApiBase, getHealth, headers, ApiError, get, post, createWorkOrder, discoverModels, getWorkOrderAuditExport, getWorkOrderTimeline, routePlan, streamWorkerRunEvents, testModelConnection, setCallerIdentityProof } from "../api/client";

describe("API Client", () => {
  beforeEach(() => {
    vi.unstubAllEnvs();
    localStorage.clear();
  });

  it("constructs coevo metadata headers", () => {
    const h = headers();
    expect(h["Content-Type"]).toBe("application/json");
    expect(h["x-coevo-tenant-id"]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(localStorage.getItem("coevo-tenant-id")).toBe(h["x-coevo-tenant-id"]);
    expect(headers()["x-coevo-tenant-id"]).toBe(h["x-coevo-tenant-id"]);
    expect(h["x-coevo-actor-role"]).toBe("Admin");
    expect(h["x-coevo-contract-hash"]).toHaveLength(64);
    expect(h["x-coevo-policy-version"]).toHaveLength(64);
    expect(h["x-coevo-execution-plan-hash"]).toHaveLength(64);
    expect(h["traceparent"]).toMatch(/^00-[a-f0-9]{32}-[a-f0-9]{16}-01$/);
  });

  it("attaches the local caller identity proof when available", () => {
    localStorage.setItem("coevo-user-id", "default-founder");
    setCallerIdentityProof("ed25519:default-founder:signed-proof");

    const h = headers();

    expect(h["x-coevo-caller-identity-proof"]).toBe("ed25519:default-founder:signed-proof");
    expect(h["x-coevo-actor-id"]).toBe("default-founder");
  });

  it("lets a request override the global active opc header", () => {
    localStorage.setItem("coevo-opc-id", "opc-global");

    const h = (headers as (options: { opcId?: string }) => Record<string, string>)({ opcId: "opc-alpha" });

    expect(h["x-coevo-opc-id"]).toBe("opc-alpha");
  });
  it("getHealth returns ok from mock", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true, status: 200,
      json: () => Promise.resolve({ status: "ok", version: "1.0.0" }),
    });
    const result = await getHealth();
    expect(result.status).toBe("ok");
  });

  it("getApiBase falls back to saved Developer API Base when runtime base is absent", () => {
    localStorage.setItem("coevo-settings", JSON.stringify({
      developer: { api_base_url: "http://127.0.0.1:8727" },
    }));

    expect(getApiBase()).toBe("http://127.0.0.1:8727");
  });

  it("getApiBase lets the runtime API Base override a saved Developer API Base", () => {
    localStorage.setItem("coevo-api-base", "http://127.0.0.1:8717");
    localStorage.setItem("coevo-settings", JSON.stringify({
      developer: { api_base_url: "http://127.0.0.1:8727" },
    }));

    expect(getApiBase()).toBe("http://127.0.0.1:8717");
  });

  it("getApiBase lets the build-time environment API Base drive isolated web dev runs", () => {
    vi.stubEnv("COEVO_API_BASE", "http://127.0.0.1:8729");
    localStorage.setItem("coevo-api-base", "http://127.0.0.1:8717");
    localStorage.setItem("coevo-settings", JSON.stringify({
      developer: { api_base_url: "http://127.0.0.1:8727" },
    }));

    expect(getApiBase()).toBe("http://127.0.0.1:8729");
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

  it("discoverModels tests a candidate config without relying on persisted config", async () => {
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ models: [{ id: "gpt-4o" }] }),
    });

    await discoverModels({ provider_id: "desktop", kind: "OpenAI", api_key: "sk-test" });

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/models/discover",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ config: { provider_id: "desktop", kind: "OpenAI", api_key: "sk-test" } }),
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

  it("streamWorkerRunEvents uses company-scoped headers and parses SSE events", async () => {
    localStorage.setItem("coevo-opc-id", "opc-stream");
    const encoder = new TextEncoder();
    let chunkIndex = 0;
    const chunks = [
      encoder.encode('id: 1\nevent: ContentDelta\ndata: {"event_type":"ContentDelta","payload":{"delta":"Hello"}}\n\n'),
      encoder.encode('id: 2\nevent: Done\ndata: {"event_type":"Done"}\n\n'),
    ];

    globalThis.EventSource = class {
      close() {}
      addEventListener() {}
    } as unknown as typeof EventSource;

    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      body: {
        getReader: () => ({
          read: vi.fn(async () => {
            if (chunkIndex >= chunks.length) {
              return { done: true, value: undefined };
            }
            return { done: false, value: chunks[chunkIndex++] };
          }),
          releaseLock: vi.fn(),
        }),
      },
    });

    const seen: Array<Record<string, unknown>> = [];
    const cleanup = streamWorkerRunEvents("run-stream-1", (event) => {
      seen.push(event);
    });

    await vi.waitFor(() => expect(seen).toHaveLength(2));
    cleanup();

    expect(globalThis.fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8717/opc/workers/runs/run-stream-1/events/stream",
      expect.objectContaining({
        headers: expect.objectContaining({
          Accept: "text/event-stream",
          "x-coevo-opc-id": "opc-stream",
        }),
      }),
    );
    expect(seen[0]).toEqual({
      event_type: "ContentDelta",
      payload: { delta: "Hello" },
    });
    expect(seen[1]).toEqual({
      event_type: "Done",
    });
  });
});
