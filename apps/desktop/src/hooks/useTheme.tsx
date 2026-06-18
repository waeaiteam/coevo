import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type ThemeMode = "system" | "light" | "dark";

type ThemeContextValue = {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);
const STORAGE_KEY = "coevo-theme-mode";
// useSettings persists the canonical theme under this key. Read it as a fallback so the
// two stores agree on first paint instead of fighting over document.documentElement.
const SETTINGS_THEME_KEY = "coevo-theme";

function getInitialMode(): ThemeMode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") return stored;
    // Fall back to the value the Settings page writes, so toggling theme there is
    // reflected here without a conflicting second source of truth.
    const fromSettings = localStorage.getItem(SETTINGS_THEME_KEY);
    if (fromSettings === "light" || fromSettings === "dark" || fromSettings === "system") return fromSettings;
  } catch {
    // Fall back to system.
  }
  return "system";
}

function resolveSystemTheme() {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [mode, setModeState] = useState<ThemeMode>(getInitialMode);

  useEffect(() => {
    const apply = () => {
      document.documentElement.dataset.theme = mode === "system" ? resolveSystemTheme() : mode;
    };
    apply();
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (mode === "system") media?.addEventListener("change", apply);
    return () => media?.removeEventListener("change", apply);
  }, [mode]);

  const value = useMemo<ThemeContextValue>(() => ({
    mode,
    setMode: (next) => {
      setModeState(next);
      try {
        localStorage.setItem(STORAGE_KEY, next);
        // Keep the Settings-owned key in sync so both stores resolve to the same theme.
        localStorage.setItem(SETTINGS_THEME_KEY, next);
      } catch {
        // Non-persistent theme is fine.
      }
    },
  }), [mode]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used within ThemeProvider");
  return value;
}
