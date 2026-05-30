import { listEmployees, listSkills, seedEmployees, seedSkills } from "./client";

export type WorkspaceBootstrap = {
  selectedAgentIds: string[];
  requiredSkillIds: string[];
  seededEmployees: boolean;
  seededSkills: boolean;
};

const TRACK_RISK: Record<"green" | "yellow" | "red", number> = {
  green: 0.3,
  yellow: 0.6,
  red: 0.9,
};

function isActive(item: Record<string, unknown>) {
  return String(item.lifecycle_status || item.status || "").toLowerCase() === "active";
}

function riskCeiling(item: Record<string, unknown>) {
  const n = Number(item.risk_ceiling);
  return Number.isFinite(n) ? n : 0;
}

function chooseEmployee(employees: Record<string, unknown>[], minimumRisk: number) {
  const active = employees.filter(isActive);
  const qualified = active.filter((e) => riskCeiling(e) >= minimumRisk);
  return qualified.find((e) => e.agent_id === "agent-founder-01")
    || qualified.find((e) => e.agent_id === "agent-risk-01")
    || qualified.find((e) => e.agent_id === "agent-critic-01")
    || qualified[0];
}

function chooseFallbackEmployee(employees: Record<string, unknown>[]) {
  const active = employees.filter(isActive);
  return active.find((e) => e.agent_id === "agent-founder-01")
    || active.find((e) => e.agent_id === "agent-risk-01")
    || active.find((e) => e.agent_id === "agent-critic-01")
    || active[0];
}

function chooseEmployeeForTrack(employees: Record<string, unknown>[], track: "green" | "yellow" | "red") {
  if (track === "red") {
    return chooseEmployee(employees, TRACK_RISK.yellow) || chooseFallbackEmployee(employees);
  }
  return chooseEmployee(employees, TRACK_RISK[track]);
}

function hasActiveEmployee(employees: Record<string, unknown>[]) {
  return employees.some(isActive);
}

function hasQualifiedEmployee(employees: Record<string, unknown>[], track: "green" | "yellow" | "red") {
  if (track === "red") return hasActiveEmployee(employees);
  return Boolean(chooseEmployee(employees, TRACK_RISK[track]));
}

export async function ensureWorkspaceDefaults(track: "green" | "yellow" | "red" = "green"): Promise<WorkspaceBootstrap> {
  let employees = await listEmployees().catch(() => []);
  let seededEmployees = false;
  if (!hasQualifiedEmployee(employees, track)) {
    await seedEmployees();
    seededEmployees = true;
    employees = await listEmployees().catch(() => []);
  }

  let skills = await listSkills().catch(() => []);
  let seededSkills = false;
  const hasMissionDraftSkill = skills.some((s) => s.skill_id === "skill-mission-draft" && isActive(s));
  if (!hasMissionDraftSkill) {
    await seedSkills();
    seededSkills = true;
    skills = await listSkills().catch(() => []);
  }

  const employee = chooseEmployeeForTrack(employees, track);
  const agentId = String((employee || {}).agent_id || "");
  if (!agentId) {
    throw new Error(`No active AI Employee can handle ${track} track after workspace bootstrap.`);
  }

  const missionSkill = skills.find((s) => s.skill_id === "skill-mission-draft" && isActive(s));
  const skillId = String((missionSkill || skills.find(isActive) || {}).skill_id || "skill-mission-draft");

  return {
    selectedAgentIds: [agentId],
    requiredSkillIds: [skillId],
    seededEmployees,
    seededSkills,
  };
}
