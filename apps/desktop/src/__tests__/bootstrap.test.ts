import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  listEmployees: vi.fn(),
  listSkills: vi.fn(),
}));

async function loadBootstrap() {
  vi.resetModules();
  vi.doMock("../api/client", () => ({
    listEmployees: api.listEmployees,
    listSkills: api.listSkills,
  }));
  return import("../api/bootstrap");
}

describe("ensureWorkspaceDefaults", () => {
  beforeEach(() => {
    api.listEmployees.mockReset();
    api.listSkills.mockReset();
    api.listSkills.mockResolvedValue([
      { skill_id: "skill-mission-draft", status: "Active" },
    ]);
  });

  it("selects a Yellow-qualified employee instead of falling back to a low-risk active employee", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
      { agent_id: "agent-risk-01", lifecycle_status: "Active", risk_ceiling: 0.6 },
    ]);

    const { ensureWorkspaceDefaults } = await loadBootstrap();
    const result = await ensureWorkspaceDefaults("yellow");

    expect(result.selectedAgentIds).toEqual(["agent-risk-01"]);
  });

  it("allows Red WorkOrder creation bootstrap with an active employee for audit-only Alpha behavior", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);

    const { ensureWorkspaceDefaults } = await loadBootstrap();
    const result = await ensureWorkspaceDefaults("red");

    expect(result.selectedAgentIds).toEqual(["agent-founder-01"]);
    expect(result.requiredSkillIds).toEqual(["skill-mission-draft"]);
  });

  it("fails clearly when no active employee can handle the requested track", async () => {
    api.listEmployees.mockResolvedValue([]);

    const { ensureWorkspaceDefaults } = await loadBootstrap();

    await expect(ensureWorkspaceDefaults("green")).rejects.toThrow(
      "No active AI Employee can handle green track. Create an employee in AI Employees before starting tasks.",
    );
  });

  it("fails clearly when the company starter skill is missing", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);
    api.listSkills.mockResolvedValue([]);

    const { ensureWorkspaceDefaults } = await loadBootstrap();

    await expect(ensureWorkspaceDefaults("green")).rejects.toThrow(
      "Company skill template skill-mission-draft is missing. Recreate the company or repair company skills before starting tasks.",
    );
  });
});
