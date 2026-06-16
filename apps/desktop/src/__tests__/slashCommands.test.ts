import { describe, expect, it } from "vitest";
import { setLanguage, t } from "../settings/i18n";
import {
  SLASH_COMMANDS,
  getSlashCommandMatches,
  parseSlashCommandInput,
  resolveSlashTarget,
} from "../utils/slashCommands";

describe("slashCommands", () => {
  it("shows suggestions only for a leading slash query", () => {
    expect(getSlashCommandMatches("/").map((command) => command.name)).toContain("status");
    expect(getSlashCommandMatches("/ap").map((command) => command.name)).toContain("approve");
    expect(getSlashCommandMatches("/ap").map((command) => command.name)).not.toContain("reject");
    expect(getSlashCommandMatches("hello /ap")).toHaveLength(0);
  });

  it("parses the command name and trailing arguments", () => {
    const parsed = parseSlashCommandInput("/reject   needs more context");
    expect(parsed.active).toBe(true);
    expect(parsed.commandName).toBe("reject");
    expect(parsed.args).toBe("needs more context");
    expect(parsed.raw).toBe("/reject   needs more context");
    expect(parsed.command?.name).toBe("reject");

    const empty = parseSlashCommandInput("/");
    expect(empty.active).toBe(true);
    expect(empty.commandName).toBe("");
    expect(empty.args).toBe("");
    expect(empty.raw).toBe("/");
    expect(empty.command).toBeNull();
  });

  it("resolves go targets from route aliases", () => {
    expect(resolveSlashTarget("tasks")).toBe("/work-orders");
    expect(resolveSlashTarget("work-orders")).toBe("/work-orders");
    expect(resolveSlashTarget("plans")).toBe("/work-orders");
    expect(resolveSlashTarget("mission")).toBe("/");
    expect(resolveSlashTarget("risk")).toBe("/settings/risk_gate");
    expect(resolveSlashTarget("unknown")).toBeNull();
  });

  it("keeps localized slash copy available", () => {
    setLanguage("en");
    expect(t("slash.status_label")).toBe("Status");
    expect(t("slash.help_desc")).toBe("Show slash command help");

    setLanguage("zh");
    expect(t("slash.status_label")).toBe("状态");
    expect(t("slash.help_desc")).toBe("显示斜杠命令帮助");
  });

  it("publishes the expected command set", () => {
    expect(SLASH_COMMANDS.map((command) => command.name)).toEqual([
      "status",
      "approve",
      "reject",
      "run",
      "cancel",
      "go",
      "clear",
      "help",
    ]);
  });
});
