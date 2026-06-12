import { useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  createMemory,
  updateCompanyProfile,
  updateUserProfile,
} from "../api/client";
import { createCompany, setActiveOpcId } from "../api/companies";
import { createLocalOpc, type LocalIdentity } from "../settings/identity";
import { getLanguage, setLanguage, t, useLanguage } from "../settings/i18n";

type WizardStep = "identity" | "foundation" | "handoff";
type AlphaPosture = "conservative" | "balanced" | "exploratory";

function uuid() {
  if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  return `id-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export default function FirstRun({ onDone }: { onDone: () => void }) {
  useLanguage();
  const navigate = useNavigate();
  const [step, setStep] = useState<WizardStep>("identity");
  const [identity, setIdentity] = useState<LocalIdentity | null>(null);
  const [opcName, setOpcName] = useState("My OPC");
  const [ownerName, setOwnerName] = useState("Founder");
  const [language, setLocalLanguage] = useState<"en" | "zh">(() => getLanguage());
  const [companyMission, setCompanyMission] = useState("");
  const [companyDomain, setCompanyDomain] = useState("");
  const [operatingPrinciples, setOperatingPrinciples] = useState("");
  const [posture, setPosture] = useState<AlphaPosture>("balanced");
  const [teamCount, setTeamCount] = useState<number | null>(null);
  const [skillCount, setSkillCount] = useState<number | null>(null);
  const [foundationReady, setFoundationReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function continueToFoundation() {
    const nextIdentity = createLocalOpc({ opcName, userName: ownerName, language });
    setLanguage(language);
    setIdentity(nextIdentity);
    setStep("foundation");
    setError("");
    setTeamCount(0);
    setSkillCount(1);
    setFoundationReady(true);
  }

  async function createFoundation() {
    const current = identity || createLocalOpc({ opcName, userName: ownerName, language });
    const now = Date.now();
    const principles = operatingPrinciples
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    const finalPrinciples = principles.length > 0
      ? principles
      : [t("first_run.rule_red_blocked"), t("first_run.rule_audit_evidence")];
    const riskPreference = posture === "exploratory" ? "aggressive" : posture;
    const mission = companyMission.trim() || t("first_run.company_mission_default");
    const domain = companyDomain.trim() || t("first_run.company_domain_default");
    setBusy(true);
    setError("");
    try {
      const createdCompany = await createCompany({
        name: current.opcName,
        mission,
      });
      const canonicalOpcId = String(createdCompany?.opc_id || "").trim();
      if (!canonicalOpcId) {
        throw new Error("Company creation did not return an opc_id.");
      }
      setActiveOpcId(canonicalOpcId);
      await updateUserProfile({
        user_id: current.userId,
        display_name: current.userName,
        preferred_language: current.language,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "local",
        risk_preference: riskPreference,
        default_mission_mode: posture === "conservative" ? "read_only" : "auto",
        long_term_goals: [mission],
        business_domains: [domain],
        communication_style: "concise, audit-aware",
        approval_preferences: {
          auto_approve_below_risk: posture === "exploratory" ? 0.35 : posture === "balanced" ? 0.25 : 0.15,
          require_explicit_for_yellow: true,
          require_mfa_for_red: true,
          negative_consent_timeout_secs: 300,
        },
        data_boundaries: ["local-opc-workspace"],
        budget_limits: {
          max_cost_per_task_usd: 5,
          max_cost_per_day_usd: 25,
          max_agents_per_task: posture === "conservative" ? 3 : 5,
        },
        favorite_tools: [],
        active_projects: [],
        created_at_ms: now,
        updated_at_ms: now,
      });
      await updateCompanyProfile({
        opc_id: canonicalOpcId,
        founder_user_id: current.userId,
        name: current.opcName,
        mission,
        current_strategy: domain,
        operating_principles: finalPrinciples,
        active_projects: [],
        asset_indexes: [],
        policy_profile: `alpha-${posture}`,
        memory_policy: {
          fact_ttl_default_seconds: 3600,
          require_provenance_for_fact: true,
          auto_stale_days: 30,
        },
        default_departments: ["FounderOffice", "Research", "Product", "Governance"],
        created_at_ms: now,
        updated_at_ms: now,
      });
      await createMemory({
        memory_id: uuid(),
        scope: "company",
        owner_id: canonicalOpcId,
        title: "Operating Principles",
        content: [mission, domain, ...finalPrinciples].filter(Boolean).join("\n"),
        tags: ["company-foundation", posture],
        source: "first-run",
        provenance: `first-run:${canonicalOpcId}:company-foundation`,
        confidence: 0.9,
        ttl_seconds: 2592000,
        created_at_ms: now,
        updated_at_ms: now,
        access_policy: "opc-local",
        status: "active",
        cognitive_layer: "Suggestion",
        linked_contract_hash: null,
        linked_plan_hash: null,
        linked_adr_id: null,
      });
      setStep("handoff");
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function openModelProvider() {
    onDone();
    navigate("/settings/model_provider");
  }

  return (
    <div className="flex min-h-screen items-center justify-center px-6" style={{ background: "var(--bg-primary)", color: "var(--text-primary)" }}>
      <div className="grid w-full max-w-5xl gap-8 md:grid-cols-[1fr_360px]">
        <section className="flex flex-col justify-center">
          <div className="mb-5 flex items-center gap-3">
            <span className="grid h-9 w-9 place-items-center rounded-md text-sm font-bold text-white" style={{ background: "var(--cv-text)" }}>c</span>
            <span className="text-sm font-semibold">{t("app.tagline")}</span>
          </div>
          <h1 className="text-3xl font-bold tracking-tight md:text-4xl">{t("first_run.title")}</h1>
          <p className="mt-3 max-w-xl text-sm leading-6" style={{ color: "var(--text-secondary)" }}>
            {t("first_run.subtitle")}
          </p>
          <div className="mt-8 grid max-w-2xl gap-3 sm:grid-cols-3">
            {[t("first_run.step_identity"), t("first_run.step_foundation"), t("first_run.step_provider")].map((label, i) => (
              <div key={label} className="rounded-lg border p-3 text-xs" style={{ background: "var(--bg-card)", borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
                <div className="mb-1 font-mono" style={{ color: "var(--text-muted)" }}>0{i + 1}</div>
                {label}
              </div>
            ))}
          </div>
        </section>

        <section className="rounded-xl border p-5 shadow-sm" style={{ background: "var(--bg-card)", borderColor: "var(--border-subtle)" }}>
          {step === "identity" && (
            <div className="space-y-4">
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-opc-name">{t("first_run.opc_name")}</label>
                <input id="first-run-opc-name" className="input w-full" value={opcName} onChange={(e) => setOpcName(e.target.value)} />
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-owner-name">{t("first_run.owner_name")}</label>
                <input id="first-run-owner-name" className="input w-full" value={ownerName} onChange={(e) => setOwnerName(e.target.value)} />
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-language">{t("first_run.language")}</label>
                <select
                  id="first-run-language"
                  className="input w-full"
                  value={language}
                  onChange={(e) => {
                    const next = e.target.value === "zh" ? "zh" : "en";
                    setLocalLanguage(next);
                    setLanguage(next);
                  }}
                >
                  <option value="en">English</option>
                  <option value="zh">中文</option>
                </select>
              </div>
              <button onClick={continueToFoundation} className="w-full rounded-md py-3 text-sm font-semibold" style={{ background: "var(--cv-text)", color: "var(--cv-bg)" }}>
                {t("first_run.continue_foundation")}
              </button>
            </div>
          )}

          {step === "foundation" && (
            <div className="space-y-4">
              <div>
                <h2 className="text-base font-bold">{t("first_run.foundation_title")}</h2>
                <p className="mt-1 text-xs leading-5" style={{ color: "var(--text-secondary)" }}>{t("first_run.foundation_desc")}</p>
              </div>
              <div className="grid grid-cols-2 gap-2 text-xs">
                <div className="rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>
                  <div style={{ color: "var(--text-muted)" }}>{t("first_run.team_preview")}</div>
                  <div className="font-semibold">{`${teamCount ?? 0} ${t("first_run.agents")}`}</div>
                </div>
                <div className="rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>
                  <div style={{ color: "var(--text-muted)" }}>{t("first_run.skills_preview")}</div>
                  <div className="font-semibold">{`${skillCount ?? 0} ${t("first_run.skills")}`}</div>
                </div>
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-company-mission">{t("first_run.company_mission")}</label>
                <textarea id="first-run-company-mission" className="input w-full resize-none" rows={3} value={companyMission} onChange={(e) => setCompanyMission(e.target.value)} />
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-company-domain">{t("first_run.company_domain")}</label>
                <input id="first-run-company-domain" className="input w-full" value={companyDomain} onChange={(e) => setCompanyDomain(e.target.value)} />
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-operating-principles">{t("first_run.operating_principles")}</label>
                <textarea
                  id="first-run-operating-principles"
                  className="input w-full resize-none"
                  rows={4}
                  value={operatingPrinciples}
                  placeholder={t("first_run.operating_principles_placeholder")}
                  onChange={(e) => setOperatingPrinciples(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-posture">{t("first_run.permission_posture")}</label>
                <select id="first-run-posture" className="input w-full" value={posture} onChange={(e) => setPosture(e.target.value as AlphaPosture)}>
                  <option value="conservative">{t("first_run.posture_conservative")}</option>
                  <option value="balanced">{t("first_run.posture_balanced")}</option>
                  <option value="exploratory">{t("first_run.posture_exploratory")}</option>
                </select>
              </div>
              <div className="rounded-md border p-3 text-xs leading-5" style={{ borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
                {t("first_run.rules_context_note")}
              </div>
              {error && <div className="text-xs" style={{ color: "var(--red)" }}>{error}</div>}
              <button disabled={busy || !foundationReady} onClick={createFoundation} className="w-full rounded-md py-3 text-sm font-semibold disabled:opacity-50" style={{ background: "var(--cv-text)", color: "var(--cv-bg)" }}>
                {busy ? t("mission.creating") : t("first_run.create_and_continue")}
              </button>
            </div>
          )}

          {step === "handoff" && (
            <div className="space-y-4">
              <div>
                <h2 className="text-base font-bold">{t("first_run.model_handoff_title")}</h2>
                <p className="mt-1 text-xs leading-5" style={{ color: "var(--text-secondary)" }}>{t("first_run.model_handoff_desc")}</p>
              </div>
              <button onClick={openModelProvider} className="w-full rounded-md py-3 text-sm font-semibold" style={{ background: "var(--cv-text)", color: "var(--cv-bg)" }}>
                {t("first_run.open_model_provider")}
              </button>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
