import { Link } from "react-router-dom";
import { getApiBase } from "../api/client";
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

  return (
    <div className="space-y-5">
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
