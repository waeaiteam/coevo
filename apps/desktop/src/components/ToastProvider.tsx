import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import Icon from "./Icon";

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

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  const show = useCallback(
    (message: string, kind: ToastKind = "info") => {
      const text = String(message || "").trim();
      if (!text) return "";
      const id = crypto.randomUUID();
      setToasts((prev) => [...prev, { id, kind, message: text }]);
      const timer = setTimeout(() => dismiss(id), AUTO_DISMISS_MS);
      timersRef.current.set(id, timer);
      return id;
    },
    [dismiss],
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
      <div className="toast-stack" role="region" aria-live="polite" aria-label="Notifications">
        {toasts.map((toast) => (
          <div key={toast.id} className={`toast toast-${toast.kind}`} role="status">
            <span className="toast-icon">
              <Icon name={iconForKind[toast.kind]} />
            </span>
            <span className="toast-message">{toast.message}</span>
            <button
              type="button"
              className="toast-close"
              aria-label="Dismiss notification"
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
