import { useState, useCallback } from "react";
import { runDemo } from "../api/client";
import type { DemoResponse } from "../types";

export default function Demos() {
  const [results, setResults] = useState<DemoResponse[]>([]);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleRun = useCallback(async (track: "green" | "yellow" | "red") => {
    setLoading(track);
    setError(null);
    try {
      const r = await runDemo(track);
      setResults((prev) => [r, ...prev]);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(null);
    }
  }, []);

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--accent)" }}>▶</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>Demo Scenarios</h2>
      </div>

      <div className="card flex gap-2">
        {(["green", "yellow", "red"] as const).map((track) => (
          <button
            key={track}
            onClick={() => handleRun(track)}
            disabled={loading !== null}
            className="flex-1 py-2 text-xs font-semibold rounded-md border transition-all duration-150 disabled:opacity-30"
            style={{
              background: loading === track ? `rgba(${track==="green"?"34,197,94":track==="yellow"?"234,179,8":"239,68,68"},0.15)` : "transparent",
              borderColor: `rgba(${track==="green"?"34,197,94":track==="yellow"?"234,179,8":"239,68,68"},0.3)`,
              color: `var(--${track})`,
            }}
          >
            {loading === track ? "···" : `${track.toUpperCase()} Track`}
          </button>
        ))}
      </div>

      {error && (
        <div className="card" style={{ borderColor: "rgba(239,68,68,0.4)", background: "rgba(239,68,68,0.06)" }}>
          <div className="text-xs" style={{ color: "var(--red)" }}>{error}</div>
        </div>
      )}

      {results.map((r, i) => (
        <div key={i} className="card glow-top" style={{ borderTopColor: r.track === "green" ? "var(--green)" : r.track === "yellow" ? "var(--yellow)" : "var(--red)" }}>
          <div className="flex items-center justify-between mb-2">
            <span className={`track-${r.track}`}>{r.track.toUpperCase()}</span>
            <span className="text-xs" style={{ color: "var(--text-muted)" }}>{r.elapsed_ms}ms</span>
          </div>
          <div className="grid grid-cols-2 gap-2 text-xs font-mono">
            <div><span style={{ color: "var(--text-muted)" }}>Contract:</span> <span style={{ color: "var(--text-secondary)" }}>{r.contract_hash.slice(0, 16)}...</span></div>
            <div><span style={{ color: "var(--text-muted)" }}>Plan:</span> <span style={{ color: "var(--text-secondary)" }}>{r.plan_hash.slice(0, 16)}...</span></div>
          </div>
          <div className="mt-2">
            <span className="text-xs" style={{ color: "var(--text-muted)" }}>Entries: </span>
            {r.entries_created.map((e, j) => (
              <code key={j} className="text-xs ml-1 px-1.5 py-0.5 rounded" style={{ background: "var(--bg-primary)", color: "var(--accent)" }}>{e}</code>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
