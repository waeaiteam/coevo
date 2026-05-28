export default function MetricCard({
  label,
  value,
  sub,
  accent = "default",
}: {
  label: string;
  value: string | number;
  sub?: string;
  accent?: "green" | "yellow" | "red" | "blue" | "purple" | "default";
}) {
  const colors: Record<string, string> = {
    green: "var(--green)",
    yellow: "var(--yellow)",
    red: "var(--red)",
    blue: "var(--blue)",
    purple: "var(--accent)",
    default: "var(--text-primary)",
  };

  return (
    <div className="card glow-top" style={{ borderTopColor: colors[accent] || "var(--border-subtle)" }}>
      <div className="metric-label">{label}</div>
      <div className="metric-value mt-1" style={{ color: colors[accent] }}>
        {value}
      </div>
      {sub && (
        <div className="text-xs mt-1 tracking-wide" style={{ color: "var(--text-muted)" }}>
          {sub}
        </div>
      )}
    </div>
  );
}
