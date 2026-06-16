// Multi-company data layer (API_CONTRACT).
//
// The `/companies/...` endpoints are owned by the backend (codex Stage 1) and may
// not be live yet. Per COLLAB_PROTOCOL 3, this module consumes the real contract
// endpoints when available and otherwise falls back to a local, contract-shaped shell
// for list/read convenience so the four-layer drilldown UI is fully navigable today.
// Mutations stay backend-authoritative and do not fabricate success on failures.
//
// Strictly contract fields only (no invented fields):
//   Company:       { opc_id, name, mission, employee_count, created_at_ms, dir }
//   CompanyDetail: Company + { charter_md, goals, departments, shared_files_count,
//                              memory_count, report_count }

import { get, post, del } from "./client";
import { getCompanyProfile, listEmployees, listMemory, listWorkOrders } from "./client";
import { getLocalIdentity } from "../settings/identity";

export type Company = {
  opc_id: string;
  name: string;
  mission: string;
  employee_count: number;
  created_at_ms: number;
  dir: string;
};

export type CompanyDetail = Company & {
  charter_md: string;
  goals: Array<Record<string, unknown>>;
  departments: string[];
  shared_files_count: number;
  memory_count: number;
  report_count: number;
};

const LOCAL_COMPANIES_KEY = "coevo-local-companies";
const ACTIVE_OPC_KEY = "coevo-opc-id";

function readLocal(): Company[] {
  try {
    const raw = localStorage.getItem(LOCAL_COMPANIES_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as Company[]) : [];
  } catch {
    return [];
  }
}

function writeLocal(rows: Company[]) {
  try {
    localStorage.setItem(LOCAL_COMPANIES_KEY, JSON.stringify(rows));
  } catch {
    /* ignore persistence failures */
  }
}

function readActiveOpcId(): string {
  try {
    return localStorage.getItem(ACTIVE_OPC_KEY) || "";
  } catch {
    return "";
  }
}

export function getActiveOpcId(): string {
  try {
    return localStorage.getItem(ACTIVE_OPC_KEY) || getLocalIdentity().opcId;
  } catch {
    return getLocalIdentity().opcId;
  }
}

export function setActiveOpcId(opcId: string) {
  try {
    localStorage.setItem(ACTIVE_OPC_KEY, opcId);
  } catch {
    /* ignore */
  }
}

function chooseCompany(
  companies: Company[],
  preferredOpcId?: string,
  preferredName?: string,
): Company | null {
  if (companies.length === 0) return null;

  const trimmedPreferredId = String(preferredOpcId || "").trim();
  if (trimmedPreferredId) {
    const byId = companies.find((company) => company.opc_id === trimmedPreferredId);
    if (byId) return byId;
  }

  const trimmedPreferredName = String(preferredName || "").trim().toLowerCase();
  if (trimmedPreferredName) {
    const exactMatches = companies.filter(
      (company) => company.name.trim().toLowerCase() === trimmedPreferredName,
    );
    if (exactMatches.length === 1) return exactMatches[0];
  }

  return [...companies].sort((left, right) => right.created_at_ms - left.created_at_ms)[0];
}

async function fetchCanonicalCompanies(): Promise<Company[]> {
  try {
    const rows = await get<Company[]>("/companies");
    return Array.isArray(rows) ? rows : [];
  } catch {
    return [];
  }
}

// The current backend serves a single company. Represent it as the founder's first
// company so the list view always has at least one real, enterable company.
async function currentCompanyShell(): Promise<Company> {
  const identity = getLocalIdentity();
  let name = identity.opcName;
  let mission = "";
  try {
    const profile = (await getCompanyProfile()) as Record<string, unknown>;
    name = String(profile?.name || name);
    mission = String(profile?.mission || "");
  } catch {
    /* fall back to local identity */
  }
  let employeeCount = 0;
  try {
    const employees = await listEmployees();
    employeeCount = Array.isArray(employees) ? employees.length : 0;
  } catch {
    /* leave at 0 */
  }
  return {
    opc_id: identity.opcId,
    name,
    mission,
    employee_count: employeeCount,
    created_at_ms: 0,
    dir: `~/.coevo/${identity.opcId}`,
  };
}

export async function listCompanies(): Promise<Company[]> {
  const rows = await fetchCanonicalCompanies();
  if (rows.length > 0) return rows;
  try {
    /* backend not ready: use shell */
  } catch {
    /* no-op */
  }
  const current = await currentCompanyShell();
  const locals = readLocal().filter((row) => row.opc_id !== current.opc_id);
  return [current, ...locals];
}

export async function createCompany(input: { name: string; mission?: string }): Promise<Company> {
  const created = await post<Company>("/companies", input);
  if (created && created.opc_id) return created;
  throw new Error("Company creation failed");
}

export async function ensureActiveCompany(options?: {
  createIfMissing?: boolean;
  preferredName?: string;
  preferredMission?: string;
}): Promise<Company | null> {
  const canonicalCompanies = await fetchCanonicalCompanies();
  const identity = getLocalIdentity();
  const preferredName = options?.preferredName || identity.opcName;
  const preferredOpcId = readActiveOpcId();

  if (canonicalCompanies.length > 0) {
    const chosen = chooseCompany(canonicalCompanies, preferredOpcId, preferredName);
    if (!chosen) return null;
    setActiveOpcId(chosen.opc_id);
    return chosen;
  }

  if (!options?.createIfMissing) {
    return null;
  }

  const created = await createCompany({
    name: preferredName.trim() || "My AI Startup",
    mission: options?.preferredMission?.trim() || undefined,
  });
  setActiveOpcId(created.opc_id);
  return created;
}

export async function deleteCompany(opcId: string): Promise<{ ok: boolean }> {
  const res = await del<{ ok: boolean }>(`/companies/${encodeURIComponent(opcId)}`);
  if (res && res.ok) return res;
  throw new Error("Company deletion failed");
}

export async function getCompanyDetail(opcId: string): Promise<CompanyDetail> {
  try {
    const detail = await get<CompanyDetail>(`/companies/${encodeURIComponent(opcId)}`);
    if (detail && detail.opc_id) return detail;
  } catch {
    /* backend not ready: assemble a shell from existing endpoints */
  }

  const companies = await listCompanies();
  const base =
    companies.find((row) => row.opc_id === opcId) ||
    (await currentCompanyShell());

  const departments = new Set<string>();
  let employeeCount = base.employee_count;
  let memoryCount = 0;
  try {
    const employees = await listEmployees();
    if (Array.isArray(employees)) {
      employeeCount = employees.length;
      for (const e of employees) {
        const dept = String((e as Record<string, unknown>).department || "");
        if (dept) departments.add(dept);
      }
    }
  } catch {
    /* keep defaults */
  }
  try {
    const memories = await listMemory({ scope: "company" });
    memoryCount = Array.isArray(memories) ? memories.length : 0;
  } catch {
    /* keep 0 */
  }
  let charterMd = "";
  try {
    const profile = (await getCompanyProfile()) as Record<string, unknown>;
    const principles = Array.isArray(profile?.operating_principles)
      ? (profile.operating_principles as unknown[]).map(String)
      : [];
    charterMd = principles.length ? principles.map((p) => `- ${p}`).join("\n") : "";
  } catch {
    /* no charter yet */
  }

  return {
    ...base,
    employee_count: employeeCount,
    charter_md: charterMd,
    goals: [],
    departments: [...departments],
    shared_files_count: 0,
    memory_count: memoryCount,
    report_count: 0,
  };
}

// Re-exported so company-scoped pages can fetch the active company's working data
// through one import while the backend transitions to path-based addressing.
export async function companyWorkOrders() {
  try {
    return await listWorkOrders();
  } catch {
    return [];
  }
}



