export default function ToolCallPanel({ toolCalls }: { toolCalls?: unknown }) {
  const items = Array.isArray(toolCalls) ? (toolCalls as Record<string, unknown>[]) : [];
  if (items.length === 0) return null;
  return (
    <div className="card space-y-2">
      <div className="text-sm font-semibold">Tool Calls</div>
      {items.map((t, i) => (
        <div key={String(t.tool_call_id || i)} className="text-xs rounded border p-2" style={{borderColor:"var(--border-subtle)"}}>
          <div className="font-semibold">{String(t.tool_id || "tool")}</div>
          <div>type: {String(t.tool_type || "-")}</div>
          <div>success: {String(t.success)}</div>
          <div>risk: {String(t.risk_ceiling ?? "-")}</div>
          <div>memory: {String(t.memory_id || "none")}</div>
        </div>
      ))}
    </div>
  );
}
