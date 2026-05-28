import { useState } from "react";
import { runDemo } from "../api/client";
import type { DemoResponse } from "../types";

export default function DemoActionPanel({
  onResult,
}: {
  onResult: (r: DemoResponse) => void;
}) {
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handle(track: "green" | "yellow" | "red") {
    setLoading(track);
    setError(null);
    try {
      const r = await runDemo(track);
      onResult(r);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(null);
    }
  }

  return (
    <div className="card">
      <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
        Demo Actions
      </div>
      <div className="flex gap-2">
        <button
          onClick={() => handle("green")}
          disabled={loading !== null}
          className="flex-1 py-2 text-xs font-semibold rounded-md border transition-all duration-150 disabled:opacity-30"
          style={{
            background: loading === "green" ? "rgba(34,197,94,0.15)" : "transparent",
            borderColor: "rgba(34,197,94,0.3)",
            color: "var(--green)",
          }}
        >
          {loading === "green" ? "···" : "Green"}
        </button>
        <button
          onClick={() => handle("yellow")}
          disabled={loading !== null}
          className="flex-1 py-2 text-xs font-semibold rounded-md border transition-all duration-150 disabled:opacity-30"
          style={{
            background: loading === "yellow" ? "rgba(234,179,8,0.15)" : "transparent",
            borderColor: "rgba(234,179,8,0.3)",
            color: "var(--yellow)",
          }}
        >
          {loading === "yellow" ? "···" : "Yellow"}
        </button>
        <button
          onClick={() => handle("red")}
          disabled={loading !== null}
          className="flex-1 py-2 text-xs font-semibold rounded-md border transition-all duration-150 disabled:opacity-30"
          style={{
            background: loading === "red" ? "rgba(239,68,68,0.15)" : "transparent",
            borderColor: "rgba(239,68,68,0.3)",
            color: "var(--red)",
          }}
        >
          {loading === "red" ? "···" : "Red"}
        </button>
      </div>
      {error && (
        <div className="mt-2 text-xs p-2 rounded border" style={{ color: "var(--red)", borderColor: "rgba(239,68,68,0.3)", background: "rgba(239,68,68,0.06)" }}>
          {error}
        </div>
      )}
    </div>
  );
}
