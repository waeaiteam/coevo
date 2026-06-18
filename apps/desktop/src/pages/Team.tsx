import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import Icon, { type IconName } from "../components/Icon";
import AIEmployees from "./AIEmployees";
import { listEmployees } from "../api/client";
import { useAdvancedMode } from "../hooks/useAdvancedMode";
import { t, useLanguage } from "../settings/i18n";

// Unified team surface (P1.4). Default mode shows the org chart + a "hire" button so
// founders have one obvious place for their team. Advanced mode additionally reveals
// the full list/detail/workbench console (the former AI Employees page) below.

type Employee = Record<string, unknown>;

const normalize = (v: unknown): string =>
  String(v || "")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[\s-]+/g, "_")
    .toLowerCase();

const str = (employee: Employee, key: string): string => String(employee[key] ?? "");

const DEPARTMENT_LABEL_KEYS: Record<string, string> = {
  founder_office: "employees.department_founder_office",
  product: "employees.department_product",
  engineering: "employees.department_engineering",
  research: "employees.department_research",
  governance: "employees.department_governance",
  sre: "employees.department_sre",
  growth: "employees.department_growth",
  finance: "employees.department_finance",
  legal: "employees.department_legal",
  design: "employees.department_design",
  content: "employees.department_content",
  custom: "employees.department_custom",
};

const DEPARTMENT_ICONS: Record<string, IconName> = {
  founder_office: "user",
  product: "sparkles",
  engineering: "wrench",
  research: "brain",
  governance: "shield-check",
  growth: "gauge",
  finance: "badge-check",
  content: "file-text",
};

const titleCase = (raw: string): string =>
  raw.split("_").filter(Boolean).map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");

const departmentLabel = (dept: string): string => {
  const key = DEPARTMENT_LABEL_KEYS[dept];
  return key ? t(key) : titleCase(dept) || t("employees.department_custom");
};

const departmentIcon = (dept: string): IconName => DEPARTMENT_ICONS[dept] || "users";

export default function Team() {
  useLanguage();
  const advancedMode = useAdvancedMode();
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void listEmployees()
      .catch<Employee[]>(() => [])
      .then((rows) => {
        if (!alive) return;
        setEmployees(Array.isArray(rows) ? rows : []);
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const departments = useMemo(() => {
    const map = new Map<string, Employee[]>();
    for (const employee of employees) {
      // The secretary is shown as the dispatcher node, not inside a department column.
      if (str(employee, "agent_id") === "agent-secretary-01" || str(employee, "role") === "Secretary") {
        continue;
      }
      const dept = normalize(employee.department) || "custom";
      const bucket = map.get(dept) ?? [];
      bucket.push(employee);
      map.set(dept, bucket);
    }
    return [...map.entries()].sort((a, b) => {
      if (a[0] === "founder_office") return -1;
      if (b[0] === "founder_office") return 1;
      return departmentLabel(a[0]).localeCompare(departmentLabel(b[0]));
    });
  }, [employees]);

  const secretary = useMemo(
    () =>
      employees.find(
        (e) => str(e, "agent_id") === "agent-secretary-01" || str(e, "role") === "Secretary",
      ),
    [employees],
  );

  return (
    <div className="product-page">
      <header className="product-header">
        <div>
          <div className="product-kicker">{t("team.kicker")}</div>
          <h1 className="product-title">{t("team.title")}</h1>
          <p className="product-subtitle">{t("team.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link to="/market" className="primary-button product-action">
            <Icon name="plus" /> {t("team.hire")}
          </Link>
        </div>
      </header>

      <section className="product-panel">
        {loading ? (
          <div className="empty-state"><p>{t("settings.loading")}</p></div>
        ) : employees.length === 0 ? (
          <div className="empty-state">
            <div className="empty-state-icon"><Icon name="users" /></div>
            <p>{t("office.empty")}</p>
            <Link to="/market" className="product-link-button">{t("team.hire")}</Link>
          </div>
        ) : (
          <div className="office-canvas">
            <div className="office-founder-node">
              <div className="office-node-avatar"><Icon name="user" /></div>
              <div>
                <div className="office-node-title">{t("office.founder")}</div>
                <div className="office-node-sub">{t("office.you")}</div>
              </div>
            </div>
            <div className="office-trunk" />
            {secretary && (
              <>
                <Link
                  to={`/employees/${encodeURIComponent(str(secretary, "agent_id"))}`}
                  className="office-employee-node office-secretary-node"
                >
                  <div className="office-node-avatar"><Icon name="sparkles" /></div>
                  <div className="office-node-meta">
                    <div className="office-node-title">
                      {str(secretary, "display_name") || t("team.secretary")}
                      <span className="product-pill blue" style={{ marginLeft: 8 }}>{t("team.secretary")}</span>
                    </div>
                    <div className="office-node-sub">{t("team.secretary_desc")}</div>
                  </div>
                  <span className="office-node-dot active" />
                </Link>
                <div className="office-trunk" />
              </>
            )}
            <div className="office-departments">
              {departments.map(([dept, members]) => (
                <div key={dept} className="office-department">
                  <div className="office-department-head">
                    <span className="office-department-name">{departmentLabel(dept)}</span>
                    <span className="product-pill blue">{members.length}</span>
                  </div>
                  {members.map((employee, index) => {
                    const agentId = str(employee, "agent_id");
                    const active = normalize(employee.lifecycle_status) === "active";
                    const supervisorId = str(employee, "supervisor_agent_id");
                    const supervisor = supervisorId
                      ? employees.find((e) => str(e, "agent_id") === supervisorId)
                      : undefined;
                    const supervisorName = supervisorId === "agent-founder-01" || !supervisor
                      ? t("office.founder")
                      : str(supervisor, "display_name") || supervisorId;
                    // A subagent reports to a non-founder head (i.e. a head's helper).
                    const isSubagent = Boolean(
                      supervisor && supervisorId && supervisorId !== "agent-founder-01",
                    );
                    return (
                      <Link
                        key={agentId || `${dept}-${index}`}
                        to={`/employees/${encodeURIComponent(agentId)}`}
                        className={`office-employee-node ${isSubagent ? "office-subagent-node" : ""}`}
                        style={isSubagent ? { marginLeft: 18 } : undefined}
                      >
                        <div className="office-node-avatar"><Icon name={departmentIcon(dept)} /></div>
                        <div className="office-node-meta">
                          <div className="office-node-title">
                            {str(employee, "display_name") || agentId}
                            {isSubagent && <span className="product-pill" style={{ marginLeft: 8 }}>{t("team.subagent")}</span>}
                          </div>
                          <div className="office-node-sub">{t("office.reports_to")} {supervisorName}</div>
                        </div>
                        <span className={`office-node-dot ${active ? "active" : "idle"}`} />
                      </Link>
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      {advancedMode && (
        <section className="product-panel">
          <AIEmployees />
        </section>
      )}
    </div>
  );
}
