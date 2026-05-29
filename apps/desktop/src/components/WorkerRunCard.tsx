type Props = { result?: Record<string, unknown> | null };

function arr(v: unknown): Record<string, unknown>[] {
  return Array.isArray(v) ? (v as Record<string, unknown>[]) : [];
}

export default function WorkerRunCard({ result }: Props) {
  const run = arr(result?.worker_runs)[0];
  if (!run) return null;
  const memoryIds = Array.isArray(result?.memory_ids) ? result?.memory_ids as unknown[] : [];
  return (
    <div className="card space-y-1">
      <div className="flex items-center justify-between">
        <div className="text-sm font-semibold">WorkerRun</div>
        <span className="text-xs px-1.5 py-0.5 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>{String(run.status || result?.status || "Unknown")}</span>
      </div>
      <div className="grid grid-cols-2 gap-1 text-xs" style={{color:"var(--text-muted)"}}>
        <div>run_id: <span className="font-mono">{String(run.run_id || "-")}</span></div>
        <div>worker: <span className="font-mono">{String(run.worker_id || "-")}</span></div>
        <div>agent: <span className="font-mono">{String(run.agent_id || "-")}</span></div>
        <div>session: <span className="font-mono">{String(run.session_id || "-")}</span></div>
        <div>memory writes: {memoryIds.length}</div>
        <div>reflection: <span className="font-mono">{String(result?.reflection_id || "none")}</span></div>
        <div>proposal: <span className="font-mono">{String(result?.proposal_id || "none")}</span></div>
      </div>
    </div>
  );
}
