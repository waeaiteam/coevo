export default function CognitiveBoard({ result }: { result: unknown }) {
  const r = result as Record<string, unknown>;
  if (r.error) {
    return <div className="p-3 border rounded text-sm" style={{ background: "var(--red-dim)", borderColor: "var(--red)", color: "var(--red)" }}>{String(r.error)}</div>;
  }
  return (
    <div className="card text-sm">
      <h3 className="font-bold mb-2">Commit Receipt</h3>
      <pre className="p-2 rounded text-xs overflow-x-auto" style={{ background: "var(--surface-raised)" }}>{JSON.stringify(r, null, 2)}</pre>
    </div>
  );
}
