import { t } from "../settings/i18n";

// Status of a task's execution lane, in plain language. Colors come from theme tokens
// (never hardcoded rgba) so light/dark stay correct.
export default function TrackStatusCard({
  track,
  metrics,
}: {
  track: "green" | "yellow" | "red";
  metrics: { label: string; value: string }[];
}) {
  const trackStyles = {
    green: { border: "var(--green)", bg: "var(--green-dim)", dot: "var(--green)" },
    yellow: { border: "var(--yellow)", bg: "var(--yellow-dim)", dot: "var(--yellow)" },
    red: { border: "var(--red)", bg: "var(--red-dim)", dot: "var(--red)" },
  };
  const s = trackStyles[track];
  const label = track === "green"
    ? t("track.auto")
    : track === "yellow"
      ? t("track.needs_confirm")
      : t("track.paused");

  return (
    <div className="card" style={{ borderLeft: `2px solid ${s.border}`, background: s.bg }}>
      <div className="flex items-center gap-2 mb-3">
        <span className="status-dot" style={{ background: s.dot }} />
        <span className={`track-${track}`}>{label}</span>
      </div>
      <div className="space-y-2">
        {metrics.map((m) => (
          <div key={m.label} className="flex justify-between text-xs">
            <span style={{ color: "var(--text-muted)" }}>{m.label}</span>
            <span className="font-mono tracking-tight" style={{ color: "var(--text-primary)" }}>{m.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
