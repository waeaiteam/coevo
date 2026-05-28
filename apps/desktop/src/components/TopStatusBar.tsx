import { useEffect, useState } from "react";
import { getHealth } from "../api/client";

export default function TopStatusBar() {
  const [status, setStatus] = useState<{ ok: boolean; version: string; latency: number }>({
    ok: false,
    version: "—",
    latency: 0,
  });

  useEffect(() => {
    let mounted = true;
    async function check() {
      const start = Date.now();
      try {
        const h = await getHealth();
        if (mounted) setStatus({ ok: h.status === "ok", version: h.version, latency: Date.now() - start });
      } catch {
        if (mounted) setStatus({ ok: false, version: "—", latency: 0 });
      }
    }
    check();
    const iv = setInterval(check, 15000);
    return () => { mounted = false; clearInterval(iv); };
  }, []);

  return (
    <div className="flex items-center gap-6 px-5 py-2.5 border-b text-xs tracking-wide" style={{ background: "#0d0d16", borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
      <div className="flex items-center gap-2">
        <span className={`status-dot ${status.ok ? "online pulse" : "offline"}`} />
        <span style={{ color: status.ok ? "var(--green)" : "var(--red)" }}>
          {status.ok ? "Control Plane Online" : "Control Plane Offline"}
        </span>
      </div>
      <span style={{ color: "var(--border-accent)" }}>|</span>
      <span>Server: <span style={{ color: "var(--text-primary)" }}>127.0.0.1:8717</span></span>
      <span style={{ color: "var(--border-accent)" }}>|</span>
      <span>Version: <span style={{ color: "var(--text-primary)" }}>{status.version}</span></span>
      <span style={{ color: "var(--border-accent)" }}>|</span>
      <span>Latency: <span style={{ color: status.latency < 100 ? "var(--green)" : "var(--yellow)" }}>{status.latency}ms</span></span>
      <span style={{ color: "var(--border-accent)" }}>|</span>
      <span>Last Sync: <span style={{ color: "var(--text-primary)" }}>{new Date().toLocaleTimeString()}</span></span>
      <div className="flex-1" />
      <span style={{ color: "var(--text-muted)" }}>{new Date().toISOString().slice(0, 19).replace("T", " ")} UTC</span>
    </div>
  );
}
