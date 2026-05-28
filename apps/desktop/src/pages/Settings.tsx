export default function Settings() {
  return (
    <div className="space-y-5 max-w-lg">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--text-muted)" }}>⚙</span>
        <h2 className="text-lg font-bold">Settings</h2>
      </div>
      <div className="card space-y-3">
        <div className="flex justify-between items-center text-sm">
          <span>Backend URL</span>
          <code className="text-xs px-2 py-1 rounded" style={{ background: "var(--bg-secondary)", color: "var(--accent)" }}>http://127.0.0.1:8717</code>
        </div>
        <div className="flex justify-between items-center text-sm">
          <span>API Version</span>
          <code className="text-xs px-2 py-1 rounded" style={{ background: "var(--bg-secondary)" }}>v1.0.0</code>
        </div>
        <div className="flex justify-between items-center text-sm">
          <span>Desktop Version</span>
          <code className="text-xs px-2 py-1 rounded" style={{ background: "var(--bg-secondary)" }}>1.0.0</code>
        </div>
        <div className="flex justify-between items-center text-sm">
          <span>Theme</span>
          <code className="text-xs px-2 py-1 rounded" style={{ background: "var(--bg-secondary)" }}>Light</code>
        </div>
      </div>
    </div>
  );
}
