import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import Icon from "./Icon";
import { t } from "../settings/i18n";

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: string;
  kind: ToastKind;
  message: string;
}

export interface ToastApi {
  show: (message: string, kind?: ToastKind) => string;
  success: (message: string) => string;
  error: (message: string) => string;
  info: (message: string) => string;
  dismiss: (id: string) => void;
}

const AUTO_DISMISS_MS = 5000;
// Errors stay until dismissed (auto-dismiss = 0). Cap the visible stack so a burst of
// failures can't bury the screen.
const MAX_VISIBLE = 5;

// A no-op fallback so components can call useToast() outside a ToastProvider
// (e.g. isolated unit tests) without crashing. Real UI is provided by the
// provider below.
const noopApi: ToastApi = {
  show: () => "",
  success: () => "",
  error: () => "",
  info: () => "",
  dismiss: () => undefined,
};

const ToastContext = createContext<ToastApi | null>(null);

export function useToast(): ToastApi {
  return useContext(ToastContext) ?? noopApi;
}

const iconForKind: Record<ToastKind, "check" | "alert" | "info"> = {
  success: "check",
  error: "alert",
  info: "info",
};

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const clearTimer = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
    clearTimer(id);
  }, [clearTimer]);

  const scheduleDismiss = useCallback((id: string, kind: ToastKind) => {
    // Errors require an explicit close so the user can read and act on them.
    if (kind === "error") return;
    clearTimer(id);
    const timer = setTimeout(() => dismiss(id), AUTO_DISMISS_MS);
    timersRef.current.set(id, timer);
  }, [clearTimer, dismiss]);

  const show = useCallback(
    (message: string, kind: ToastKind = "info") => {
      const text = String(message || "").trim();
      if (!text) return "";
      const id = crypto.randomUUID();
      setToasts((prev) => {
        const next = [...prev, { id, kind, message: text }];
        // Trim oldest beyond the cap (and clear their timers).
        while (next.length > MAX_VISIBLE) {
          const removed = next.shift();
          if (removed) clearTimer(removed.id);
        }
        return next;
      });
      scheduleDismiss(id, kind);
      return id;
    },
    [scheduleDismiss, clearTimer],
  );

  const api = useMemo<ToastApi>(
    () => ({
      show,
      success: (message: string) => show(message, "success"),
      error: (message: string) => show(message, "error"),
      info: (message: string) => show(message, "info"),
      dismiss,
    }),
    [show, dismiss],
  );

  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      timers.forEach((timer) => clearTimeout(timer));
      timers.clear();
    };
  }, []);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toast-stack" role="region" aria-live="polite" aria-label={t("toast.region_label")}>
        {toasts.map((toast) => (
          <div
            key={toast.id}
            className={`toast toast-${toast.kind}`}
            role={toast.kind === "error" ? "alert" : "status"}
            onMouseEnter={() => clearTimer(toast.id)}
            onMouseLeave={() => scheduleDismiss(toast.id, toast.kind)}
          >
            <span className="toast-icon">
              <Icon name={iconForKind[toast.kind]} />
            </span>
            <span className="toast-message">{toast.message}</span>
            <button
              type="button"
              className="toast-close"
              aria-label={t("toast.dismiss")}
              onClick={() => dismiss(toast.id)}
            >
              <Icon name="x" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}
