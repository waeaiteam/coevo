import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";

export default function Audit() {
  useLanguage();
  const events = [
    "contract.compiled",
    "plan.created",
    "fact.proposed",
    "fact.promoted",
    "risk.evaluated",
    "lease.granted",
    "adr.generated",
    "human.overridden",
    "contract.closed",
  ];
  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("audit.title")}</div>
          <h1 className="product-title">{t("audit.title")}</h1>
        </div>
      </header>
      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="clipboard" /></div>
        <div>
          <h2>{t("audit.title")}</h2>
          <p>{t("audit.desc")}</p>
        </div>
      </section>
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("audit.title")}</h2>
        </div>
        <div className="chip-row">
          {events.map((event) => (
            <span key={event} className="mono-chip">{event}</span>
          ))}
        </div>
      </div>
    </div>
  );
}
