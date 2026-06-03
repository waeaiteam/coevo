import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import Icon from "../components/Icon";
import {
  listCompanies,
  createCompany,
  deleteCompany,
  getActiveOpcId,
  setActiveOpcId,
  type Company,
} from "../api/companies";
import { t, useLanguage } from "../settings/i18n";

export default function MyCompany() {
  useLanguage();
  const navigate = useNavigate();
  const [companies, setCompanies] = useState<Company[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeId, setActiveId] = useState<string>(getActiveOpcId());
  const [showCreate, setShowCreate] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<string>("");
  const [deletingId, setDeletingId] = useState<string>("");

  async function load(): Promise<void> {
    setLoading(true);
    try {
      const rows = await listCompanies();
      setCompanies(Array.isArray(rows) ? rows : []);
    } catch {
      setCompanies([]);
    }
    setActiveId(getActiveOpcId());
    setLoading(false);
  }

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void listCompanies()
      .then((rows) => {
        if (!alive) return;
        setCompanies(Array.isArray(rows) ? rows : []);
        setActiveId(getActiveOpcId());
        setLoading(false);
      })
      .catch(() => {
        if (!alive) return;
        setCompanies([]);
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  function enterCompany(opcId: string): void {
    setActiveOpcId(opcId);
    setActiveId(opcId);
    navigate(`/companies/${encodeURIComponent(opcId)}`);
  }

  function handleCreated(company: Company): void {
    setShowCreate(false);
    setActiveOpcId(company.opc_id);
    setActiveId(company.opc_id);
    navigate(`/companies/${encodeURIComponent(company.opc_id)}`);
  }

  async function confirmDelete(opcId: string): Promise<void> {
    if (deletingId) return;
    setDeletingId(opcId);
    try {
      await deleteCompany(opcId);
    } catch {
      /* ignore — reload reflects current state */
    }
    setPendingDelete("");
    setDeletingId("");
    await load();
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("companies.kicker")}</div>
          <h1 className="product-title">{t("companies.title")}</h1>
          <p className="product-subtitle">{t("companies.subtitle")}</p>
        </div>
        <div className="product-actions">
          <button type="button" className="primary-button product-action" onClick={() => setShowCreate(true)}>
            <Icon name="plus" /> {t("companies.new")}
          </button>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {!loading && companies.length === 0 && (
        <div className="empty-state">
          <div className="empty-state-icon">
            <Icon name="building" />
          </div>
          <p>{t("companies.empty")}</p>
        </div>
      )}

      {!loading && companies.length > 0 && (
        <section className="project-grid">
          {companies.map((company) => {
            const isActive = company.opc_id === activeId;
            const isPending = pendingDelete === company.opc_id;
            const isDeleting = deletingId === company.opc_id;
            return (
              <div key={company.opc_id} className="project-card">
                <div className="project-card-head">
                  <h2>
                    <Icon name="building" /> {company.name}
                  </h2>
                  {isActive && <span className="product-pill green">{t("companies.active_badge")}</span>}
                </div>
                <p>{company.mission || t("company.mission_empty")}</p>
                <div className="project-card-meta">
                  <span>
                    {company.employee_count} {t("companies.employees_unit")}
                  </span>
                  <span className="mono-chip">{company.opc_id}</span>
                </div>
                <div className="project-card-footer">
                  {isPending ? (
                    <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
                      <span style={{ fontSize: 12, color: "var(--text-muted)" }}>{t("companies.delete_confirm")}</span>
                      <div style={{ display: "flex", gap: 8 }}>
                        <button
                          type="button"
                          className="primary-button"
                          style={{ background: "var(--red)", borderColor: "var(--red)" }}
                          disabled={isDeleting}
                          onClick={() => void confirmDelete(company.opc_id)}
                        >
                          {isDeleting ? (
                            <>
                              <Icon name="spinner" className="icon-spin" /> {t("companies.deleting")}
                            </>
                          ) : (
                            <>
                              <Icon name="x" /> {t("companies.delete_yes")}
                            </>
                          )}
                        </button>
                        <button
                          type="button"
                          className="product-link-button"
                          disabled={isDeleting}
                          onClick={() => setPendingDelete("")}
                        >
                          {t("companies.cancel")}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <button type="button" className="primary-button" onClick={() => enterCompany(company.opc_id)}>
                        {t("companies.enter")} <Icon name="chevron-right" />
                      </button>
                      <button
                        type="button"
                        className="product-link-button"
                        onClick={() => setPendingDelete(company.opc_id)}
                      >
                        <Icon name="x" /> {t("companies.delete")}
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </section>
      )}

      {showCreate && <CreateCompanyModal onClose={() => setShowCreate(false)} onCreated={handleCreated} />}
    </div>
  );
}

function CreateCompanyModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (company: Company) => void;
}) {
  useLanguage();
  const [name, setName] = useState("");
  const [mission, setMission] = useState("");
  const [creating, setCreating] = useState(false);

  async function submit(): Promise<void> {
    if (!name.trim() || creating) return;
    setCreating(true);
    try {
      const company = await createCompany({ name: name.trim(), mission: mission.trim() || undefined });
      onCreated(company);
    } catch {
      setCreating(false);
    }
  }

  return (
    <div className="command-overlay" onMouseDown={onClose}>
      <div
        className="command-panel"
        style={{ width: "min(520px, calc(100vw - 32px))", padding: 20 }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <h3 className="text-lg font-bold mb-3">{t("companies.new")}</h3>
        <div className="space-y-3">
          <label className="block">
            <span className="metric-label">{t("companies.name_label")}</span>
            <input
              className="select-control w-full mt-1"
              value={name}
              placeholder={t("companies.name_placeholder")}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label className="block">
            <span className="metric-label">{t("companies.mission_label")}</span>
            <textarea
              className="composer-textarea w-full mt-1"
              style={{ minHeight: 80, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
              value={mission}
              placeholder={t("companies.mission_placeholder")}
              onChange={(event) => setMission(event.target.value)}
            />
          </label>
        </div>
        <div className="flex items-center gap-2 mt-4">
          <button type="button" className="primary-button" disabled={creating || !name.trim()} onClick={() => void submit()}>
            {creating ? (
              <>
                <Icon name="spinner" className="icon-spin" /> {t("companies.creating")}
              </>
            ) : (
              <>
                <Icon name="plus" /> {t("companies.create")}
              </>
            )}
          </button>
          <button type="button" className="product-link-button" onClick={onClose}>
            {t("companies.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}
