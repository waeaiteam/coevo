function parseOutput(v: unknown): Record<string, unknown> | null {
  if (!v) return null;
  if (typeof v === "string") { try { return JSON.parse(v) as Record<string, unknown>; } catch { return null; } }
  if (typeof v === "object") return v as Record<string, unknown>;
  return null;
}

export default function ModelRoutingPanel({ steps }: { steps?: unknown }) {
  const items = Array.isArray(steps) ? (steps as Record<string, unknown>[]) : [];
  const modelSteps = items.filter(s => String(s.step_type || "") === "ModelCall" || String(s.step_type || "") === "ModelRoute");
  if (modelSteps.length === 0) return null;
  return (
    <div className="card space-y-2">
      <div className="text-sm font-semibold">Model Routing</div>
      <div className="text-xs p-2 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>
        Model provides cognition, not authorization. Execution is governed by WorkOrder, RiskGate, and ToolPolicy.
      </div>
      {modelSteps.map((s, i) => {
        const out = parseOutput(s.output_json);
        if (!out) return null;
        return (
          <div key={String(s.step_id || i)} className="text-xs rounded border p-2" style={{borderColor:"var(--border-subtle)"}}>
            <div>model: <span className="font-mono">{String(out.selected_model_id || "-")}</span></div>
            <div>provider: <span className="font-mono">{String(out.selected_provider_id || "-")}</span></div>
            <div>reason: {String(out.reason || "-")}</div>
            <div>fallbacks: {Array.isArray(out.fallback_model_ids) ? out.fallback_model_ids.join(", ") : "-"}</div>
            <div>notes: {Array.isArray(out.governance_notes) ? out.governance_notes.join(" | ") : "-"}</div>
          </div>
        );
      })}
    </div>
  );
}
