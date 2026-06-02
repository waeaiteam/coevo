import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";

export default function Resolution() {
  useLanguage();
  const fields = [
    "decision_id",
    "mcl_reference",
    "proposer_agent",
    "critic_objections",
    "rejected_alternatives",
    "responsibility_anchor",
    "risk_accepted",
  ];
  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("resolution.title")}</div>
          <h1 className="product-title">{t("resolution.title")}</h1>
        </div>
      </header>
      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="git-branch" /></div>
        <div>
          <h2>{t("resolution.title")}</h2>
          <p>{t("resolution.records")}</p>
        </div>
      </section>
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("resolution.records")}</h2>
        </div>
        <div className="chip-row">
          {fields.map((field) => (
            <span key={field} className="mono-chip">{field}</span>
          ))}
        </div>
      </div>
    </div>
  );
}
