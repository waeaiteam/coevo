import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { getApiBase, listConversations, listEmployees, listMemory, listWorkOrders } from "../api/client";
import { getLocalIdentity, getTenantId } from "../settings/identity";
import { t, useLanguage } from "../settings/i18n";

function InfoCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className="truncate text-sm font-semibold">{value}</div>
    </div>
  );
}

function TrackCard({ title, desc, tone }: { title: string; desc: string; tone: "green" | "yellow" | "red" }) {
  const color = tone === "green" ? "var(--green)" : tone === "yellow" ? "var(--yellow)" : "var(--red)";
  const bg = tone === "green" ? "var(--green-dim)" : tone === "yellow" ? "var(--yellow-dim)" : "var(--red-dim)";
  return (
    <div className="rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
      <div className="mb-2 inline-flex rounded px-2 py-0.5 text-[10px] font-semibold uppercase" style={{ background: bg, color }}>
        {title}
      </div>
      <p className="text-xs leading-5" style={{ color: "var(--text-secondary)" }}>{desc}</p>
    </div>
  );
}

export default function OpcOverview() {
  useLanguage();
  const identity = getLocalIdentity();
  const tenant = getTenantId();
  const [employees, setEmployees] = useState<Record<string, unknown>[]>([]);
  const [memories, setMemories] = useState<Record<string, unknown>[]>([]);
  const [workOrders, setWorkOrders] = useState<Record<string, unknown>[]>([]);
  const [conversations, setConversations] = useState<Record<string, unknown>[]>([]);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      listEmployees().catch(() => []),
      listMemory({ scope: "Company" }).catch(() => []),
      listWorkOrders().catch(() => []),
      listConversations().catch(() => []),
    ]).then(([e, m, w, c]) => {
      if (!alive) return;
      const nextEmployees = Array.isArray(e) ? e : [];
      const nextMemories = Array.isArray(m) ? m : [];
      const nextWorkOrders = Array.isArray(w) ? w : [];
      const nextConversations = Array.isArray(c) ? c : [];
      if (!nextEmployees.length && !nextMemories.length && !nextWorkOrders.length && !nextConversations.length) return;
      setEmployees(nextEmployees);
      setMemories(nextMemories);
      setWorkOrders(nextWorkOrders);
      setConversations(nextConversations);
    });
    return () => {
      alive = false;
    };
  }, []);

  const activeEmployees = useMemo(
    () => employees.filter((row) => String(row.lifecycle_status || "").toLowerCase() === "active"),
    [employees]
  );
  const hasRedWorkOrder = useMemo(
    () => workOrders.some((row) => String(row.track || "").toLowerCase() === "red"),
    [workOrders]
  );

  return (
    <div className="space-y-5">
      <section className="card">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold">{t("opc.operating_room")}</h2>
            <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>
              {t("opc.operating_room_desc")}
            </p>
          </div>
          <div className="flex gap-2">
            <Link to="/employees" className="rounded-md border px-3 py-1.5 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>
              {t("opc.manage_employees")}
            </Link>
            <Link to="/work-orders" className="rounded-md border px-3 py-1.5 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>
              {t("opc.open_task_center")}
            </Link>
          </div>
        </div>
        <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <InfoCell label={t("opc.metric_people")} value={`${activeEmployees.length} ${t("opc.metric_active_employees")}`} />
          <InfoCell label={t("opc.metric_memory")} value={`${memories.length} ${t("opc.metric_company_memories")}`} />
          <InfoCell label={t("opc.metric_work")} value={`${workOrders.length} ${t("opc.metric_work_orders")}`} />
          <InfoCell
            label={t("opc.metric_conversations")}
            value={`${conversations.length} ${conversations.length === 1 ? t("opc.metric_conversation") : t("opc.metric_conversations_unit")}`}
          />
        </div>
        <div className="mt-4 grid gap-3 lg:grid-cols-4">
          <div className="rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
            <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>{t("opc.ai_employees")}</div>
            <div className="space-y-1.5 text-xs">
              {activeEmployees.slice(0, 6).map((row, index) => (
                <div key={String(row.agent_id || `agent-${index}`)}>{String(row.display_name || row.agent_id || "Agent")}</div>
              ))}
              {!activeEmployees.length && <div style={{ color: "var(--text-muted)" }}>{t("opc.no_active_employees")}</div>}
            </div>
          </div>
          <div className="rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
            <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>{t("opc.company_memory")}</div>
            <div className="space-y-1.5 text-xs">
              {memories.slice(0, 6).map((row, index) => (
                <div key={String(row.memory_id || `memory-${index}`)}>{String(row.title || row.memory_id || "Memory")}</div>
              ))}
              {!memories.length && <div style={{ color: "var(--text-muted)" }}>{t("opc.no_company_memories")}</div>}
            </div>
          </div>
          <div className="rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
            <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>{t("workorders.title")}</div>
            <div className="space-y-2 text-xs">
              {workOrders.slice(0, 6).map((row, index) => (
                <div key={String(row.work_order_id || `work-order-${index}`)} className="rounded border px-2 py-1" style={{ borderColor: "var(--border-subtle)" }}>
                  <div>{String(row.mission_intent || row.work_order_id || "Work order")}</div>
                  <div className="font-semibold" style={{ color: "var(--text-secondary)" }}>{String(row.status || "")}</div>
                  <div style={{ color: "var(--text-muted)" }}>{t("opc.track_label")}: {String(row.track || "")}</div>
                </div>
              ))}
              {!workOrders.length && <div style={{ color: "var(--text-muted)" }}>{t("opc.no_work_orders")}</div>}
            </div>
            {hasRedWorkOrder && <div className="mt-2 text-xs font-semibold" style={{ color: "var(--red)" }}>{t("opc.red_blocked_short")}</div>}
          </div>
          <div className="rounded-md border bg-white p-3" style={{ borderColor: "var(--border-subtle)" }}>
            <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>{t("opc.recent_conversations")}</div>
            <div className="space-y-1.5 text-xs">
              {conversations.slice(0, 6).map((row, index) => (
                <div key={String(row.conversation_id || `conversation-${index}`)}>{String(row.title || row.conversation_id || "Conversation")}</div>
              ))}
              {!conversations.length && <div style={{ color: "var(--text-muted)" }}>{t("opc.no_conversations")}</div>}
            </div>
          </div>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[1.15fr_0.85fr]">
        <div className="card">
          <div className="mb-4 flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h1 className="truncate text-lg font-bold">{identity.opcName}</h1>
              <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>{t("opc.identity_desc")}</p>
            </div>
            <Link to="/settings/general" className="rounded-md border px-3 py-1.5 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>
              {t("opc.open")}
            </Link>
          </div>
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
            <InfoCell label={t("opc.owner")} value={identity.userName} />
            <InfoCell label={t("opc.opc_id")} value={identity.opcId || "local"} />
            <InfoCell label={t("opc.tenant")} value={tenant} />
            <InfoCell label={t("opc.workspace")} value="COEVO_HOME/workspace" />
          </div>
        </div>

        <div className="card">
          <h2 className="text-sm font-semibold">{t("opc.gateway")}</h2>
          <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>{t("opc.gateway_desc")}</p>
          <div className="mt-4 space-y-2 text-xs">
            <div className="flex justify-between gap-3">
              <span style={{ color: "var(--text-muted)" }}>{t("opc.api_base")}</span>
              <span className="truncate font-mono">{getApiBase()}</span>
            </div>
            <div className="flex justify-between gap-3">
              <span style={{ color: "var(--text-muted)" }}>{t("opc.logs")}</span>
              <span className="truncate font-mono">COEVO_HOME/logs</span>
            </div>
            <div className="flex justify-between gap-3">
              <span style={{ color: "var(--text-muted)" }}>{t("opc.runtime_path")}</span>
              <span className="truncate font-mono">runtime/server.port</span>
            </div>
          </div>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-3">
        <div className="card">
          <h2 className="text-sm font-semibold">{t("opc.agents")}</h2>
          <p className="mt-1 text-xs leading-5" style={{ color: "var(--text-muted)" }}>{t("opc.agents_desc")}</p>
          <div className="mt-4 grid grid-cols-3 gap-2 text-center text-xs">
            <Link to="/employees" className="rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>{t("opc.ai_employees")}</Link>
            <Link to="/skills" className="rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>{t("workorders.skills")}</Link>
            <Link to="/executors" className="rounded-md border p-2" style={{ borderColor: "var(--border-subtle)" }}>{t("workorders.executors")}</Link>
          </div>
        </div>
        <div className="card xl:col-span-2">
          <h2 className="text-sm font-semibold">{t("opc.governance")}</h2>
          <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>{t("opc.governance_desc")}</p>
          <div className="mt-4 grid gap-3 md:grid-cols-3">
            <TrackCard title={t("opc.green")} desc={t("opc.green_desc")} tone="green" />
            <TrackCard title={t("opc.yellow")} desc={t("opc.yellow_desc")} tone="yellow" />
            <TrackCard title={t("opc.red")} desc={t("opc.red_desc")} tone="red" />
          </div>
        </div>
      </section>

      <section className="card">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold">{t("opc.first_mission")}</h2>
            <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>{t("mission.governance_note")}</p>
          </div>
          <Link to="/" className="rounded-md px-3 py-1.5 text-xs text-white" style={{ background: "var(--accent)" }}>
            {t("nav.new_chat")}
          </Link>
        </div>
        <div className="grid gap-2 text-xs md:grid-cols-5">
          {[
            t("settings.model_provider"),
            t("mission.title"),
            t("opc.step.workorder"),
            t("opc.step.riskgate"),
            t("nav.audit"),
          ].map((step, i) => (
            <div key={`${step}-${i}`} className="rounded-md border p-3" style={{ borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
              <div className="mb-1 font-mono" style={{ color: "var(--accent)" }}>0{i + 1}</div>
              {step}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
