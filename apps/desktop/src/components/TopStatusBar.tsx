import { useEffect, useState } from "react";
import { getHealth } from "../api/client";
import { t, useLanguage } from "../settings/i18n";

export default function TopStatusBar() {
  useLanguage();
  const [status, setStatus] = useState({ ok: false, version: "-" });

  useEffect(() => {
    let mounted = true;
    async function check() {
      try {
        const h = await getHealth();
        if (mounted) setStatus({ ok: h.status === "ok", version: h.version });
      } catch {
        if (mounted) setStatus({ ok: false, version: "-" });
      }
    }
    check();
    const timer = setInterval(check, 30000);
    return () => {
      mounted = false;
      clearInterval(timer);
    };
  }, []);

  return (
    <div className="top-status flex items-center gap-3 px-4 py-1.5 text-[11px]" data-tauri-drag-region="">
      <div className="flex-1" data-tauri-drag-region="" />
      <span className={`status-dot ${status.ok ? "online" : "offline"}`} />
      <span style={{ color: status.ok ? "var(--green)" : "var(--red)" }}>
        {status.ok ? t("top.online") : t("top.offline")}
      </span>
    </div>
  );
}
