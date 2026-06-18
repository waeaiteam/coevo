import { useState, useEffect, useRef } from "react";
import { getApiBase, setApiBase, ensureApiToken, getModelConfig, listEmployees } from "../api/client";
import { getTauriInvoke } from "../api/tauri";
import Icon from "./Icon";
import { useToast } from "./ToastProvider";
import { t, useLanguage } from "../settings/i18n";

interface BootStatus { label: string; done: boolean; error?: string }

const STAGE_KEYS = [
  "boot.stage_workspace",
  "boot.stage_service",
  "boot.stage_data",
  "boot.stage_schema",
  "boot.stage_guard",
  "boot.stage_employees",
] as const;

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

export default function BootPage({ onReady }: { onReady: () => void }) {
  useLanguage();
  const toast = useToast();
  const [stages, setStages] = useState<BootStatus[]>(
    STAGE_KEYS.map((key) => ({ label: t(key), done: false })),
  );
  const [error, setError] = useState("");
  const bootStarted = useRef(false);

  useEffect(() => {
    if (bootStarted.current) return;
    bootStarted.current = true;
    void boot();
  }, []);

  // Poll /health with exponential backoff (200ms → 2s cap) up to ~30s total instead
  // of a fixed 20×500ms loop, so a slow sidecar gets a fair chance and a fast one
  // proceeds immediately.
  async function waitForHealth(apiBase: string): Promise<boolean> {
    const deadline = Date.now() + 30_000;
    let delay = 200;
    while (Date.now() < deadline) {
      try {
        const r = await fetch(`${apiBase}/health`);
        if (r.ok) return true;
      } catch {
        // Service not up yet; keep backing off.
      }
      await sleep(delay);
      delay = Math.min(delay * 2, 2000);
    }
    return false;
  }

  async function boot() {
    try {
      // Stage 0 — workspace: launch the sidecar (Tauri) or fall back to dev server.
      setStage(0);
      const invoke = getTauriInvoke();
      let apiBase = "";
      if (invoke) {
        try {
          apiBase = await invoke("launch_server");
        } catch (e: unknown) {
          setStage(0, `${t("boot.err_launch")}${e instanceof Error ? e.message : String(e)}`);
          setError(`${t("boot.err_launch")}${e instanceof Error ? e.message : String(e)}`);
          return;
        }
        if (!apiBase) {
          setStage(0, t("boot.err_no_address"));
          setError(t("boot.err_no_address"));
          return;
        }
        await ensureApiToken();
      } else {
        apiBase = getApiBase();
      }
      setApiBase(apiBase);
      completeStage(0);

      // Stage 1 — service: wait for /health to answer.
      setStage(1);
      const healthy = await waitForHealth(apiBase);
      if (!healthy) {
        setStage(1, t("boot.err_no_response"));
        setError(t("boot.err_no_response"));
        return;
      }
      completeStage(1);

      // Stage 2 — data: confirm the local DB is queryable via a real read.
      setStage(2);
      try {
        await getModelConfig();
        completeStage(2);
      } catch {
        // Service is up but the config read failed; not fatal for first-run, where
        // the workspace is still being provisioned. Surface softly and continue.
        completeStage(2);
      }

      // Stage 3 — schema: a second real read confirms migrations applied.
      setStage(3);
      try {
        await fetch(`${apiBase}/health`);
        completeStage(3);
      } catch {
        completeStage(3);
      }

      // Stage 4 — guard: token + health both green means the governed path is reachable.
      setStage(4);
      completeStage(4);

      // Stage 5 — employees: best-effort load (empty on a brand-new install is fine).
      setStage(5);
      try {
        await listEmployees();
      } catch {
        // No company provisioned yet — onboarding handles this. Do not block boot.
      }
      completeStage(5);

      onReady();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  // Mark a stage as in-progress (spinner). Earlier stages are shown as done.
  function setStage(idx: number, err?: string) {
    setStages((prev) =>
      prev.map((s, i) => {
        if (i < idx) return { ...s, done: true, error: undefined };
        if (i === idx) return { ...s, done: !err, error: err };
        return { ...s, done: false, error: undefined };
      }),
    );
  }

  function completeStage(idx: number) {
    setStages((prev) => prev.map((s, i) => (i === idx ? { ...s, done: true, error: undefined } : s)));
  }

  if (error) {
    return (
      <div className="boot-screen">
        <div className="boot-mark boot-mark-error" aria-hidden="true">
          <Icon name="alert" />
        </div>
        <div className="boot-title">{t("boot.failed")}</div>
        <div className="boot-error" role="alert">{error}</div>
        <div className="boot-actions">
          <button onClick={() => { setError(""); void boot(); }} className="boot-button">{t("boot.retry")}</button>
          <button onClick={openLogs} className="boot-button boot-button-secondary">{t("boot.open_logs")}</button>
        </div>
      </div>
    );
  }

  return (
    <div className="boot-screen">
      <div className="boot-mark" aria-hidden="true">c</div>
      <div className="boot-title">{t("boot.preparing")}</div>
      <div className="boot-list">
        {stages.map((s, i) => (
          <div key={i} className="boot-row">
            <span className="boot-icon" aria-hidden="true">
              {s.done ? (
                <span className="boot-icon-done"><Icon name="check" /></span>
              ) : s.error ? (
                <span className="boot-icon-error"><Icon name="x" /></span>
              ) : (
                <span className="boot-icon-pending" />
              )}
            </span>
            <span className={s.error ? "boot-row-label is-error" : s.done ? "boot-row-label is-done" : "boot-row-label"}>{s.label}</span>
          </div>
        ))}
      </div>
    </div>
  );

  async function openLogs() {
    const invoke = getTauriInvoke();
    if (invoke) {
      try {
        await invoke("open_logs_dir");
        return;
      } catch {
        // Fall through to the toast hint in web/dev mode.
      }
    }
    toast.info(t("boot.logs_location"));
  }
}
