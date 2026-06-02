import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";

export default function RiskGate() {
  useLanguage();
  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("risk.title")}</div>
          <h1 className="product-title">{t("risk.title")}</h1>
        </div>
      </header>
      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="shield-check" /></div>
        <div>
          <h2>{t("risk.title")}</h2>
          <p>{t("risk.summary")}</p>
        </div>
      </section>
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("risk.summary")}</h2>
        </div>
        <div className="product-grid-3">
          <RiskLayer title={t("risk.layer_policy")} desc={t("risk.layer_policy_desc")} />
          <RiskLayer title={t("risk.layer_veto")} desc={t("risk.layer_veto_desc")} />
          <RiskLayer title={t("risk.layer_confidence")} desc={t("risk.layer_confidence_desc")} />
        </div>
      </div>
    </div>
  );
}

function RiskLayer({ title, desc }: { title: string; desc: string }) {
  return (
    <div
      className="rounded"
      style={{ background: "var(--surface-raised)", border: "1px solid var(--border-subtle)", padding: "12px" }}
    >
      <div className="mb-1 text-xs" style={{ color: "var(--text-muted)" }}>{title}</div>
      <div className="text-xs" style={{ color: "var(--text-secondary)" }}>{desc}</div>
    </div>
  );
}
