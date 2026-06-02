import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";

export default function Plans() {
  useLanguage();
  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("plans.title")}</div>
          <h1 className="product-title">{t("plans.title")}</h1>
        </div>
      </header>
      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="calendar" /></div>
        <div>
          <h2>{t("plans.title")}</h2>
          <p>{t("plans.empty")}</p>
        </div>
      </section>
      <div className="product-panel">
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="calendar" /></div>
          <div>{t("plans.empty")}</div>
        </div>
      </div>
    </div>
  );
}
