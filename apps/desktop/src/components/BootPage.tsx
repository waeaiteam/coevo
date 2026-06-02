import { useState, useEffect, useRef } from "react";
import { getApiBase, setApiBase } from "../api/client";
import { getTauriInvoke } from "../api/tauri";
import { t, useLanguage } from "../settings/i18n";

interface BootStatus { label: string; done: boolean; error?: string }

export default function BootPage({ onReady }: { onReady: () => void }) {
  useLanguage();
  const [stages, setStages] = useState<BootStatus[]>([
    { label: t("boot.stage_workspace"), done: false },
    { label: t("boot.stage_service"), done: false },
    { label: t("boot.stage_data"), done: false },
    { label: t("boot.stage_schema"), done: false },
    { label: t("boot.stage_guard"), done: false },
    { label: t("boot.stage_employees"), done: false },
  ]);
  const [error, setError] = useState("");
  const bootStarted = useRef(false);

  useEffect(() => {
    if (bootStarted.current) return;
    bootStarted.current = true;
    boot();
  }, []);

  async function boot() {
    try {
      setStage(0);
      // Try Tauri launch_server — returns dynamic apiBase
      const invoke = getTauriInvoke();
      let apiBase = "";
      if (invoke) {
        try { apiBase = await invoke("launch_server"); } catch (e: unknown) {
          setError(`${t("boot.err_launch")}${e instanceof Error ? e.message : String(e)}`); return;
        }
        if (!apiBase) { setError(t("boot.err_no_address")); return; }
      } else {
        // Web dev mode: try existing server
        apiBase = getApiBase();
      }
      setApiBase(apiBase);

      setStage(1);
      let healthy = false;
      for (let i = 0; i < 20; i++) {
        try { const r = await fetch(`${apiBase}/health`); if (r.ok) { healthy = true; break; } } catch { await sleep(500); }
      }
      if (!healthy) { setStage(1, t("boot.err_no_response")); setError(t("boot.err_no_response")); return; }
      setStage(2); setStage(3); setStage(4); setStage(5);
      setTimeout(onReady, 600);
    } catch (e: unknown) { setError(e instanceof Error ? e.message : String(e)); }
  }

  function setStage(idx: number, err?: string) {
    setStages(prev => prev.map((s, i) => i <= idx ? { ...s, done: i < idx ? true : !err, error: i === idx ? err : undefined } : { ...s, error: undefined }));
  }

  async function openLogs() {
    const invoke = getTauriInvoke();
    if (invoke) { try { await invoke("open_logs_dir"); return; } catch {} }
    alert(t("boot.logs_location"));
  }

  if (error) return (
    <div className="boot-screen">
      <div className="boot-mark boot-mark-error">!</div>
      <div className="boot-title">{t("boot.failed")}</div>
      <div className="boot-error">{error}</div>
      <div className="boot-actions">
        <button onClick={() => { setError(""); boot(); }} className="boot-button">{t("boot.retry")}</button>
        <button onClick={openLogs} className="boot-button boot-button-secondary">{t("boot.open_logs")}</button>
      </div>
    </div>
  );

  return (
    <div className="boot-screen">
      <div className="boot-mark">c</div>
      <div className="boot-title">{t("boot.preparing")}</div>
      <div className="boot-list">
        {stages.map((s, i) => (
          <div key={i} className="boot-row">
            {s.done ? <span className="boot-icon boot-icon-done">✓</span> : s.error ? <span className="boot-icon boot-icon-error">×</span> : <span className="boot-icon boot-icon-pending" />}
            <span className={s.error ? "boot-row-label is-error" : s.done ? "boot-row-label is-done" : "boot-row-label"}>{s.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function sleep(ms: number) { return new Promise(r => setTimeout(r, ms)); }
