import { useEffect, useMemo, useState } from "react";
import { getAgentMemory, listEmployees, seedEmployees } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

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
  const [seedResult, setSeedResult] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [agentMemory, setAgentMemory] = useState<Record<string, unknown> | null>(null);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryError, setMemoryError] = useState("");

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
    setSeedResult("");
    try {
      const r = await seedEmployees() as Record<string, unknown>;
      setSeedResult(t("employees.seed_result").replace("{inserted}", String(r.inserted ?? 0)).replace("{total}", String(r.total ?? 0)));
      await load();
    } catch(e: unknown) {
      setSeedResult("Error: " + (e instanceof Error ? e.message : String(e)));
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
          {seedResult && <span className="text-xs" style={{ color: seedResult.startsWith("Error") ? "var(--red)" : "var(--green)" }}>{seedResult}</span>}
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

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>Loading...</div>}
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

        <EmployeeDetail employee={selected} passport={passport} boundary={boundary} memory={agentMemory} memoryLoading={memoryLoading} memoryError={memoryError} />
      </div>
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
        <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{t("employees.selected")}</div>
        <h3 className="mt-1 text-lg font-bold">{String(employee.display_name || employee.agent_id || "")}</h3>
        <div className="mt-1 font-mono text-xs" style={{ color: "var(--accent)" }}>{String(employee.agent_id || "")}</div>
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
