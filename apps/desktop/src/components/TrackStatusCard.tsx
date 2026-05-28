export default function TrackStatusCard({
  track,
  metrics,
}: {
  track: "green" | "yellow" | "red";
  metrics: { label: string; value: string }[];
}) {
  const trackStyles = {
    green: { border: "rgba(34,197,94,0.3)", bg: "rgba(34,197,94,0.04)", dot: "var(--green)" },
    yellow: { border: "rgba(234,179,8,0.3)", bg: "rgba(234,179,8,0.04)", dot: "var(--yellow)" },
    red: { border: "rgba(239,68,68,0.3)", bg: "rgba(239,68,68,0.04)", dot: "var(--red)" },
  };
  const s = trackStyles[track];

  return (
    <div className="card" style={{ borderLeft: `2px solid ${s.border}`, background: s.bg }}>
      <div className="flex items-center gap-2 mb-3">
        <span className={`status-dot`} style={{ background: s.dot, boxShadow: `0 0 6px ${s.dot}` }} />
        <span className={`track-${track}`}>{track.toUpperCase()} TRACK</span>
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
