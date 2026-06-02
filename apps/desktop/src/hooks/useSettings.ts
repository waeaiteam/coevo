import { createContext, createElement, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import type { CoevoSettings } from "../settings/types";
import { defaults } from "../settings/defaults";

const STORAGE_KEY = "coevo-settings";

type SettingsState = {
  settings: CoevoSettings;
  update: <K extends keyof CoevoSettings>(section: K, patch: Partial<CoevoSettings[K]>) => void;
  dirty: boolean;
  saved: boolean;
  saveNow: () => void;
  replaceAndMarkSaved: (nextSettings: CoevoSettings) => void;
  reset: () => void;
};

const SettingsContext = createContext<SettingsState | null>(null);

function mergeSettings(partial: Partial<CoevoSettings>): CoevoSettings {
  return {
    ...defaults,
    ...partial,
    general: { ...defaults.general, ...partial.general },
    appearance: { ...defaults.appearance, ...partial.appearance },
    model_provider: { ...defaults.model_provider, ...partial.model_provider },
    agent_runtime: { ...defaults.agent_runtime, ...partial.agent_runtime },
    governance: { ...defaults.governance, ...partial.governance },
    risk_gate: { ...defaults.risk_gate, ...partial.risk_gate },
    cognitive_customs: { ...defaults.cognitive_customs, ...partial.cognitive_customs },
    policy_engine: { ...defaults.policy_engine, ...partial.policy_engine },
    privacy: { ...defaults.privacy, ...partial.privacy },
    developer: { ...defaults.developer, ...partial.developer },
  };
}

export function loadSettingsSnapshot(): CoevoSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return mergeSettings(JSON.parse(raw));
  } catch { /* ignore */ }
  return mergeSettings({});
}

function save(s: CoevoSettings) {
  const snapshot: CoevoSettings = {
    ...s,
    model_provider: { ...s.model_provider, api_key: "" },
  };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  // Preserve the runtime API base that BootPage receives from the Tauri sidecar.
  // Only explicit Developer changes should override it.
  if (s.developer.api_base_url !== defaults.developer.api_base_url) {
    localStorage.setItem("coevo-api-base", s.developer.api_base_url);
  }
  // Sync theme
  localStorage.setItem("coevo-theme", s.appearance.theme);
  localStorage.setItem("coevo-font-size", s.appearance.font_size);
  localStorage.setItem("coevo-density", s.appearance.density);
}

export function saveSettingsSnapshot(s: CoevoSettings) {
  save(s);
  applyTheme(s);
}

function useSettingsController(): SettingsState {
  const [settings, setSettings] = useState<CoevoSettings>(loadSettingsSnapshot);
  const [dirty, setDirty] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    const s = loadSettingsSnapshot();
    setSettings(s);
    applyTheme(s);
  }, []);

  const update = useCallback(<K extends keyof CoevoSettings>(
    section: K,
    patch: Partial<CoevoSettings[K]>
  ) => {
    setSettings((prev) => {
      const next = { ...prev, [section]: { ...prev[section], ...patch } };
      return next;
    });
    setDirty(true);
    setSaved(false);
  }, []);

  const saveNow = useCallback(() => {
    saveSettingsSnapshot(settings);
    setDirty(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [settings]);

  const replaceAndMarkSaved = useCallback((nextSettings: CoevoSettings) => {
    saveSettingsSnapshot(nextSettings);
    setSettings(nextSettings);
    setDirty(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, []);

  const reset = useCallback(() => {
    setSettings({ ...defaults });
    setDirty(true);
    setSaved(false);
  }, []);

  return { settings, update, dirty, saved, saveNow, replaceAndMarkSaved, reset };
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const value = useSettingsController();
  return createElement(SettingsContext.Provider, { value }, children);
}

export function useSettings(): SettingsState {
  const value = useContext(SettingsContext);
  if (!value) throw new Error("useSettings must be used within SettingsProvider");
  return value;
}

function resolveSystemTheme(): "light" | "dark" {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

function applyTheme(s: CoevoSettings) {
  const root = document.documentElement;
  // Drive light/dark through the single source of truth: the data-theme attribute,
  // which globals.css keys all of its color tokens off of. Never inline hardcoded
  // hex values here — that previously fought with the palette in globals.css.
  const theme = s.appearance.theme;
  const resolved = theme === "system" ? resolveSystemTheme() : theme;
  root.dataset.theme = resolved;
  root.style.colorScheme = resolved;

  // The following are presentation knobs globals.css does not own; keep applying them.
  const sizes: Record<string, string> = { small: "12px", normal: "14px", large: "16px", "extra-large": "18px" };
  root.style.fontSize = sizes[s.appearance.font_size] || "14px";

  if (s.appearance.density === "compact") {
    root.style.setProperty("--card-padding", "8px");
    root.style.setProperty("--row-gap", "4px");
  } else {
    root.style.setProperty("--card-padding", "16px");
    root.style.setProperty("--row-gap", "8px");
  }

  root.style.setProperty("--transition-duration", s.appearance.reduce_motion ? "0s" : "150ms");

  // High-contrast is handled by a theme-aware attribute hook in globals.css, so it
  // stays legible in both light and dark rather than forcing a hardcoded black.
  if (s.appearance.high_contrast) {
    root.dataset.contrast = "high";
  } else {
    delete root.dataset.contrast;
  }
}
