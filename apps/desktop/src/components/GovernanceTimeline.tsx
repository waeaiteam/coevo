export interface TimelineEvent {
  id: string;
  time: string;
  type: "compile" | "route" | "propose" | "risk" | "adr" | "demo";
  message: string;
  detail?: string;
  track?: "green" | "yellow" | "red";
}

export default function GovernanceTimeline({ events }: { events: TimelineEvent[] }) {
  const colors: Record<string, string> = {
    compile: "var(--accent)",
    route: "var(--blue)",
    propose: "#8b5cf6",
    risk: "var(--yellow)",
    adr: "#ec4899",
    demo: "var(--green)",
  };

  return (
    <div className="card">
      <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
        Governance Timeline
      </div>
      <div className="space-y-3 max-h-80 overflow-y-auto">
        {events.length === 0 && (
          <div className="text-xs py-6 text-center" style={{ color: "var(--text-muted)" }}>
            No events yet — trigger a demo to populate
          </div>
        )}
        {events.map((e, i) => (
          <div key={e.id} className="flex gap-3">
            <div className="flex flex-col items-center">
              <div className="timeline-dot" style={{ background: colors[e.type] || "var(--accent)" }} />
              {i < events.length - 1 && <div className="timeline-line" />}
            </div>
            <div className="flex-1 min-w-0 pb-2">
              <div className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
                {e.message}
                {e.track && <span className={`track-${e.track} ml-2`}>{e.track}</span>}
              </div>
              {e.detail && (
                <div className="text-xs mt-0.5 font-mono truncate" style={{ color: "var(--text-muted)" }}>
                  {e.detail}
                </div>
              )}
              <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>
                {e.time}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
