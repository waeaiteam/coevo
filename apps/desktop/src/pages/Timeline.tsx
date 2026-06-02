import { useEffect, useMemo, useState } from "react";
import { getGlobalTimeline } from "../api/client";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { t, useLanguage } from "../settings/i18n";

type TimelineRow = Record<string, unknown>;

function parseDetails(row: TimelineRow): Record<string, unknown> {
  const details = row.details;
  return details && typeof details === "object" && !Array.isArray(details)
    ? details as Record<string, unknown>
    : {};
}

function timelineRowsToSpans(rows: TimelineRow[]): TimelineSpan[] {
  return rows.map((row, index) => {
    const details = parseDetails(row);
    const track = String(row.track || details.track || "green");
    const status = String(row.status || details.status || "");
    const type = String(row.type || "Activity");
    const mission = String(row.mission_intent || "");
    const title = String(row.title || type);
    return {
      id: String(details.event_id || details.session_id || row.work_order_id || `${type}-${index}`),
      type,
      label: title,
      subtitle: mission || undefined,
      round: 0,
      durationMs: 0,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: {
        outcome: track === "red" ? "blocked" : track === "yellow" && status === "WaitingApproval" ? "need_approval" : "allow",
        reason: String(details.risk_summary || ""),
      },
      output: {
        mission_intent: row.mission_intent,
        work_order_id: row.work_order_id,
        status,
        details,
      },
    };
  });
}

export default function Timeline() {
  useLanguage();
  const [rows, setRows] = useState<TimelineRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const spans = useMemo(() => timelineRowsToSpans(rows), [rows]);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError("");
    getGlobalTimeline()
      .then((items) => {
        if (alive) setRows(Array.isArray(items) ? items : []);
      })
      .catch((e: unknown) => {
        if (alive) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <div>
        <div className="text-[10px] font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
          {t("nav.timeline")}
        </div>
        <h1 className="mt-1 text-xl font-bold">{t("timeline.title")}</h1>
        <p className="mt-1 text-xs leading-5" style={{ color: "var(--text-muted)" }}>
          {t("timeline.subtitle")}
        </p>
      </div>
      {error && (
        <div className="rounded-md border p-3 text-xs" style={{ borderColor: "var(--red)", color: "var(--red)" }}>
          {error}
        </div>
      )}
      <GovernanceTimeline
        spans={spans}
        title={t("timeline.activity")}
        emptyText={loading ? t("settings.loading") : t("timeline.empty")}
      />
    </div>
  );
}
