import { t, useLanguage } from "../settings/i18n";

export default function RiskApprovalPanel() {
  useLanguage();

  return (
    <div className="card">
      <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
        {t("risk_panel.title")}
      </div>
      <div className="text-xs mb-3" style={{ color: "var(--text-muted)" }}>
        {t("risk_panel.desc")}
      </div>
      <div className="space-y-3">
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>{t("risk_panel.pending")}</span>
          <span className="font-mono" style={{ color: "var(--text-secondary)" }}>
            -
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>{t("risk_panel.extra_access")}</span>
          <span className="font-mono" style={{ color: "var(--text-secondary)" }}>
            {t("common.no")}
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>{t("risk_panel.latest_stop")}</span>
          <span className="font-mono truncate ml-2 max-w-32 text-right" style={{ color: "var(--red)" }}>
            {t("risk_panel.safety_stop")}
          </span>
        </div>
        <div className="flex justify-between text-xs pt-2 border-t" style={{ borderColor: "var(--border-subtle)" }}>
          <span style={{ color: "var(--text-muted)" }}>{t("risk_panel.rules")}</span>
          <span className="font-mono" style={{ color: "var(--accent)" }}>{t("risk_panel.local_rules")}</span>
        </div>
      </div>
    </div>
  );
}
