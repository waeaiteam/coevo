import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import SlashCommandMenu from "../components/SlashCommandMenu";
import type { SlashCommandSpec } from "../utils/slashCommands";

const commands: SlashCommandSpec[] = [
  {
    name: "status",
    labelKey: "slash.status_label",
    descKey: "slash.status_desc",
    usage: "/status",
    aliases: ["status"],
    routeTarget: null,
  },
  {
    name: "help",
    labelKey: "slash.help_label",
    descKey: "slash.help_desc",
    usage: "/help",
    aliases: ["help"],
    routeTarget: null,
  },
];

describe("SlashCommandMenu", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders the filtered menu items and forwards picks", () => {
    const onPick = vi.fn();
    const onClose = vi.fn();

    render(
      <SlashCommandMenu
        open
        query="/"
        commands={commands}
        activeIndex={0}
        onPick={onPick}
        onClose={onClose}
      />,
    );

    expect(screen.getByRole("listbox", { name: "Slash commands" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/status/i })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /\/help/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("option", { name: /\/help/i }));
    expect(onPick).toHaveBeenCalledWith(commands[1]);
    expect(onClose).not.toHaveBeenCalled();
  });
});
