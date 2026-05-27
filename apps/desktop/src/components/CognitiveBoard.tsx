export default function CognitiveBoard({ result }: { result: unknown }) {
  const r = result as Record<string, unknown>;
  if (r.error) {
    return <div className="p-3 bg-red-50 border border-red-200 rounded text-sm text-red-700">{String(r.error)}</div>;
  }
  return (
    <div className="bg-white border rounded p-4 text-sm">
      <h3 className="font-bold mb-2">Commit Receipt</h3>
      <pre className="bg-gray-50 p-2 rounded text-xs overflow-x-auto">{JSON.stringify(r, null, 2)}</pre>
    </div>
  );
}
