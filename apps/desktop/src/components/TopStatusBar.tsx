import { useEffect, useState } from "react";
import { getHealth } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

export default function TopStatusBar() {
  useLanguage();
  const [status, setStatus] = useState({ ok: false, version: "-", latency: 0 });
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    let mounted = true;
    async function check() {
      const start = Date.now();
      try {
        const h = await getHealth();
        if (mounted) setStatus({ ok: h.status === "ok", version: h.version, latency: Date.now() - start });
      } catch {
        if (mounted) setStatus({ ok: false, version: "-", latency: 0 });
      }
    }
    check();
    const healthTimer = setInterval(check, 15000);
    const clockTimer = setInterval(() => setNow(new Date()), 1000);
    return () => {
      mounted = false;
      clearInterval(healthTimer);
      clearInterval(clockTimer);
    };
  }, []);

  return (
    <div className="flex min-h-10 items-center gap-4 border-b px-5 py-2 text-xs" style={{ background: "#fff", borderColor: "var(--border-subtle)", color: "var(--text-muted)" }}>
      <div className="flex items-center gap-1.5">
        <span className={`status-dot ${status.ok ? "online pulse" : "offline"}`} />
        <span style={{ color: status.ok ? "var(--green)" : "var(--red)", fontWeight: 600 }}>
          {status.ok ? t("top.online") : t("top.offline")}
        </span>
      </div>
      <span className="hidden sm:inline">{t("top.local_runtime")}</span>
      <span>v{status.version}</span>
      <span>{status.latency}ms</span>
      <div className="flex-1" />
      <span>{now.toLocaleTimeString()}</span>
    </div>
  );
}
