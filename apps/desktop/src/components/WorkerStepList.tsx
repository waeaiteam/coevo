export default function WorkerStepList({ steps }: { steps?: unknown }) {
  const items = Array.isArray(steps) ? (steps as Record<string, unknown>[]) : [];
  if (items.length === 0) return null;
  return (
    <div className="card space-y-2">
      <div className="text-sm font-semibold">Worker Steps</div>
      {items.map((s, i) => (
        <div key={String(s.step_id || i)} className="text-xs rounded border p-2" style={{borderColor:"var(--border-subtle)"}}>
          <div className="flex items-center gap-2">
            <span className="font-mono" style={{color:"var(--accent)"}}>#{String(s.step_index ?? i)}</span>
            <span className="font-semibold">{String(s.step_type || "Step")}</span>
            <span>{String(s.status || "Unknown")}</span>
          </div>
          {s.error ? <div style={{color:"var(--red)"}}>Error: {String(s.error)}</div> : null}
        </div>
      ))}
    </div>
  );
}
