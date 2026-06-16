import { Link } from "react-router-dom";
import { t, useLanguage } from "../settings/i18n";

type ConsoleItem = {
  labelKey: string;
  descKey: string;
  to: string;
};

const groups: { titleKey: string; items: ConsoleItem[] }[] = [
  {
    titleKey: "adv.group.context",
    items: [
      { labelKey: "adv.founder_profile", descKey: "adv.founder_profile_desc", to: "/founder" },
      { labelKey: "adv.company_memory", descKey: "adv.company_memory_desc", to: "/memory" },
      { labelKey: "adv.contracts", descKey: "adv.contracts_desc", to: "/contracts" },
    ],
  },
  {
    titleKey: "adv.group.capabilities",
    items: [
      { labelKey: "adv.ai_employees", descKey: "adv.ai_employees_desc", to: "/employees" },
      { labelKey: "adv.skills", descKey: "adv.skills_desc", to: "/skills" },
      { labelKey: "adv.external_executors", descKey: "adv.external_executors_desc", to: "/executors" },
      { labelKey: "settings.mcp_servers", descKey: "settings.mcp_servers_desc", to: "/settings/mcp_servers" },
    ],
  },
  {
    titleKey: "adv.group.quality",
    items: [
      { labelKey: "adv.quality_check", descKey: "adv.quality_check_desc", to: "/evaluations" },
      { labelKey: "adv.usage_overview", descKey: "adv.usage_overview_desc", to: "/performance" },
      { labelKey: "adv.task_replay", descKey: "adv.task_replay_desc", to: "/traces" },
    ],
  },
  {
    titleKey: "adv.group.governance",
    items: [
      { labelKey: "adv.resolution", descKey: "adv.resolution_desc", to: "/resolution" },
      { labelKey: "adv.cognitive_customs", descKey: "adv.cognitive_customs_desc", to: "/customs" },
      { labelKey: "adv.policy_engine", descKey: "adv.policy_engine_desc", to: "/settings/policy_engine" },
      { labelKey: "adv.privacy", descKey: "adv.privacy_desc", to: "/settings/privacy" },
    ],
  },
  {
    titleKey: "adv.group.system",
    items: [
      { labelKey: "adv.model_provider", descKey: "adv.model_provider_desc", to: "/settings/model_provider" },
      { labelKey: "adv.language_appearance", descKey: "adv.language_appearance_desc", to: "/settings/appearance" },
      { labelKey: "adv.data_management", descKey: "adv.data_management_desc", to: "/settings/data_management" },
      { labelKey: "adv.developer_mode", descKey: "adv.developer_mode_desc", to: "/settings/developer" },
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
                    <span className="text-xs font-semibold">{t(item.labelKey)}</span>
                    <span className="text-[10px]" style={{ color: "var(--accent)" }}>{t("opc.open")}</span>
                  </div>
                  <p className="mt-1 line-clamp-2 text-[11px] leading-5" style={{ color: "var(--text-muted)" }}>
                    {t(item.descKey)}
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
