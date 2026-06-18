import { useEffect, useMemo, useState } from "react";
import { getActiveOpcId } from "../api/companies";
import { listCompanyAuditEvents, type CompanyAuditEvent } from "../api/org";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

type ParsedAuditEvent = CompanyAuditEvent & {
  parsedData: Record<string, unknown> | null;
};

export default function Audit() {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [events, setEvents] = useState<ParsedAuditEvent[]>([]);
  const [activeEventId, setActiveEventId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");

    void listCompanyAuditEvents(activeOpcId, { limit: 100 })
      .then((rows) => {
        if (cancelled) return;
        const next = rows.map((row) => ({
          ...row,
          parsedData: parseEventData(row.event_data_json),
        }));
        setEvents(next);
        setActiveEventId((current) => current || next[0]?.id || null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [activeOpcId]);

  const activeEvent = events.find((event) => event.id === activeEventId) || null;

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

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("audit.title")}</h2>
            <span>{events.length}</span>
          </div>
          {loading ? (
            <div className="empty-state"><p>{t("executors.loading")}</p></div>
          ) : error ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="info" /></div>
              <p>{error}</p>
            </div>
          ) : events.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="clipboard" /></div>
              <p>{t("audit.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {events.map((event) => {
                const parsed = event.parsedData;
                const workOrderId = stringValue(parsed?.work_order_id);
                const runId = stringValue(parsed?.run_id);
                return (
                  <button
                    key={event.id}
                    className="product-list-row"
                    onClick={() => setActiveEventId(event.id)}
                    style={{ borderColor: event.id === activeEventId ? "var(--accent)" : undefined }}
                  >
                    <span className="product-row-main">{event.event_type}</span>
                    <span className="flex flex-wrap items-center justify-end gap-2 text-[11px]">
                      {workOrderId ? <span className="mono-chip">{workOrderId}</span> : null}
                      {runId ? <span className="mono-chip">{runId}</span> : null}
                      <span className="mono-chip">{formatTime(event.recorded_at_ms)}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{activeEvent?.event_type || t("audit.desc")}</h2>
          </div>
          {!activeEvent ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>{error || t("audit.select")}</p>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="grid gap-3 md:grid-cols-2">
                <InfoRow label="Event ID" value={activeEvent.id} mono />
                <InfoRow label="Recorded" value={formatDateTime(activeEvent.recorded_at_ms)} />
                <InfoRow label="Agent" value={activeEvent.agent_id || "-"} />
                <InfoRow label="Tenant" value={activeEvent.tenant_id} mono />
                <InfoRow label="Contract" value={activeEvent.contract_hash || "-"} mono />
                <InfoRow label="Traceparent" value={activeEvent.traceparent || "-"} mono />
              </div>

              <div className="rounded border p-3" style={{ borderColor: "var(--border-subtle)" }}>
                <div className="mb-2 text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>
                  Event Data
                </div>
                <pre className="overflow-auto text-xs whitespace-pre-wrap break-words">
                  {formatEventData(activeEvent)}
                </pre>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function InfoRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="rounded border p-3" style={{ borderColor: "var(--border-subtle)" }}>
      <div className="text-[11px]" style={{ color: "var(--text-secondary)" }}>{label}</div>
      <div className={mono ? "mt-1 text-sm font-medium font-mono break-all" : "mt-1 text-sm font-medium break-words"}>
        {value}
      </div>
    </div>
  );
}

function parseEventData(value: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(value) as unknown;
    return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function formatEventData(event: ParsedAuditEvent): string {
  if (event.parsedData) {
    return JSON.stringify(event.parsedData, null, 2);
  }
  return event.event_data_json;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function formatTime(epochMs: number): string {
  try {
    return new Date(epochMs).toLocaleTimeString();
  } catch {
    return String(epochMs);
  }
}

function formatDateTime(epochMs: number): string {
  try {
    return new Date(epochMs).toLocaleString();
  } catch {
    return String(epochMs);
  }
}
