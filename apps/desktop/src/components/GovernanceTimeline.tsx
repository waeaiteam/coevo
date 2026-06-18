import { useMemo, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon, { type IconName } from "./Icon";

export interface TimelineEvent {
  id: string;
  time: string;
  type: "compile" | "route" | "propose" | "risk" | "adr" | "work_order";
  message: string;
  detail?: string;
  track?: "green" | "yellow" | "red";
}

export type TimelineSpan = {
  id: string;
  type: string;
  label?: string;
  subtitle?: string;
  round?: number;
  durationMs?: number;
  tokens?: number;
  costUsd?: number;
  trust?: "native" | "external";
  gate?: {
    outcome?: "allow" | "deny" | "need_approval" | "blocked" | string;
    reason?: string;
    action_digest?: string;
  };
  overlays?: Array<"deny" | "need_approval" | "sandbox_blocked" | "hypothesis_downgraded" | string>;
  thought?: string;
  proposal?: unknown;
  confidence?: number;
  usage?: unknown;
  input?: unknown;
  output?: unknown;
};

type Props = {
  spans?: TimelineSpan[];
  events?: TimelineEvent[];
  title?: string;
  emptyText?: string;
  onApprove?: (span: TimelineSpan, comment: string) => void;
  onReject?: (span: TimelineSpan, comment: string) => void;
};

const spanIcons: Record<string, IconName> = {
  ModelCall: "brain",
  CallTool: "wrench",
  CallExecutor: "external",
  BuildContext: "layers",
  SelectTool: "filter",
  ApprovalRequired: "shield-check",
};

function normalizeEventLabel(type: string, fallback?: string) {
  const mapped = t(`timeline.event.${type}`);
  if (mapped !== `timeline.event.${type}`) return mapped;
  return (fallback || type).replace(/([a-z0-9])([A-Z])/g, "$1 $2");
}

function eventToSpan(event: TimelineEvent): TimelineSpan {
  return {
    id: event.id,
    type: event.type,
    label: event.message,
    trust: "native",
    output: event.detail || event.time,
    gate: { outcome: event.track === "red" ? "blocked" : event.track === "yellow" ? "need_approval" : "allow" },
  };
}

function formatMs(value?: number) {
  if (!value && value !== 0) return "-";
  if (value < 1000) return `${value}ms`;
  return `${(value / 1000).toFixed(1)}s`;
}

function formatCost(value?: number) {
  if (!value) return "$0.0000";
  return `$${value.toFixed(4)}`;
}

function pretty(value: unknown) {
  if (value == null || value === "") return "-";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function gateLabel(outcome?: string) {
  if (outcome === "deny") return t("timeline.gate_deny");
  if (outcome === "need_approval") return t("timeline.gate_need_approval");
  if (outcome === "blocked") return t("timeline.gate_blocked");
  return t("timeline.gate_allow");
}

function gateClass(outcome?: string) {
  if (outcome === "deny") return "deny";
  if (outcome === "need_approval") return "need_approval";
  if (outcome === "blocked") return "blocked";
  return "allow";
}

function overlayLabel(value: string) {
  if (value === "deny") return t("timeline.overlay_deny");
  if (value === "need_approval") return t("timeline.overlay_need_approval");
  if (value === "sandbox_blocked") return t("timeline.overlay_sandbox_blocked");
  if (value === "hypothesis_downgraded") return t("timeline.overlay_hypothesis_downgraded");
  return value;
}

export default function GovernanceTimeline({
  spans,
  events = [],
  title = t("timeline.execution_title"),
  emptyText = t("timeline.execution_empty"),
  onApprove,
  onReject,
}: Props) {
  useLanguage();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [comments, setComments] = useState<Record<string, string>>({});
  const normalized = useMemo(() => spans?.length ? spans : events.map(eventToSpan), [events, spans]);

  return (
    <section className="timeline-waterfall" aria-label={title}>
      <div className="timeline-header">
        <div className="text-sm font-bold">{title}</div>
        <div className="mt-1 text-xs muted">{t("timeline.execution_legend")}</div>
      </div>
      <div className="timeline-body">
        {normalized.length === 0 && <div className="timeline-empty">{emptyText}</div>}
        {normalized.map((span) => {
          const isOpen = Boolean(expanded[span.id]);
          const outcome = span.gate?.outcome;
          const isApproval = outcome === "need_approval";
          return (
            <div key={span.id} className={`span-row ${span.trust === "external" ? "external" : ""}`}>
              <div className="span-card">
                <button
                  type="button"
                  className="span-summary"
                  aria-label={span.type === "ApprovalRequired" ? t("timeline.approval_title") : normalizeEventLabel(span.type, span.label || span.type)}
                  onClick={() => setExpanded((prev) => ({ ...prev, [span.id]: !isOpen }))}
                >
                  <span className="span-icon" aria-hidden="true"><Icon name={spanIcons[span.type] ?? "info"} /></span>
                  <span className="min-w-0">
                    <span className="span-type block truncate">{normalizeEventLabel(span.type, span.label || span.type)}</span>
                    {span.subtitle && <span className="span-meta block truncate">{span.subtitle}</span>}
                  </span>
                  <span className={`span-badge ${gateClass(outcome)}`}>{gateLabel(outcome)}</span>
                  <span className="span-meta">{isOpen ? t("timeline.collapse") : t("timeline.expand")}</span>
                </button>
                {isOpen && (
                  <div className="span-detail">
                    <div className="mb-2 text-[11px] muted">
                      {t("timeline.round")} {span.round ?? 0} · {formatMs(span.durationMs)} · {span.tokens ?? 0} {t("timeline.usage")} · {formatCost(span.costUsd)}
                    </div>
                    {span.overlays && span.overlays.length > 0 && (
                      <div className="mb-3 flex flex-wrap gap-2">
                        {span.overlays.map((overlay) => (
                          <span key={overlay} className={`span-badge ${gateClass(overlay)}`}>{overlayLabel(overlay)}</span>
                        ))}
                      </div>
                    )}
                    <div className="span-grid">
                      <TimelineField label={t("timeline.field_thought")} value={span.thought} />
                      <TimelineField label={t("timeline.field_confidence")} value={span.confidence == null ? undefined : `${Math.round(span.confidence * 100)}%`} />
                      <TimelineField label={t("timeline.field_proposal")} value={span.proposal} full />
                      <TimelineField label={t("timeline.field_input")} value={span.input} />
                      <TimelineField label={t("timeline.field_output")} value={span.output} />
                      <TimelineField label={t("timeline.field_usage")} value={span.usage} />
                      <TimelineField label={t("timeline.field_gate")} value={span.gate} />
                    </div>
                    {isApproval && (
                      <TimelineApprovalControl
                        span={span}
                        comment={comments[span.id] || ""}
                        onComment={(value) => setComments((prev) => ({ ...prev, [span.id]: value }))}
                        onApprove={() => onApprove?.(span, comments[span.id] || "")}
                        onReject={() => onReject?.(span, comments[span.id] || "")}
                      />
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function TimelineField({ label, value, full = false }: { label: string; value: unknown; full?: boolean }) {
  return (
    <div className={`span-field ${full ? "full" : ""}`}>
      <div className="span-label">{label}</div>
      <pre className="span-value">{pretty(value)}</pre>
    </div>
  );
}

// Inline approval control embedded in a timeline span. This is distinct from the
// standalone components/ApprovalCard.tsx (used by MissionChat/WorkOrders): this one is
// span-scoped with a controlled comment, driven by the timeline's onApprove/onReject.
function TimelineApprovalControl({
  span,
  comment,
  onComment,
  onApprove,
  onReject,
}: {
  span: TimelineSpan;
  comment: string;
  onComment: (value: string) => void;
  onApprove: () => void;
  onReject: () => void;
}) {
  return (
    <div className="approval-card mt-3">
      <div className="text-sm font-bold">{t("timeline.approval_title")}</div>
      <div className="mt-1 text-xs secondary">{span.gate?.reason || t("timeline.approval_desc")}</div>
      <div className="mt-2 font-mono text-[11px] secondary">{span.gate?.action_digest || ""}</div>
      <textarea
        className="composer-textarea mt-3 min-h-[70px] rounded-md border"
        placeholder={t("timeline.approval_comment_placeholder")}
        value={comment}
        onChange={(event) => onComment(event.target.value)}
      />
      <div className="approval-actions">
        <button type="button" aria-label={t("timeline.approval_approve")} onClick={onApprove}>{t("timeline.approval_approve")}</button>
        <button type="button" aria-label={t("timeline.approval_reject")} onClick={onReject}>{t("timeline.approval_reject")}</button>
      </div>
    </div>
  );
}
