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

export class ApiError extends Error {
  status: number;
  payload: unknown;
  constructor(status: number, message: string, payload?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.payload = payload;
  }
}

async function handleResponse(res: Response) {
  if (!res.ok) {
    let body: Record<string,unknown> = {};
    try { body = await res.json(); } catch { /* use empty */ }
    const msg = (body.error as string) || (body.message as string) || `HTTP ${res.status}`;
    throw new ApiError(res.status, msg, body);
  }
  try { return await res.json(); } catch { return {}; }
}

export async function get<T = unknown>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { headers: headers() });
  return handleResponse(res) as Promise<T>;
}

export async function post<T = unknown>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify(body),
  });
  return handleResponse(res) as Promise<T>;
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
export async function getCompanyProfile(): Promise<Record<string,unknown>> { return get("/opc/profile/company"); }
export async function updateCompanyProfile(p: Record<string,unknown>) { return put("/opc/profile/company", p); }
export async function listMemory(params?: Record<string,string>) {
  const qs = params ? "?" + new URLSearchParams(params).toString() : "";
  return get(`/opc/memory${qs}`);
}
export async function createMemory(m: Record<string,unknown>) { return post("/opc/memory", m); }
export async function markMemoryStale(id: string) { return post(`/opc/memory/${id}/stale`, {}); }
export async function revokeMemory(id: string) { return post(`/opc/memory/${id}/revoke`, {}); }
export async function searchMemory(q: string) { return get(`/opc/memory?q=${encodeURIComponent(q)}`); }
export async function listEmployees(): Promise<Record<string,unknown>[]> { return get("/opc/agents/employees"); }
export async function seedEmployees() { return post("/opc/agents/employees/seed", {}); }
export async function getAgentMemory(agentId: string) { return get(`/opc/agents/employees/${agentId}/memory`); }
export async function listExecutors(): Promise<Record<string,unknown>[]> { return get("/opc/executors"); }
export async function registerExecutor(p: Record<string,unknown>) { return post("/opc/executors/register", p); }
export async function disableExecutor(id: string) { return post(`/opc/executors/${id}/disable`, {}); }
export async function executorHealth(id: string) { return post(`/opc/executors/${id}/health`, {}); }
export async function executorDryRun(executorId: string, workOrderId: string) { return post(`/opc/executors/${executorId}/dry-run`, { work_order_id: workOrderId }); }
export async function listSkills(agentId?: string): Promise<Record<string,unknown>[]> {
  const p = agentId ? `?agent_id=${encodeURIComponent(agentId)}` : "";
  return get(`/opc/skills${p}`);
}
export async function seedSkills() { return post("/opc/skills/seed", {}); }
export async function activateSkill(skillId: string, version: string) { return post(`/opc/skills/${skillId}/${version}/activate`, {}); }
export async function rollbackSkill(skillId: string, version: string) { return post(`/opc/skills/${skillId}/${version}/rollback`, {}); }
export async function listSkillProposals(): Promise<Record<string,unknown>[]> { return get("/opc/skills/evolution/proposals"); }
export async function runEvolution() { return post("/opc/skills/evolution/run", {}); }
export async function verifySkillProposal(id: string) { return post(`/opc/skills/evolution/proposals/${id}/verify`, {}); }
export async function approveSkillProposal(id: string) { return post(`/opc/skills/evolution/proposals/${id}/approve`, {}); }
export async function rejectSkillProposal(id: string) { return post(`/opc/skills/evolution/proposals/${id}/reject`, {}); }
export async function listWorkOrders(): Promise<Record<string,unknown>[]> { return get("/opc/work-orders"); }
export async function createWorkOrder(wo: Record<string,unknown>) { return post("/opc/work-orders", wo); }
export async function executeWorkOrder(id: string, req: Record<string,unknown> = {}) { return post(`/opc/work-orders/${id}/execute`, req); }
export async function cancelWorkOrder(id: string) { return post(`/opc/work-orders/${id}/cancel`, {}); }
export async function submitWorkOrderFeedback(id: string, feedback: string, agentId?: string) { return post(`/opc/work-orders/${id}/feedback`, { feedback, agent_id: agentId }); }

// === Model Gateway ===
export async function getModelConfig() { return get("/opc/models/config"); }
export async function updateModelConfig(config: Record<string,unknown>) { return put("/opc/models/config", config); }
export async function testModelConnection() { return post("/opc/models/test", {}); }
export async function modelChat(payload: Record<string,unknown>) { return post("/opc/models/chat", payload); }
export async function modelStructured(payload: Record<string,unknown>) { return post("/opc/models/structured", payload); }

async function put<T=unknown>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { method: "PUT", headers: headers(), body: JSON.stringify(body) });
  return handleResponse(res) as Promise<T>;
}
