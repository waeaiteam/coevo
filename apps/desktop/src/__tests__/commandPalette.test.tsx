import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import CommandPalette from "../components/CommandPalette";
import { ThemeProvider } from "../hooks/useTheme";

describe("CommandPalette", () => {
  afterEach(() => {
    cleanup();
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
  });

  it("opens with ctrl+k and supports pinyin-initial page search", () => {
    render(
      <MemoryRouter>
        <ThemeProvider>
          <CommandPalette />
        </ThemeProvider>
      </MemoryRouter>
    );

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog", { name: "命令面板" })).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索页面或输入拼音首字母..."), {
      target: { value: "gjsz" },
    });

    expect(screen.getByRole("button", { name: /高级设置/ })).toBeInTheDocument();
  });

  it("uses the theme context to switch data-theme", () => {
    render(
      <MemoryRouter>
        <ThemeProvider>
          <CommandPalette />
        </ThemeProvider>
      </MemoryRouter>
    );

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByPlaceholderText("搜索页面或输入拼音首字母..."), {
      target: { value: "ss" },
    });
    fireEvent.click(screen.getByRole("button", { name: /深色主题/ }));

    expect(localStorage.getItem("coevo-theme-mode")).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
