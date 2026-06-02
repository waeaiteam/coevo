export type ProductRow = Record<string, unknown>;

export type ProjectSummary = {
  id: string;
  name: string;
  description: string;
  folder: string;
  status: "active" | "waiting" | "done";
  updatedAtMs: number;
  conversations: ProductRow[];
  tasks: ProductRow[];
  memories: ProductRow[];
};

export function stringField(row: ProductRow | undefined, key: string): string {
  const value = row?.[key];
  return value == null ? "" : String(value);
}

export function numberField(row: ProductRow | undefined, key: string): number {
  const value = Number(row?.[key] || 0);
  return Number.isFinite(value) ? value : 0;
}

export function listField(row: ProductRow | undefined, key: string): string[] {
  const value = row?.[key];
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (typeof value === "string" && value.trim()) return [value.trim()];
  return [];
}

export function shortText(value: unknown, max = 82): string {
  const text = String(value || "").replace(/\s+/g, " ").trim();
  if (text.length <= max) return text;
  return `${text.slice(0, Math.max(0, max - 3))}...`;
}

export function formatRelativeTime(ms: number): string {
  if (!ms) return "-";
  const delta = Date.now() - ms;
  if (delta < 60_000) return "just now";
  if (delta < 3_600_000) return `${Math.max(1, Math.round(delta / 60_000))}m ago`;
  if (delta < 86_400_000) return `${Math.max(1, Math.round(delta / 3_600_000))}h ago`;
  return `${Math.max(1, Math.round(delta / 86_400_000))}d ago`;
}

export function projectSlug(name: string): string {
  const clean = name.trim().toLowerCase();
  const ascii = clean
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return ascii || "general";
}

function normalizeProjectValue(value: unknown, index: number): ProjectSummary {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const row = value as ProductRow;
    const name = stringField(row, "name") || stringField(row, "title") || `Project ${index + 1}`;
    return {
      id: stringField(row, "id") || projectSlug(name),
      name,
      description: stringField(row, "description"),
      folder: stringField(row, "folder") || stringField(row, "path"),
      status: "active",
      updatedAtMs: numberField(row, "updated_at_ms"),
      conversations: [],
      tasks: [],
      memories: [],
    };
  }
  const name = String(value || `Project ${index + 1}`);
  return {
    id: projectSlug(name),
    name,
    description: "",
    folder: "",
    status: "active",
    updatedAtMs: 0,
    conversations: [],
    tasks: [],
    memories: [],
  };
}

function rawList(row: ProductRow | null | undefined, key: string): unknown[] {
  const value = row?.[key];
  return Array.isArray(value) ? value : [];
}

export function extractProjectNameFromText(value: unknown): string {
  const text = String(value || "");
  const folderMatch = text.match(/(?:Project folder|项目文件夹|Folder|文件夹)\s*[:：]\s*([^\n]+)/i);
  if (folderMatch?.[1]) {
    const parts = folderMatch[1].trim().split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] || folderMatch[1].trim();
  }
  const projectMatch = text.match(/(?:Project|项目)\s*[:：]\s*([^\n,，]+)/i);
  if (projectMatch?.[1]) return projectMatch[1].trim();
  return "";
}

function ensureProject(map: Map<string, ProjectSummary>, name: string): ProjectSummary {
  const clean = name.trim() || "General Workspace";
  const id = projectSlug(clean);
  const existing = map.get(id);
  if (existing) return existing;
  const created: ProjectSummary = {
    id,
    name: clean,
    description: "",
    folder: "",
    status: "active",
    updatedAtMs: 0,
    conversations: [],
    tasks: [],
    memories: [],
  };
  map.set(id, created);
  return created;
}

export function deriveProjects(input: {
  companyProfile?: ProductRow | null;
  userProfile?: ProductRow | null;
  conversations?: ProductRow[];
  workOrders?: ProductRow[];
  memories?: ProductRow[];
}): ProjectSummary[] {
  const map = new Map<string, ProjectSummary>();
  const declared = [
    ...rawList(input.companyProfile, "active_projects"),
    ...rawList(input.userProfile, "active_projects"),
  ];
  declared.forEach((project, index) => {
    const normalized = normalizeProjectValue(project, index);
    map.set(normalized.id, normalized);
  });

  for (const task of input.workOrders || []) {
    const name = extractProjectNameFromText(stringField(task, "mission_intent")) || "General Workspace";
    const project = ensureProject(map, name);
    project.tasks.push(task);
    project.updatedAtMs = Math.max(project.updatedAtMs, numberField(task, "updated_at_ms") || numberField(task, "created_at_ms"));
    if (!project.folder) project.folder = extractProjectNameFromText(stringField(task, "mission_intent"));
  }

  for (const conversation of input.conversations || []) {
    const name = extractProjectNameFromText(stringField(conversation, "title")) || "General Workspace";
    const project = ensureProject(map, name);
    project.conversations.push(conversation);
    project.updatedAtMs = Math.max(project.updatedAtMs, numberField(conversation, "updated_at_ms") || numberField(conversation, "created_at_ms"));
  }

  for (const memory of input.memories || []) {
    const name = extractProjectNameFromText(stringField(memory, "title")) || extractProjectNameFromText(stringField(memory, "content"));
    if (!name) continue;
    const project = ensureProject(map, name);
    project.memories.push(memory);
    project.updatedAtMs = Math.max(project.updatedAtMs, numberField(memory, "updated_at_ms") || numberField(memory, "created_at_ms"));
  }

  if (map.size === 0) ensureProject(map, "General Workspace");

  return [...map.values()]
    .map((project) => {
      const waiting = project.tasks.some((task) => stringField(task, "status") === "WaitingApproval");
      const done = project.tasks.length > 0 && project.tasks.every((task) => stringField(task, "status") === "Completed");
      const status: ProjectSummary["status"] = waiting ? "waiting" : done ? "done" : "active";
      return { ...project, status };
    })
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs || a.name.localeCompare(b.name));
}

export function taskStatusTone(status: string, track = ""): "green" | "yellow" | "red" {
  if (track === "red" || status === "Failed" || status === "Cancelled") return "red";
  if (track === "yellow" || status === "WaitingApproval" || status === "Running") return "yellow";
  return "green";
}
