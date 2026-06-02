import { useEffect, useState } from "react";
import { approveSkillProposal, listSkillProposals, listSkills, rejectSkillProposal, rollbackSkill, seedSkills, verifySkillProposal } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

export default function SkillsPage() {
  useLanguage();
  const [skills, setSkills] = useState<Record<string, unknown>[]>([]);
  const [proposals, setProposals] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");

  async function load() {
    setLoading(true);
    try {
      setSkills(await listSkills() || []);
      setProposals(await listSkillProposals() || []);
    } catch {
      setSkills([]);
      setProposals([]);
    }
    setLoading(false);
  }

  useEffect(() => {
    void load();
  }, []);

  async function action(fn: () => Promise<unknown>, label: string) {
    setResult("");
    try {
      const response = await fn();
      setResult(`${label}: ${JSON.stringify(response)}`);
      await load();
    } catch (e: unknown) {
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg font-semibold" style={{ color: "var(--accent)" }}>S</span>
          <h2 className="text-lg font-bold">{t("skills.title")}</h2>
        </div>
        <button onClick={() => action(() => seedSkills(), "Seed")} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("skills.seed")}</button>
      </div>
      <div className="text-xs p-3 rounded" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>{t("skills.desc")}</div>
      {result && <div className="card"><pre className="text-xs" style={{ color: "var(--text-secondary)" }}>{result}</pre></div>}
      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>Loading...</div>}

      <h3 className="text-sm font-semibold">{t("skills.packages")}</h3>
      <div className="grid gap-2 md:grid-cols-2">
        {skills.map((skill, index) => (
          <div key={String(skill.skill_id || index)} className="card">
            <div className="mb-1 flex justify-between">
              <span className="text-sm font-semibold">{skill.name as string}</span>
              <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: skill.status === "Active" ? "var(--green-dim)" : "var(--yellow-dim)", color: skill.status === "Active" ? "var(--green)" : "var(--yellow)" }}>{skill.status as string}</span>
            </div>
            <div className="text-xs space-y-0.5" style={{ color: "var(--text-muted)" }}>
              <div>v{skill.version as string} | {skill.department as string} | owner: {skill.owner_agent_id as string} | risk: {String(skill.risk_ceiling)}</div>
              <div className="mt-2 flex gap-2">
                <button onClick={() => action(() => rollbackSkill(skill.skill_id as string, skill.version as string), "Rollback")} className="px-2 py-1 text-xs rounded border" style={{ borderColor: "var(--red)", color: "var(--red)" }}>{t("skills.rollback")}</button>
              </div>
            </div>
          </div>
        ))}
      </div>

      <h3 className="text-sm font-semibold">{t("skills.proposals")}</h3>
      <div className="grid grid-cols-1 gap-2">
        {proposals.map((proposal, index) => (
          <div key={String(proposal.proposal_id || index)} className="card">
            <div className="mb-1 flex justify-between">
              <span className="text-sm font-semibold">{proposal.proposal_id as string}</span>
              <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: proposal.status === "Applied" ? "var(--green-dim)" : "var(--yellow-dim)", color: proposal.status === "Applied" ? "var(--green)" : "var(--yellow)" }}>{proposal.status as string}</span>
            </div>
            <div className="text-xs space-y-0.5" style={{ color: "var(--text-muted)" }}>
              <div>Target: {proposal.target_skill_id as string} | Type: {proposal.proposal_type as string} | Risk: {proposal.risk_assessment as string}</div>
              <div>Diagnosis: {proposal.diagnosis as string}</div>
              <div className="mt-2 flex gap-2">
                <button onClick={() => action(() => verifySkillProposal(proposal.proposal_id as string), "Verify")} className="px-2 py-1 text-xs rounded border" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("skills.verify")}</button>
                <button onClick={() => action(() => approveSkillProposal(proposal.proposal_id as string), "Approve")} className="px-2 py-1 text-xs rounded border" style={{ borderColor: "var(--green)", color: "var(--green)" }}>{t("skills.approve")}</button>
                <button onClick={() => action(() => rejectSkillProposal(proposal.proposal_id as string), "Reject")} className="px-2 py-1 text-xs rounded border" style={{ borderColor: "var(--red)", color: "var(--red)" }}>{t("skills.reject")}</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
