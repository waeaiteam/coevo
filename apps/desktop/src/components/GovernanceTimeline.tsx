import { useMemo, useState } from "react";

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

const spanIcons: Record<string, string> = {
  ModelCall: "思",
  CallTool: "工",
  CallExecutor: "外",
  BuildContext: "境",
  SelectTool: "选",
  ApprovalRequired: "审",
};

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
  if (outcome === "deny") return "已拒绝";
  if (outcome === "need_approval") return "待确认";
  if (outcome === "blocked") return "已拦截";
  return "已通过";
}

function gateClass(outcome?: string) {
  if (outcome === "deny") return "deny";
  if (outcome === "need_approval") return "need_approval";
  if (outcome === "blocked") return "blocked";
  return "allow";
}

function overlayLabel(value: string) {
  if (value === "deny") return "Deny";
  if (value === "need_approval") return "NeedApproval";
  if (value === "sandbox_blocked") return "沙箱拦截";
  if (value === "hypothesis_downgraded") return "Hypothesis 降级";
  return value;
}

export default function GovernanceTimeline({
  spans,
  events = [],
  title = "执行时间线",
  emptyText = "任务提交后，这里会显示每一步的判断、用量和结果。",
  onApprove,
  onReject,
}: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [comments, setComments] = useState<Record<string, string>>({});
  const normalized = useMemo(() => spans?.length ? spans : events.map(eventToSpan), [events, spans]);

  return (
    <section className="timeline-waterfall" aria-label={title}>
      <div className="timeline-header">
        <div className="text-sm font-bold">{title}</div>
        <div className="mt-1 text-xs muted">实线来自本地执行，虚线来自外部员工回报。</div>
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
                  onClick={() => setExpanded((prev) => ({ ...prev, [span.id]: !isOpen }))}
                >
                  <span className="span-icon" aria-hidden="true">{spanIcons[span.type] || "•"}</span>
                  <span className="min-w-0">
                    <span className="span-type block truncate">{span.label || span.type}</span>
                    <span className="span-meta">round {span.round ?? 0}</span>
                  </span>
                  <span className="span-meta optional">{formatMs(span.durationMs)}</span>
                  <span className="span-meta optional">{span.tokens ?? 0} 用量</span>
                  <span className="span-meta optional">{formatCost(span.costUsd)}</span>
                  <span className={`span-badge ${gateClass(outcome)}`}>{gateLabel(outcome)}</span>
                  <span className="span-meta">{isOpen ? "收起" : "展开"}</span>
                </button>
                {isOpen && (
                  <div className="span-detail">
                    {span.overlays && span.overlays.length > 0 && (
                      <div className="mb-3 flex flex-wrap gap-2">
                        {span.overlays.map((overlay) => (
                          <span key={overlay} className={`span-badge ${gateClass(overlay)}`}>{overlayLabel(overlay)}</span>
                        ))}
                      </div>
                    )}
                    <div className="span-grid">
                      <TimelineField label="思考" value={span.thought} />
                      <TimelineField label="置信度" value={span.confidence == null ? undefined : `${Math.round(span.confidence * 100)}%`} />
                      <TimelineField label="提案" value={span.proposal} full />
                      <TimelineField label="输入" value={span.input} />
                      <TimelineField label="输出" value={span.output} />
                      <TimelineField label="用量" value={span.usage} />
                      <TimelineField label="裁定" value={span.gate} />
                    </div>
                    {isApproval && (
                      <ApprovalCard
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

function ApprovalCard({
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
      <div className="text-sm font-bold">需要确认</div>
      <div className="mt-1 text-xs secondary">{span.gate?.reason || "这一步会影响你的数据或工作区，请确认后继续。"}</div>
      <div className="mt-2 font-mono text-[11px] secondary">{span.gate?.action_digest || ""}</div>
      <textarea
        className="composer-textarea mt-3 min-h-[70px] rounded-md border"
        placeholder="批注意见，可留空"
        value={comment}
        onChange={(event) => onComment(event.target.value)}
      />
      <div className="approval-actions">
        <button type="button" onClick={onApprove}>批准</button>
        <button type="button" onClick={onReject}>拒绝</button>
      </div>
    </div>
  );
}
