import Icon, { type IconName } from "./Icon";
import { t } from "../settings/i18n";

// Shared trace-span waterfall, used by both the Traces and Workflows (advanced) pages.
// Previously each page had its own identical copy of Waterfall + kindIcon.

export type TraceSpan = {
  span_id: string;
  parent_span_id?: string | null;
  name: string;
  kind: string;
  status: string;
  started_at_ms: number;
  ended_at_ms?: number | null;
  input?: string;
  output?: string;
};

function kindIcon(kind: string): IconName {
  switch (kind) {
    case "mission":
      return "sparkles";
    case "subtask":
      return "list-checks";
    case "model_call":
      return "brain";
    case "tool_call":
      return "wrench";
    case "governance":
      return "shield-check";
    default:
      return "info";
  }
}

export function TraceWaterfall({ spans }: { spans: TraceSpan[] }) {
  if (spans.length === 0) return null;
  const minStart = Math.min(...spans.map((span) => span.started_at_ms));
  const maxEnd = Math.max(...spans.map((span) => span.ended_at_ms || Date.now()));
  const totalDuration = Math.max(1, maxEnd - minStart);

  const depthMap = new Map<string, number>();
  const sorted = [...spans].sort((a, b) => a.started_at_ms - b.started_at_ms);
  for (const span of sorted) {
    const parentDepth = span.parent_span_id ? depthMap.get(span.parent_span_id) ?? 0 : -1;
    depthMap.set(span.span_id, parentDepth + 1);
  }

  return (
    <div className="trace-waterfall">
      {sorted.map((span) => {
        const offset = ((span.started_at_ms - minStart) / totalDuration) * 100;
        const width = (((span.ended_at_ms || Date.now()) - span.started_at_ms) / totalDuration) * 100;
        const depth = depthMap.get(span.span_id) ?? 0;
        const color = span.status === "error" ? "var(--red)" : span.status === "running" ? "var(--blue)" : "var(--accent)";
        return (
          <div key={span.span_id} className="trace-span-row">
            <div className="trace-span-label" style={{ paddingLeft: depth * 14 }}>
              <Icon name={kindIcon(span.kind)} />
              <span className="truncate">{span.name}</span>
            </div>
            <div className="trace-span-track">
              <div
                className="trace-span-bar"
                style={{ marginLeft: `${offset}%`, width: `${Math.max(2, width)}%`, background: color }}
                title={`${(span.ended_at_ms || Date.now()) - span.started_at_ms}ms`}
              />
            </div>
            <span className="trace-span-time">
              {span.ended_at_ms != null ? `${span.ended_at_ms - span.started_at_ms}ms` : t("traces.running")}
            </span>
          </div>
        );
      })}
    </div>
  );
}
