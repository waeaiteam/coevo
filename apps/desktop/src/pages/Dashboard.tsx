import AdvancedConsole from "../components/AdvancedConsole";
import OpcOverview from "../components/OpcOverview";
import RiskApprovalPanel from "../components/RiskApprovalPanel";
import { t, useLanguage } from "../settings/i18n";

export default function Dashboard() {
  useLanguage();

  return (
    <div className="mx-auto max-w-7xl space-y-5">
      <div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
            {t("nav.opc")}
          </div>
          <h1 className="mt-1 text-xl font-bold">{t("opc.identity")}</h1>
        </div>
        <RiskApprovalPanel />
      </div>
      <OpcOverview />
      <AdvancedConsole />
    </div>
  );
}
