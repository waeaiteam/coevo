// Global, reactive "advanced mode" flag.
//
// The full settings blob (useSettings/SettingsProvider) is only mounted inside the
// Settings page, so components like Sidebar, MissionChat, and the Composer cannot read
// it through context. This standalone store mirrors the proven i18n store pattern
// (useSyncExternalStore + a localStorage-backed value + listener set) so any component
// can read and react to the flag without a provider.
//
// Default is OFF: ordinary founders see a calm, jargon-free surface. Turning it on
// reveals the full operator console (timeline, audit, traces, executors, etc.).

const STORAGE_KEY = "coevo-advanced-mode";

const listeners = new Set<() => void>();

function readInitial(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

let advancedMode = readInitial();

export function getAdvancedMode(): boolean {
  return advancedMode;
}

export function setAdvancedMode(next: boolean) {
  advancedMode = next;
  try {
    localStorage.setItem(STORAGE_KEY, next ? "true" : "false");
  } catch {
    // Non-persistent is acceptable; the in-memory value still drives the UI.
  }
  listeners.forEach((listener) => listener());
}

export function subscribeAdvancedMode(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
