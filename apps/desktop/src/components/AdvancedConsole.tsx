import { Link } from "react-router-dom";
import { t, useLanguage } from "../settings/i18n";

type ConsoleItem = {
  label: string;
  desc: string;
  to: string;
};

const groups: { titleKey: string; items: ConsoleItem[] }[] = [
  {
    titleKey: "adv.group.context",
    items: [
      { label: "Founder Profile", desc: "Founder identity, operating preferences, and profile context.", to: "/founder" },
      { label: "Company Memory", desc: "Scoped memory records with provenance and lifecycle controls.", to: "/memory" },
      { label: "Contracts", desc: "Mission contract anchors and compiled governance records.", to: "/contracts" },
      { label: "Plans", desc: "Execution plans and routing outputs.", to: "/plans" },
    ],
  },
  {
    titleKey: "adv.group.capabilities",
    items: [
      { label: "AI Employees", desc: "Passports, departments, capabilities, and risk ceilings.", to: "/employees" },
      { label: "Skills", desc: "Versioned capabilities with activation, rollback, and evolution.", to: "/skills" },
      { label: "External Executors", desc: "Governed worker adapters and dry-run contracts.", to: "/executors" },
    ],
  },
  {
    titleKey: "adv.group.governance",
    items: [
      { label: "Risk Gate", desc: "Green, Yellow, and Red threshold configuration.", to: "/risk" },
      { label: "Resolution", desc: "ADR-A conflict handling and escalation review.", to: "/resolution" },
      { label: "Cognitive Customs", desc: "Fact provenance, TTL, and promotion policy.", to: "/customs" },
      { label: "Policy Engine", desc: "Policy profile, simulation, and decision logging.", to: "/settings/policy_engine" },
      { label: "Privacy", desc: "Retention, prompt storage, PII redaction, and local paths.", to: "/settings/privacy" },
    ],
  },
  {
    titleKey: "adv.group.system",
    items: [
      { label: "Model Provider", desc: "Provider, credential-vault save path, and model discovery.", to: "/settings/model_provider" },
      { label: "Language & Appearance", desc: "Language, theme, density, and accessibility preferences.", to: "/settings/appearance" },
      { label: "Data Management", desc: "COEVO_HOME, logs, runtime files, and local data actions.", to: "/settings/data_management" },
      { label: "Developer Mode", desc: "API base, trace panels, raw JSON, and feature flags.", to: "/settings/developer" },
    ],
  },
];

export default function AdvancedConsole() {
  useLanguage();

  return (
    <section aria-label={t("opc.advanced")} className="card" style={{ padding: 0 }}>
      <div className="border-b px-4 py-3" style={{ borderColor: "var(--border-subtle)" }}>
        <h2 className="text-sm font-semibold">{t("opc.advanced")}</h2>
        <p className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>{t("opc.advanced_desc")}</p>
      </div>
      <div className="grid gap-0 md:grid-cols-2 xl:grid-cols-4">
        {groups.map((group) => (
          <div key={group.titleKey} className="border-r p-4 last:border-r-0" style={{ borderColor: "var(--border-subtle)" }}>
            <div className="mb-3 text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
              {t(group.titleKey)}
            </div>
            <div className="space-y-2">
              {group.items.map((item) => (
                <Link
                  key={item.to}
                  to={item.to}
                  className="block rounded-md border p-3 transition-colors hover:bg-[var(--bg-card-hover)]"
                  style={{ borderColor: "var(--border-subtle)", color: "var(--text-primary)" }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-semibold">{item.label}</span>
                    <span className="text-[10px]" style={{ color: "var(--accent)" }}>{t("opc.open")}</span>
                  </div>
                  <p className="mt-1 line-clamp-2 text-[11px] leading-5" style={{ color: "var(--text-muted)" }}>
                    {item.desc}
                  </p>
                </Link>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
