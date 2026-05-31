import { useState, useEffect } from "react";
import { setApiBase } from "../api/client";
import { getTauriInvoke } from "../api/tauri";

interface BootStatus { label: string; done: boolean; error?: string }

export default function BootPage({ onReady }: { onReady: () => void }) {
  const [stages, setStages] = useState<BootStatus[]>([
    { label: "准备本地工作区", done: false },
    { label: "启动本地 AI 员工服务", done: false },
    { label: "检查本地数据", done: false },
    { label: "更新工作区结构", done: false },
    { label: "连接安全守护", done: false },
    { label: "载入 AI 员工", done: false },
  ]);
  const [error, setError] = useState("");

  useEffect(() => { boot(); }, []);

  async function boot() {
    try {
      setStage(0);
      // Try Tauri launch_server — returns dynamic apiBase
      const invoke = getTauriInvoke();
      let apiBase = "";
      if (invoke) {
        try { apiBase = await invoke("launch_server"); } catch (e: unknown) {
          setError(`启动本地服务失败：${e instanceof Error ? e.message : String(e)}`); return;
        }
        if (!apiBase) { setError("本地服务没有返回连接地址"); return; }
      } else {
        // Web dev mode: try existing server
        apiBase = "http://127.0.0.1:8717";
      }
      setApiBase(apiBase);

      setStage(1);
      let healthy = false;
      for (let i = 0; i < 20; i++) {
        try { const r = await fetch(`${apiBase}/health`); if (r.ok) { healthy = true; break; } } catch { await sleep(500); }
      }
      if (!healthy) { setStage(1, "本地服务暂时没有响应，请查看日志。"); setError("本地服务暂时没有响应，请查看日志。"); return; }
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
    alert("日志位置：~/.coevo/logs");
  }

  if (error) return (
    <div className="boot-screen">
      <div className="boot-mark boot-mark-error">!</div>
      <div className="boot-title">coevo 启动失败</div>
      <div className="boot-error">{error}</div>
      <div className="boot-actions">
        <button onClick={() => { setError(""); boot(); }} className="boot-button">重试</button>
        <button onClick={openLogs} className="boot-button boot-button-secondary">打开日志</button>
      </div>
    </div>
  );

  return (
    <div className="boot-screen">
      <div className="boot-mark">c</div>
      <div className="boot-title">coevo 正在准备</div>
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
