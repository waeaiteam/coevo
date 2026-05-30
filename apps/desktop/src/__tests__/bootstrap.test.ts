import { beforeEach, describe, expect, it, vi } from "vitest";
import { ensureWorkspaceDefaults } from "../api/bootstrap";

const api = vi.hoisted(() => ({
  listEmployees: vi.fn(),
  seedEmployees: vi.fn(),
  listSkills: vi.fn(),
  seedSkills: vi.fn(),
}));

vi.mock("../api/client", () => ({
  listEmployees: api.listEmployees,
  seedEmployees: api.seedEmployees,
  listSkills: api.listSkills,
  seedSkills: api.seedSkills,
}));

describe("ensureWorkspaceDefaults", () => {
  beforeEach(() => {
    api.listEmployees.mockReset();
    api.seedEmployees.mockReset();
    api.listSkills.mockReset();
    api.seedSkills.mockReset();
    api.listSkills.mockResolvedValue([
      { skill_id: "skill-mission-draft", status: "Active" },
    ]);
    api.seedEmployees.mockResolvedValue({ ok: true });
    api.seedSkills.mockResolvedValue({ ok: true });
  });

  it("selects a Yellow-qualified employee instead of falling back to a low-risk active employee", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
      { agent_id: "agent-risk-01", lifecycle_status: "Active", risk_ceiling: 0.6 },
    ]);

    const result = await ensureWorkspaceDefaults("yellow");

    expect(result.selectedAgentIds).toEqual(["agent-risk-01"]);
    expect(api.seedEmployees).not.toHaveBeenCalled();
  });

  it("fails Yellow bootstrap when no qualified employee exists after seeding", async () => {
    api.listEmployees
      .mockResolvedValueOnce([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }])
      .mockResolvedValueOnce([{ agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 }]);

    await expect(ensureWorkspaceDefaults("yellow")).rejects.toThrow(/No active AI Employee can handle yellow track/i);
    expect(api.seedEmployees).toHaveBeenCalledTimes(1);
  });

  it("allows Red WorkOrder creation bootstrap with an active employee for audit-only Alpha behavior", async () => {
    api.listEmployees.mockResolvedValue([
      { agent_id: "agent-founder-01", lifecycle_status: "Active", risk_ceiling: 0.3 },
    ]);

    const result = await ensureWorkspaceDefaults("red");

    expect(result.selectedAgentIds).toEqual(["agent-founder-01"]);
  });
});
