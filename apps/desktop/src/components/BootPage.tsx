import { useState, useEffect } from "react";
import { setApiBase } from "../api/client";

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

  useEffect(() => { boot(); }, []);

  async function getInvoke() {
    try {
      const w = window as any;
      if (w.__TAURI_INTERNALS__) {
        const mod = await (Function('return import("@tauri-apps/api/core")')());
        return mod.invoke;
      }
    } catch { /* web */ }
    return null;
  }

  async function boot() {
    try {
      setStage(0);
      let apiBase = "http://127.0.0.1:8717";

      // Try Tauri launch_server — returns dynamic apiBase
      const invoke = await getInvoke();
      if (invoke) {
        try { apiBase = await invoke("launch_server"); } catch { /* already running */ }
        try { apiBase = await invoke("get_api_base"); } catch {}
        if (!apiBase || apiBase === "http://127.0.0.1:8717") {
          try { apiBase = `http://127.0.0.1:${await invoke("get_server_port")}`; } catch {}
        }
      }
      setApiBase(apiBase);

      setStage(1);
      let healthy = false;
      for (let i = 0; i < 20; i++) {
        try { const r = await fetch(`${apiBase}/health`); if (r.ok) { healthy = true; break; } } catch { await sleep(500); }
      }
      if (!healthy) { setStage(1, "Server did not respond. Check logs."); setError("Server did not respond. Check logs."); return; }
      setStage(2); setStage(3); setStage(4); setStage(5);
      setTimeout(onReady, 600);
    } catch (e: unknown) { setError(e instanceof Error ? e.message : String(e)); }
  }

  function setStage(idx: number, err?: string) {
    setStages(prev => prev.map((s, i) => i <= idx ? { ...s, done: i < idx ? true : !err, error: i === idx ? err : undefined } : { ...s, error: undefined }));
  }

  async function openLogs() {
    const invoke = await getInvoke();
    if (invoke) { try { await invoke("open_logs_dir"); return; } catch {} }
    alert("Logs: ~/.coevo/logs");
  }

  if (error) return (
    <div className="flex flex-col items-center justify-center h-screen" style={{background:"var(--bg-primary)",color:"var(--text-primary)"}}>
      <div className="text-4xl mb-4" style={{color:"var(--red)"}}>⚠</div>
      <div className="text-lg font-bold mb-2">coevo failed to start</div>
      <div className="text-sm mb-4" style={{color:"var(--red)"}}>{error}</div>
      <div className="flex gap-3">
        <button onClick={() => { setError(""); boot(); }} className="px-4 py-2 text-sm rounded-md text-white" style={{background:"var(--accent)"}}>Retry</button>
        <button onClick={openLogs} className="px-4 py-2 text-sm rounded-md border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}>Open Logs</button>
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
            {s.done ? <span style={{color:"var(--green)"}}>✓</span> : s.error ? <span style={{color:"var(--red)"}}>✗</span> : <span className="animate-spin">◌</span>}
            <span className="text-sm" style={{color: s.error ? "var(--red)" : s.done ? "var(--text-secondary)" : "var(--text-primary)"}}>{s.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function sleep(ms: number) { return new Promise(r => setTimeout(r, ms)); }
