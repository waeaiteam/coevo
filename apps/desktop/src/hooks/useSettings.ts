import { useState, useEffect, useCallback } from "react";
import type { CoevoSettings } from "../settings/types";
import { defaults } from "../settings/defaults";

const STORAGE_KEY = "coevo-settings";

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

export function useSettings() {
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

  const reset = useCallback(() => {
    setSettings({ ...defaults });
    setDirty(true);
    setSaved(false);
  }, []);

  return { settings, update, dirty, saved, saveNow, reset };
}

function applyTheme(s: CoevoSettings) {
  const root = document.documentElement;
  const theme = s.appearance.theme;
  if (theme === "dark") {
    root.style.setProperty("--bg-primary", "#0a0a0f");
    root.style.setProperty("--bg-card", "#16161f");
    root.style.setProperty("--text-primary", "#e4e4ed");
    root.style.colorScheme = "dark";
  } else if (theme === "light") {
    root.style.setProperty("--bg-primary", "#fafafa");
    root.style.setProperty("--bg-card", "#ffffff");
    root.style.setProperty("--text-primary", "#1a1a2e");
    root.style.colorScheme = "light";
  }

  const sizes: Record<string, string> = { small: "12px", normal: "14px", large: "16px", "extra-large": "18px" };
  root.style.fontSize = sizes[s.appearance.font_size] || "14px";

  if (s.appearance.density === "compact") {
    root.style.setProperty("--card-padding", "8px");
    root.style.setProperty("--row-gap", "4px");
  } else {
    root.style.setProperty("--card-padding", "16px");
    root.style.setProperty("--row-gap", "8px");
  }

  if (s.appearance.reduce_motion) {
    root.style.setProperty("--transition-duration", "0s");
  } else {
    root.style.setProperty("--transition-duration", "150ms");
  }

  if (s.appearance.high_contrast) {
    root.style.setProperty("--text-secondary", "#000000");
    root.style.setProperty("--text-muted", "#333333");
  }
}
