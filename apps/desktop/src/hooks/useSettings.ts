import { useState, useEffect, useCallback } from "react";
import type { CoevoSettings } from "../settings/types";
import { defaults } from "../settings/defaults";

const STORAGE_KEY = "coevo-settings";

function load(): CoevoSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...defaults, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { ...defaults };
}

function save(s: CoevoSettings) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
  // Also sync api_base_url for client.ts
  localStorage.setItem("coevo-api-base", s.developer.api_base_url);
  // Sync theme
  localStorage.setItem("coevo-theme", s.appearance.theme);
  localStorage.setItem("coevo-font-size", s.appearance.font_size);
  localStorage.setItem("coevo-density", s.appearance.density);
}

export function useSettings() {
  const [settings, setSettings] = useState<CoevoSettings>(load);
  const [dirty, setDirty] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    const s = load();
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
    save(settings);
    setDirty(false);
    setSaved(true);
    applyTheme(settings);
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
