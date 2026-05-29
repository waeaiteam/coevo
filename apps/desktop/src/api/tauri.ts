type TauriInvoke = <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>;

type TauriGlobal = {
  core?: {
    invoke?: TauriInvoke;
  };
};

declare global {
  interface Window {
    __TAURI__?: TauriGlobal;
  }
}

export function getTauriInvoke(): TauriInvoke | null {
  const invoke = window.__TAURI__?.core?.invoke;
  return typeof invoke === "function" ? invoke.bind(window.__TAURI__?.core) : null;
}
