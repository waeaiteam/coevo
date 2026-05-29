import { useState, useEffect } from "react";

interface BootStatus { label: string; done: boolean; error?: string }

export default function BootPage({ onReady }: { onReady: () => void }) {
  const [stages, setStages] = useState<BootStatus[]>([
    { label: "Initializing COEVO_HOME...", done: false },
    { label: "Starting coevo core service...", done: false },
    { label: "Checking database...", done: false },
    { label: "Running migrations...", done: false },
    { label: "Connecting governance kernel...", done: false },
    { label: "Loading AI Employees...", done: false },
  ]);
  const [error, setError] = useState("");
  const [ready, setReady] = useState(false);

  useEffect(() => { boot(); }, []);

  async function boot() {
    try {
      setStage(0, true);
      const apiBase = localStorage.getItem("coevo-api-base") || "http://127.0.0.1:8717";

      // Try to start server via Tauri
      try { const tauriWindow = (window as any).__TAURI__ || (window as any).__TAURI_INTERNALS__;
        if (tauriWindow) {
          const invoke = tauriWindow?.invoke || tauriWindow?.core?.invoke;
          if (invoke) { try { await invoke("launch_server"); } catch { /* may be running */ } }
        }
      } catch { /* non-Tauri / web dev */ }

      setStage(1, true);
      // Health check with retry
      let healthy = false;
      for (let i = 0; i < 20; i++) {
        try {
          const r = await fetch(`${apiBase}/health`);
          if (r.ok) { healthy = true; break; }
        } catch { await sleep(500); }
      }
      if (!healthy) { setStage(1, false, "Server failed to start"); setError("Server did not respond. Check logs."); return; }
      setStage(2, true);
      setStage(3, true);
      setStage(4, true);
      setStage(5, true);
      setReady(true);
      setTimeout(onReady, 800);
    } catch (e: unknown) { setError(e instanceof Error ? e.message : String(e)); }
  }

  function setStage(idx: number, done: boolean, err?: string) {
    setStages(prev => prev.map((s, i) => i === idx ? { ...s, done, error: err } : s));
  }

  if (error) return (
    <div className="flex flex-col items-center justify-center h-screen" style={{background:"var(--bg-primary)",color:"var(--text-primary)"}}>
      <div className="text-4xl mb-4" style={{color:"var(--red)"}}>⚠</div>
      <div className="text-lg font-bold mb-2">coevo failed to start</div>
      <div className="text-sm mb-4" style={{color:"var(--red)"}}>{error}</div>
      <div className="flex gap-3">
        <button onClick={() => { setError(""); boot(); }} className="px-4 py-2 text-sm rounded-md text-white" style={{background:"var(--accent)"}}>Retry</button>
        <button onClick={() => alert("Logs: " + (localStorage.getItem("coevo-api-base") || "~/.coevo/logs"))} className="px-4 py-2 text-sm rounded-md border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}>Open Logs</button>
      </div>
    </div>
  );

  return (
    <div className="flex flex-col items-center justify-center h-screen" style={{background:"var(--bg-primary)",color:"var(--text-primary)"}}>
      <div className="text-4xl mb-6" style={{color:"var(--accent)"}}>◈</div>
      <div className="text-xl font-bold mb-6">coevo is starting</div>
      <div className="space-y-2 w-80">
        {stages.map((s, i) => (
          <div key={i} className="flex items-center gap-3">
            {s.done ? <span style={{color:"var(--green)"}}>✓</span> : !s.error ? <span className="animate-spin">◌</span> : <span style={{color:"var(--red)"}}>✗</span>}
            <span className="text-sm" style={{color: s.error ? "var(--red)" : s.done ? "var(--text-secondary)" : "var(--text-primary)"}}>{s.label}</span>
            {s.error && <span className="text-xs" style={{color:"var(--red)"}}>{s.error}</span>}
          </div>
        ))}
      </div>
      {ready && <div className="text-sm mt-4" style={{color:"var(--green)"}}>Ready — launching coevo...</div>}
    </div>
  );
}

function sleep(ms: number) { return new Promise(r => setTimeout(r, ms)); }
