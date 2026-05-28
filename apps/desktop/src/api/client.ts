import type { HealthResponse, ContractResponse, DemoResponse } from "../types";

export type { HealthResponse, ContractResponse, DemoResponse } from "../types";

const API_BASE = (() => { try { return localStorage.getItem("coevo-api-base") || "http://127.0.0.1:8717"; } catch { return "http://127.0.0.1:8717"; } })();

export function headers(): Record<string, string> {
  return {
    "Content-Type": "application/json",
    "x-coevo-tenant-id": "desktop-tenant",
    "x-coevo-actor-role": "Admin",
    "x-coevo-contract-hash": "0".repeat(64),
    "x-coevo-policy-version": "0".repeat(64),
    "x-coevo-execution-plan-hash": "0".repeat(64),
    "x-coevo-causality-parent-id": crypto.randomUUID(),
    "x-coevo-idempotency-key": crypto.randomUUID(),
    "x-coevo-request-ttl-ms": "30000",
    "x-coevo-replay-mode": "false",
    "x-coevo-timestamp": String(Date.now()),
    traceparent: `00-${crypto.randomUUID().replace(/-/g, "")}-${Array.from({length:16},()=>Math.floor(Math.random()*16).toString(16)).join("")}-01`,
  };
}

export async function get<T = unknown>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { headers: headers() });
  return res.json();
}

export async function post<T = unknown>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify(body),
  });
  return res.json();
}

export async function getHealth(): Promise<HealthResponse> {
  return get("/health");
}

export async function compileContract(userIntent: string, mode = "DRAFT"): Promise<ContractResponse> {
  return post("/mcl/compile", { user_intent: userIntent, requested_mode: mode, parent_contract_hash: null });
}

export async function routePlan(contract: unknown, agentIds: string[]) {
  return post("/router/route", { contract, agent_ids: agentIds });
}

export async function proposeFact(request: unknown): Promise<Record<string, unknown>> {
  return post("/customs/propose", request);
}

export async function evaluateRisk(request: unknown) {
  return post("/risk/evaluate", request);
}

export async function resolveConflict(request: unknown) {
  return post("/resolution/process", request);
}

export async function runDemo(track: "green" | "yellow" | "red"): Promise<DemoResponse> {
  return post(`/demo/${track}`, { tenant_id: "desktop-demo", agent_ids: ["agent-synthesizer-01"] });
}

// === OPC API ===
export async function getUserProfile(): Promise<Record<string,unknown>> { return get("/opc/profile/user"); }
export async function updateUserProfile(p: Record<string,unknown>) { return put("/opc/profile/user", p); }
export async function listMemory(params?: Record<string,string>) {
  const qs = params ? "?" + new URLSearchParams(params).toString() : "";
  return get(`/opc/memory${qs}`);
}
export async function createMemory(m: Record<string,unknown>) { return post("/opc/memory", m); }
export async function staleMemory(id: string) { return post(`/opc/memory/${id}/stale`, {}); }
export async function revokeMemory(id: string) { return post(`/opc/memory/${id}/revoke`, {}); }
export async function listEmployees(): Promise<Record<string,unknown>[]> { return get("/opc/agents/employees"); }
export async function seedEmployees() { return post("/opc/agents/employees/seed", {}); }
export async function listWorkOrders(): Promise<Record<string,unknown>[]> { return get("/opc/work-orders"); }
export async function createWorkOrder(wo: Record<string,unknown>) { return post("/opc/work-orders", wo); }

async function put<T=unknown>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { method: "PUT", headers: headers(), body: JSON.stringify(body) });
  return res.json();
}
