import AdvancedConsole from "../components/AdvancedConsole";
import Icon from "../components/Icon";
import OpcOverview from "../components/OpcOverview";
import RiskApprovalPanel from "../components/RiskApprovalPanel";
import { t, useLanguage } from "../settings/i18n";

export default function Dashboard() {
  useLanguage();

  return (
    <div className="product-page">
      <div className="product-header">
        <div className="feature-hero">
          <div className="feature-hero-icon">
            <Icon name="gauge" />
          </div>
          <div>
            <div className="product-kicker">{t("dashboard.kicker")}</div>
            <h2>{t("opc.identity")}</h2>
          </div>
        </div>
        <RiskApprovalPanel />
      </div>
      <OpcOverview />
      <AdvancedConsole />
    </div>
  );
}
