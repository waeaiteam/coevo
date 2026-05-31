import { useEffect, useMemo, useState } from "react";
import { getAgentMemory, listEmployees, seedEmployees } from "../api/client";

const DEPTS = [
  { key: "founder_office", label: "Founder Office" },
  { key: "product", label: "Product" },
  { key: "engineering", label: "Engineering" },
  { key: "research", label: "Research" },
  { key: "governance", label: "Governance" },
  { key: "sre", label: "SRE" },
  { key: "growth", label: "Growth" },
  { key: "finance", label: "Finance" },
  { key: "legal", label: "Legal" },
  { key: "design", label: "Design" },
  { key: "content", label: "Content" },
  { key: "custom", label: "Custom" },
];

function asList(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String).filter(Boolean) : [];
}

function boolLabel(value: unknown, label: string) {
  return `${label} ${value ? "allowed" : "blocked"}`;
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

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className="mt-1 text-sm" style={{ color: "var(--text-secondary)" }}>{value || "-"}</div>
    </div>
  );
}

function TagList({ items }: { items: string[] }) {
  if (!items.length) return <span className="text-xs" style={{ color: "var(--text-muted)" }}>None</span>;
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
      setSeedResult(`Inserted: ${r.inserted}, Total: ${r.total}`);
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
          <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>Company team</div>
          <h2 className="mt-1 text-xl font-bold">AI Employees</h2>
          <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>
            Governed workers with passports, memory, risk ceilings, and tool boundaries.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {seedResult && <span className="text-xs" style={{ color: seedResult.startsWith("Error") ? "var(--red)" : "var(--green)" }}>{seedResult}</span>}
          <button onClick={seed} disabled={seeding} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>
            {seeding ? "Seeding..." : emps.length === 0 ? "Seed 10 AI Employees" : "Re-seed"}
          </button>
        </div>
      </div>

      <section className="grid gap-3 sm:grid-cols-3">
        <Field label="Active employees" value={String(activeCount)} />
        <Field label="Departments" value={String(new Set(emps.map((e) => String(e.department || ""))).size)} />
        <Field label="Highest risk ceiling" value={String(Math.max(0, ...emps.map((e) => Number(e.risk_ceiling || 0))))} />
      </section>

      <div className="text-xs p-3 rounded" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>
        AI Employees are governed workers. They can think, remember, and propose actions only inside their Product Harness authorization envelope.
      </div>

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>Loading...</div>}
      {!loading && emps.length === 0 && <div className="text-xs" style={{ color: "var(--text-muted)" }}>No AI Employees. Click "Seed 10 AI Employees" to initialize.</div>}

      <div className="grid gap-5 xl:grid-cols-[0.95fr_1.05fr]">
        <section className="space-y-4">
          {DEPTS.map((d) => {
            const deptEmps = emps.filter((e) => normalizeEnum(e.department) === d.key);
            if (deptEmps.length === 0) return null;
            return (
              <div key={d.key}>
                <div className="text-xs font-semibold mb-2 uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>{d.label}</div>
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
                        className="rounded-md border bg-white p-3 text-left transition-colors"
                        style={{
                          borderColor: selectedRow ? "var(--accent)" : "var(--border-subtle)",
                          color: "var(--text-primary)",
                        }}
                      >
                        <div className="flex justify-between items-start gap-2 mb-1">
                          <span className="text-sm font-semibold">{e.display_name as string}</span>
                          <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: active ? "var(--green-dim)" : "var(--yellow-dim)", color: active ? "var(--green)" : "var(--yellow)" }}>{formatEnum(e.lifecycle_status)}</span>
                        </div>
                        <div className="text-xs space-y-0.5" style={{ color: "var(--text-muted)" }}>
                          <div>Role: {e.role as string}</div>
                          <div>Risk ceiling: {String(e.risk_ceiling)}</div>
                          <div>Layers: {asList(e.allowed_cognitive_layers).join(", ")}</div>
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
      <section className="rounded-md border bg-white p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">AI Employee Passport</div>
        <p className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>Select an employee to inspect passport, memory, and permissions.</p>
      </section>
    );
  }

  return (
    <section className="rounded-md border bg-white" style={{ borderColor: "var(--border-subtle)" }}>
      <div className="border-b p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>Selected employee</div>
        <h3 className="mt-1 text-lg font-bold">{String(employee.display_name || employee.agent_id || "")}</h3>
        <div className="mt-1 font-mono text-xs" style={{ color: "var(--accent)" }}>{String(employee.agent_id || "")}</div>
      </div>

      <div className="grid gap-5 p-4 lg:grid-cols-2">
        <div className="space-y-4">
          <div>
            <h4 className="text-sm font-semibold">AI Employee Passport</h4>
            <div className="mt-3 space-y-3">
              <Field label="Passport ID" value={String(passport.passport_id || "")} />
              <Field label="Issued by" value={String(passport.issued_by || "")} />
              <Field label="Role" value={String(employee.role || "")} />
            </div>
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">Capabilities</div>
            <TagList items={asList(passport.capabilities)} />
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">Restrictions</div>
            <TagList items={asList(passport.restrictions)} />
          </div>
        </div>

        <div className="space-y-4">
          <div>
            <h4 className="text-sm font-semibold">Permission Boundary</h4>
            <div className="mt-3 grid gap-2 text-xs" style={{ color: "var(--text-secondary)" }}>
              <div>Max risk {String(boundary.max_risk_score ?? employee.risk_ceiling ?? "-")}</div>
              <div>{boolLabel(boundary.can_access_network, "Network")}</div>
              <div>{boolLabel(boundary.can_access_filesystem, "Filesystem")}</div>
              <div>{boolLabel(boundary.can_call_external_executor, "External executor")}</div>
              <div>{boolLabel(boundary.can_propose_skill, "Skill proposals")}</div>
            </div>
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">Cognitive layers</div>
            <TagList items={asList(employee.allowed_cognitive_layers)} />
          </div>
          <div>
            <div className="mb-2 text-xs font-semibold">Action modes</div>
            <TagList items={asList(employee.allowed_action_modes)} />
          </div>
        </div>
      </div>

      <div className="border-t p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <h4 className="text-sm font-semibold">Agent Memory</h4>
        {memoryLoading && <div className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>Loading agent memory...</div>}
        {memoryError && <div className="mt-2 text-xs" style={{ color: "var(--yellow)" }}>Memory unavailable: {memoryError}</div>}
        {memory && (
          <div className="mt-3 grid gap-4 md:grid-cols-2">
            <Field label="Working preference" value={String(memory.working_preferences || "")} />
            <Field label="Performance notes" value={String(memory.performance_notes || "")} />
            <MemoryList label="Learned constraints" items={asList(memory.learned_constraints)} />
            <MemoryList label="Recent tasks" items={asList(memory.recent_tasks)} />
            <MemoryList label="Successful patterns" items={asList(memory.successful_patterns)} />
            <MemoryList label="Recurring failures" items={asList(memory.recurring_failures)} />
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
        {items.length ? items.map((item) => <div key={item} className="text-xs" style={{ color: "var(--text-secondary)" }}>{item}</div>) : <div className="text-xs" style={{ color: "var(--text-muted)" }}>None</div>}
      </div>
    </div>
  );
}
