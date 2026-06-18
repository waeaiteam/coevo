import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import CommandPalette from "../components/CommandPalette";
import { ThemeProvider } from "../hooks/useTheme";
import { setLanguage } from "../settings/i18n";
import { setAdvancedMode } from "../settings/appMode";

describe("CommandPalette", () => {
  afterEach(() => {
    cleanup();
    localStorage.clear();
    setAdvancedMode(false);
    document.documentElement.removeAttribute("data-theme");
    setLanguage("en");
  });

  it("opens with ctrl+k and keeps advanced pages discoverable in advanced mode", () => {
    setLanguage("en");
    setAdvancedMode(true);
    render(
      <MemoryRouter>
        <ThemeProvider>
          <CommandPalette />
        </ThemeProvider>
      </MemoryRouter>
    );

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "contracts" },
    });

    expect(screen.getByRole("button", { name: /^Contracts\b/i })).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "company memory" },
    });

    expect(screen.getByRole("button", { name: /Company Memory/i })).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "plans" },
    });

    expect(screen.getByText("No results")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Plans/i })).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "risk gate" },
    });

    expect(screen.getByText("No results")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Risk Gate/i })).not.toBeInTheDocument();
  });

  it("hides advanced pages in the default (non-advanced) surface", () => {
    setLanguage("en");
    setAdvancedMode(false);
    render(
      <MemoryRouter>
        <ThemeProvider>
          <CommandPalette />
        </ThemeProvider>
      </MemoryRouter>
    );

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "contracts" },
    });
    expect(screen.getByText("No results")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "team" },
    });
    expect(screen.getByRole("button", { name: /My Team/i })).toBeInTheDocument();
  });

  it("uses the theme context to switch data-theme", () => {
    setLanguage("en");
    render(
      <MemoryRouter>
        <ThemeProvider>
          <CommandPalette />
        </ThemeProvider>
      </MemoryRouter>
    );

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByPlaceholderText("Search pages or commands..."), {
      target: { value: "dark" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Dark theme/i }));

    expect(localStorage.getItem("coevo-theme-mode")).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
