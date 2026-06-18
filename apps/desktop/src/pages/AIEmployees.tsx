import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { createEmployee, getAgentMemory, listEmployees, seedEmployees } from "../api/client";
import Icon from "../components/Icon";
import AgentWorkbenchPanel from "../components/AgentWorkbenchPanel";
import { loadSettingsSnapshot } from "../hooks/useSettings";
import { t, useLanguage } from "../settings/i18n";
import { presetFor } from "../settings/modelPresets";

const DEPTS = [
  { key: "founder_office", labelKey: "employees.department_founder_office" },
  { key: "product", labelKey: "employees.department_product" },
  { key: "engineering", labelKey: "employees.department_engineering" },
  { key: "research", labelKey: "employees.department_research" },
  { key: "governance", labelKey: "employees.department_governance" },
  { key: "sre", labelKey: "employees.department_sre" },
  { key: "growth", labelKey: "employees.department_growth" },
  { key: "finance", labelKey: "employees.department_finance" },
  { key: "legal", labelKey: "employees.department_legal" },
  { key: "design", labelKey: "employees.department_design" },
  { key: "content", labelKey: "employees.department_content" },
  { key: "custom", labelKey: "employees.department_custom" },
];

function asList(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String).filter(Boolean) : [];
}

function boolLabel(value: unknown, labelKey: string) {
  return `${t(labelKey)} ${value ? t("employees.allowed") : t("employees.blocked")}`;
}

function normalizeEnum(value: unknown) {
  return String(value || "")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[\s-]+/g, "_")
    .toLowerCase();
}

function formatEnum(value: unknown) {
  return String(value || "")
    .replace(/_/g, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function roleLabel(value: unknown) {
  const normalized = normalizeEnum(value);
  const labels: Record<string, string> = {
    founder_office: t("employees.department_founder_office"),
    product: t("employees.department_product"),
    engineering: t("employees.department_engineering"),
    research: t("employees.department_research"),
    governance: t("employees.department_governance"),
    sre: t("employees.department_sre"),
    growth: t("employees.department_growth"),
    finance: t("employees.department_finance"),
    legal: t("employees.department_legal"),
    design: t("employees.department_design"),
    content: t("employees.department_content"),
  };
  return labels[normalized] || formatEnum(value);
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className="mt-1 text-sm" style={{ color: "var(--text-secondary)" }}>{value || "-"}</div>
    </div>
  );
}

function TagList({ items }: { items: string[] }) {
  if (!items.length) return <span className="text-xs" style={{ color: "var(--text-muted)" }}>{t("common.none")}</span>;
  return (
    <div className="flex flex-wrap gap-1.5">
      {items.map((item) => (
        <span key={item} className="rounded border px-2 py-0.5 text-xs" style={{ borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
          {item}
        </span>
      ))}
    </div>
  );
}

export default function AIEmployees() {
  useLanguage();
  const [emps, setEmps] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [seeding, setSeeding] = useState(false);
  const [seedResult, setSeedResult] = useState<{ text: string; ok: boolean } | null>(null);
  const [selectedId, setSelectedId] = useState("");
  const [agentMemory, setAgentMemory] = useState<Record<string, unknown> | null>(null);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryError, setMemoryError] = useState("");
  const [showWorkbench, setShowWorkbench] = useState(false);
  const [showCreate, setShowCreate] = useState(false);

  async function load() {
    setLoading(true);
    try {
      const e = await listEmployees();
      const next = e || [];
      setEmps(next);
      setSelectedId((current) => current || String(next[0]?.agent_id || ""));
    } catch {
      setEmps([]);
    }
    setLoading(false);
  }

  useEffect(() => { load(); }, []);

  async function seed() {
    setSeeding(true);
    setSeedResult(null);
    try {
      const r = await seedEmployees() as Record<string, unknown>;
      setSeedResult({ text: t("employees.seed_result").replace("{inserted}", String(r.inserted ?? 0)).replace("{total}", String(r.total ?? 0)), ok: true });
      await load();
    } catch(e: unknown) {
      setSeedResult({ text: e instanceof Error ? e.message : String(e), ok: false });
    }
    setSeeding(false);
  }

  useEffect(() => {
    if (!selectedId) {
      setAgentMemory(null);
      return;
    }
    let alive = true;
    setMemoryLoading(true);
    setMemoryError("");
    void getAgentMemory(selectedId)
      .then((memory) => {
        if (alive) setAgentMemory(memory as Record<string, unknown>);
      })
      .catch((e: unknown) => {
        if (!alive) return;
        setAgentMemory(null);
        if (typeof e === "object" && e !== null && "status" in e && Number((e as { status?: number }).status) === 404) {
          setMemoryError("");
          return;
        }
        setMemoryError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (alive) setMemoryLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [selectedId]);

  const selected = useMemo(
    () => emps.find((e) => String(e.agent_id || "") === selectedId) || emps[0],
    [emps, selectedId]
  );

  const passport = (selected?.passport || {}) as Record<string, unknown>;
  const boundary = (selected?.permission_boundary || {}) as Record<string, unknown>;
  const activeCount = emps.filter((e) => normalizeEnum(e.lifecycle_status) === "active").length;

  return (
    <div className="space-y-5">
      <div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{t("employees.section")}</div>
          <h2 className="mt-1 text-xl font-bold">{t("employees.title")}</h2>
          <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>
            {t("employees.desc")}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {seedResult && <span className="text-xs" style={{ color: seedResult.ok ? "var(--green)" : "var(--red)" }}>{seedResult.text}</span>}
          <button onClick={() => setShowCreate(true)} className="product-link-button">
            <Icon name="plus" /> {t("workbench.new_employee")}
          </button>
          <button onClick={seed} disabled={seeding} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>
            {seeding ? t("employees.seeding") : emps.length === 0 ? t("employees.seed") : t("employees.reseed")}
          </button>
        </div>
      </div>

      <section className="grid gap-3 sm:grid-cols-3">
        <Field label={t("employees.active")} value={String(activeCount)} />
        <Field label={t("employees.departments")} value={String(new Set(emps.map((e) => String(e.department || ""))).size)} />
        <Field label={t("employees.recent_tasks")} value={String(emps.length > 0 ? emps.length : 0)} />
      </section>

      <div className="text-xs p-3 rounded" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>
        {t("employees.company_hub_note")}
      </div>

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("settings.loading")}</div>}
      {!loading && emps.length === 0 && <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("employees.empty")}</div>}

      <div className="grid gap-5 xl:grid-cols-[0.95fr_1.05fr]">
        <section className="space-y-4">
          {DEPTS.map((d) => {
            const deptEmps = emps.filter((e) => normalizeEnum(e.department) === d.key);
            if (deptEmps.length === 0) return null;
            return (
              <div key={d.key}>
                <div className="text-xs font-semibold mb-2 uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>{t(d.labelKey)}</div>
                <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-1">
                  {deptEmps.map((e: Record<string, unknown>, i: number) => {
                    const id = String(e.agent_id || i);
                    const selectedRow = id === String(selected?.agent_id || "");
                    const active = normalizeEnum(e.lifecycle_status) === "active";
                    return (
                      <button
                        key={id}
                        type="button"
                        onClick={() => setSelectedId(id)}
                        className="rounded-md border p-3 text-left transition-colors"
                        style={{
                          background: "var(--bg-card)",
                          borderColor: selectedRow ? "var(--accent)" : "var(--border-subtle)",
                          color: "var(--text-primary)",
                        }}
                      >
                        <div className="flex justify-between items-start gap-2 mb-1">
                          <span className="text-sm font-semibold">{e.display_name as string}</span>
                          <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: active ? "var(--green-dim)" : "var(--yellow-dim)", color: active ? "var(--green)" : "var(--yellow)" }}>{formatEnum(e.lifecycle_status)}</span>
                        </div>
                        <div className="text-xs space-y-0.5" style={{ color: "var(--text-muted)" }}>
                          <div>{t("employees.role")}: {roleLabel(e.role)}</div>
                          <div>{t("employees.risk_ceiling")}: {String(e.risk_ceiling)}</div>
                          <div>{t("employees.layers")}: {asList(e.allowed_cognitive_layers).join(", ")}</div>
                          <div className="font-mono text-xs" style={{ color: "var(--accent)" }}>{id}</div>
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </section>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <button
              className="product-link-button"
              style={{ borderColor: !showWorkbench ? "var(--accent)" : undefined, color: !showWorkbench ? "var(--accent)" : undefined }}
              onClick={() => setShowWorkbench(false)}
            >
              <Icon name="user" /> {t("workbench.tab_overview")}
            </button>
            <button
              className="product-link-button"
              style={{ borderColor: showWorkbench ? "var(--accent)" : undefined, color: showWorkbench ? "var(--accent)" : undefined }}
              onClick={() => setShowWorkbench(true)}
            >
              <Icon name="sliders" /> {t("workbench.tab_manage")}
            </button>
          </div>
          {showWorkbench && selected ? (
            <AgentWorkbenchPanel
              employee={selected}
              onChanged={load}
              onDeleted={() => { setSelectedId(""); setShowWorkbench(false); load(); }}
            />
          ) : (
            <EmployeeDetail employee={selected} passport={passport} boundary={boundary} memory={agentMemory} memoryLoading={memoryLoading} memoryError={memoryError} />
          )}
        </div>
      </div>

      {showCreate && (
        <CreateEmployeeModal
          onClose={() => setShowCreate(false)}
          onCreated={(id) => { setShowCreate(false); setSelectedId(id); setShowWorkbench(true); load(); }}
        />
      )}
    </div>
  );
}

function EmployeeDetail({
  employee,
  passport,
  boundary,
  memory,
  memoryLoading,
  memoryError,
}: {
  employee?: Record<string, unknown>;
  passport: Record<string, unknown>;
  boundary: Record<string, unknown>;
  memory: Record<string, unknown> | null;
  memoryLoading: boolean;
  memoryError: string;
}) {
  if (!employee) {
    return (
      <section className="rounded-md border p-4" style={{ background: "var(--bg-card)", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">{t("employees.passport")}</div>
        <p className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>{t("employees.select_hint")}</p>
      </section>
    );
  }

  return (
    <section className="rounded-md border" style={{ background: "var(--bg-card)", borderColor: "var(--border-subtle)" }}>
      <div className="border-b p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{t("employees.selected")}</div>
            <h3 className="mt-1 text-lg font-bold">{String(employee.display_name || employee.agent_id || "")}</h3>
            <div className="mt-1 font-mono text-xs" style={{ color: "var(--accent)" }}>{String(employee.agent_id || "")}</div>
          </div>
          <div className="flex items-center gap-2">
            <Link
              to={`/employees/${encodeURIComponent(String(employee.agent_id || ""))}`}
              className="product-link-button"
              style={{ borderColor: "var(--accent)", color: "var(--accent)" }}
            >
              <Icon name="building" /> {t("market.go_to_office")}
            </Link>
            <Link
              to={`/employees/${encodeURIComponent(String(employee.agent_id || ""))}/growth`}
              className="product-link-button"
            >
              <Icon name="badge-check" /> {t("growth.view")}
            </Link>
          </div>
        </div>
      </div>

      <div className="grid gap-5 p-4 lg:grid-cols-2">
        <div className="space-y-4">
          <div>
            <h4 className="text-sm font-semibold">{t("employees.passport")}</h4>
            <div className="mt-3 space-y-3">
              <Field label={t("employees.passport_id")} value={String(passport.passport_id || "")} />
              <Field label={t("employees.issued_by")} value={String(passport.issued_by || "")} />
              <Field label={t("employees.role")} value={roleLabel(employee.role)} />
            </div>
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">{t("employees.capabilities")}</div>
            <TagList items={asList(passport.capabilities)} />
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">{t("employees.restrictions")}</div>
            <TagList items={asList(passport.restrictions)} />
          </div>
        </div>

        <div className="space-y-4">
          <div>
            <h4 className="text-sm font-semibold">{t("employees.default_view")}</h4>
            <div className="mt-2 text-xs" style={{ color: "var(--text-secondary)" }}>
              {t("employees.default_scope")} {String(boundary.max_risk_score ?? employee.risk_ceiling ?? "-")}
            </div>
            <div className="mt-2 text-xs" style={{ color: "var(--text-secondary)" }}>
              {boolLabel(boundary.can_call_external_executor, "employees.external_executor")}
            </div>
            <div className="mt-2 text-xs" style={{ color: "var(--text-secondary)" }}>
              {boolLabel(boundary.can_propose_skill, "employees.skill_proposals")}
            </div>
          </div>
          <details className="rounded-md border p-3" style={{ borderColor: "var(--border-subtle)" }}>
            <summary className="cursor-pointer text-xs font-semibold">{t("employees.advanced_identity")}</summary>
            <div className="mt-3">
              <h4 className="text-sm font-semibold">{t("employees.permission_boundary")}</h4>
            <div className="mt-3 grid gap-2 text-xs" style={{ color: "var(--text-secondary)" }}>
              <div>{t("employees.max_risk")} {String(boundary.max_risk_score ?? employee.risk_ceiling ?? "-")}</div>
              <div>{boolLabel(boundary.can_access_network, "employees.network")}</div>
              <div>{boolLabel(boundary.can_access_filesystem, "employees.filesystem")}</div>
              <div>{boolLabel(boundary.can_call_external_executor, "employees.external_executor")}</div>
              <div>{boolLabel(boundary.can_propose_skill, "employees.skill_proposals")}</div>
            </div>
              <div className="mt-3">
                <div className="mb-2 text-xs font-semibold">{t("employees.cognitive_layers")}</div>
                <TagList items={asList(employee.allowed_cognitive_layers)} />
              </div>
              <div className="mt-3">
                <div className="mb-2 text-xs font-semibold">{t("employees.action_modes")}</div>
                <TagList items={asList(employee.allowed_action_modes)} />
              </div>
            </div>
          </details>
        </div>
      </div>

      <div className="border-t p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <h4 className="text-sm font-semibold">{t("employees.agent_memory")}</h4>
        {memoryLoading && <div className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>{t("employees.memory_loading")}</div>}
        {!memoryLoading && !memory && !memoryError && <div className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>{t("employees.memory_empty")}</div>}
        {memoryError && <div className="mt-2 text-xs" style={{ color: "var(--red)" }}>{t("employees.memory_unavailable")}: {memoryError}</div>}
        {memory && (
          <div className="mt-3 grid gap-4 md:grid-cols-2">
            <Field label={t("employees.working_preference")} value={String(memory.working_preferences || "")} />
            <Field label={t("employees.performance_notes")} value={String(memory.performance_notes || "")} />
            <MemoryList label={t("employees.learned_constraints")} items={asList(memory.learned_constraints)} />
            <MemoryList label={t("employees.recent_tasks")} items={asList(memory.recent_tasks)} />
            <MemoryList label={t("employees.successful_patterns")} items={asList(memory.successful_patterns)} />
            <MemoryList label={t("employees.recurring_failures")} items={asList(memory.recurring_failures)} />
          </div>
        )}
      </div>
    </section>
  );
}

function MemoryList({ label, items }: { label: string; items: string[] }) {
  return (
    <div>
      <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className="mt-2 space-y-1">
        {items.length ? items.map((item) => <div key={item} className="text-xs" style={{ color: "var(--text-secondary)" }}>{item}</div>) : <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("common.none")}</div>}
      </div>
    </div>
  );
}

const NEW_DEPTS = ["founder_office", "product", "engineering", "research", "governance", "sre", "growth", "finance", "legal", "design", "content", "custom"];

function CreateEmployeeModal({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  useLanguage();
  const settings = loadSettingsSnapshot();
  const providerPreset = presetFor(settings.model_provider.provider);
  const [name, setName] = useState("");
  const [department, setDepartment] = useState("custom");
  const [riskCeiling, setRiskCeiling] = useState(0.3);
  const [systemPrompt, setSystemPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function create() {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError("");
    const agentId = `agent-${name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")}-${Math.random().toString(36).slice(2, 6)}`;
    const now = Date.now();
    const employee = {
      agent_id: agentId,
      display_name: name.trim(),
      department,
      role: department,
      passport: {
        passport_id: `passport-${agentId}`,
        issued_by: "workbench",
        roles: [department],
        capabilities: ["analysis", "planning"],
        restrictions: ["no production write", "no financial transfer"],
        expires_at_ms: null,
      },
      model_profile: {
        provider: providerPreset.provider,
        base_url: settings.model_provider.base_url || providerPreset.baseUrl,
        api_key_ref: "coevo/model-provider",
        default_model: settings.model_provider.default_model || providerPreset.defaultModel,
        fast_model: settings.model_provider.fast_model || providerPreset.fastModel,
        reasoning_model: settings.model_provider.reasoning_model || providerPreset.reasoningModel,
        structured_output_model: settings.model_provider.structured_output_model || providerPreset.structuredModel,
        timeout_ms: settings.model_provider.request_timeout_ms,
        max_tokens: settings.model_provider.max_tokens,
        max_cost_per_task_usd: settings.model_provider.max_cost_per_task_usd,
      },
      tool_scopes: ["urn:coevo:tool:read"],
      memory_scope: "agent",
      permission_boundary: {
        max_risk_score: riskCeiling, can_write_fact: false, can_write_decision: false,
        can_access_network: false, can_access_filesystem: false, can_call_external_executor: false, can_propose_skill: true,
      },
      allowed_cognitive_layers: ["Hypothesis", "Suggestion"],
      allowed_action_modes: ["DRAFT_ONLY"],
      risk_ceiling: riskCeiling,
      reputation_vector: {
        agent_id: agentId, task_domain_competence: 0.5, uncertainty_honesty: 0.5,
        policy_compliance: 0.5, resource_efficiency: 0.5, task_count: 0,
        high_difficulty_avoidance_count: 0, last_updated_ms: now,
      },
      supervisor_agent_id: "agent-founder-01",
      lifecycle_status: "active",
      system_prompt: systemPrompt,
      created_at_ms: now,
      updated_at_ms: now,
    };
    try {
      await createEmployee(employee);
      onCreated(agentId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  return (
    <div className="command-overlay" onMouseDown={onClose}>
      <div className="command-panel" style={{ width: "min(560px, calc(100vw - 32px))", padding: 20 }} onMouseDown={(e) => e.stopPropagation()}>
        <h3 className="text-lg font-bold mb-3">{t("workbench.new_employee")}</h3>
        {error && <div className="product-pill red mb-3">{error}</div>}
        <div className="space-y-3">
          <label className="block">
            <span className="metric-label">{t("workbench.name")}</span>
            <input className="select-control w-full mt-1" value={name} onChange={(e) => setName(e.target.value)} placeholder={t("workbench.name_placeholder")} />
          </label>
          <label className="block">
            <span className="metric-label">{t("workbench.department")}</span>
            <select className="select-control w-full mt-1" value={department} onChange={(e) => setDepartment(e.target.value)}>
              {NEW_DEPTS.map((d) => <option key={d} value={d}>{d}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="metric-label">{t("workbench.risk_ceiling")}</span>
            <input type="number" min={0} max={1} step={0.1} className="select-control w-full mt-1" value={riskCeiling} onChange={(e) => setRiskCeiling(Number(e.target.value) || 0.3)} />
          </label>
          <label className="block">
            <span className="metric-label">{t("workbench.system_prompt")}</span>
            <textarea className="composer-textarea w-full mt-1" style={{ minHeight: 90, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
              value={systemPrompt} placeholder={t("workbench.system_prompt_placeholder")} onChange={(e) => setSystemPrompt(e.target.value)} />
          </label>
        </div>
        <div className="flex items-center gap-2 mt-4">
          <button className="primary-button" disabled={busy || !name.trim()} onClick={create}>
            {busy ? <Icon name="spinner" className="icon-spin" /> : <Icon name="plus" />} {t("workbench.create")}
          </button>
          <button className="product-link-button" onClick={onClose}>{t("workbench.cancel")}</button>
        </div>
      </div>
    </div>
  );
}
