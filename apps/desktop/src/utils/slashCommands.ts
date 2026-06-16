import { t } from "../settings/i18n";

export type SlashCommandName =
  | "status"
  | "approve"
  | "reject"
  | "run"
  | "cancel"
  | "go"
  | "clear"
  | "help";

export type SlashCommandSpec = {
  name: SlashCommandName;
  labelKey: string;
  descKey: string;
  usage: string;
  aliases: string[];
  routeTarget: string | null;
};

const ROUTE_TARGETS: Record<string, string> = {
  mission: "/",
  chat: "/",
  home: "/",
  root: "/",
  new: "/",
  tasks: "/work-orders",
  task: "/work-orders",
  workorders: "/work-orders",
  "work-orders": "/work-orders",
  company: "/company",
  opc: "/company",
  projects: "/projects",
  project: "/projects",
  timeline: "/timeline",
  settings: "/settings/general",
  dashboard: "/dashboard",
  audit: "/audit",
  founder: "/founder",
  memory: "/memory",
  skills: "/skills",
  executors: "/executors",
  contracts: "/contracts",
  plans: "/work-orders",
  customs: "/customs",
  risk: "/settings/risk_gate",
  resolution: "/resolution",
  evaluations: "/evaluations",
  traces: "/traces",
  workflows: "/workflows",
  performance: "/performance",
  employees: "/employees",
  market: "/market",
};

export const SLASH_COMMANDS: SlashCommandSpec[] = [
  {
    name: "status",
    labelKey: "slash.status_label",
    descKey: "slash.status_desc",
    usage: "/status",
    aliases: ["status"],
    routeTarget: null,
  },
  {
    name: "approve",
    labelKey: "slash.approve_label",
    descKey: "slash.approve_desc",
    usage: "/approve [comment]",
    aliases: ["approve", "yes"],
    routeTarget: null,
  },
  {
    name: "reject",
    labelKey: "slash.reject_label",
    descKey: "slash.reject_desc",
    usage: "/reject [comment]",
    aliases: ["reject", "deny", "no"],
    routeTarget: null,
  },
  {
    name: "run",
    labelKey: "slash.run_label",
    descKey: "slash.run_desc",
    usage: "/run",
    aliases: ["run", "execute", "start"],
    routeTarget: null,
  },
  {
    name: "cancel",
    labelKey: "slash.cancel_label",
    descKey: "slash.cancel_desc",
    usage: "/cancel",
    aliases: ["cancel", "stop"],
    routeTarget: null,
  },
  {
    name: "go",
    labelKey: "slash.go_label",
    descKey: "slash.go_desc",
    usage: "/go <page>",
    aliases: ["go", "open", "jump"],
    routeTarget: null,
  },
  {
    name: "clear",
    labelKey: "slash.clear_label",
    descKey: "slash.clear_desc",
    usage: "/clear",
    aliases: ["clear", "reset", "new"],
    routeTarget: null,
  },
  {
    name: "help",
    labelKey: "slash.help_label",
    descKey: "slash.help_desc",
    usage: "/help",
    aliases: ["help", "?"],
    routeTarget: null,
  },
];

export function isSlashCommandInput(value: string): boolean {
  return value.trimStart().startsWith("/");
}

export function parseSlashCommandInput(value: string) {
  const raw = value;
  const active = isSlashCommandInput(value);
  if (!active) {
    return {
      active,
      raw,
      commandName: "",
      args: "",
      command: null as SlashCommandSpec | null,
    };
  }

  const trimmed = value.trimStart();
  const body = trimmed.slice(1);
  if (!body.trim()) {
    return {
      active,
      raw,
      commandName: "",
      args: "",
      command: null as SlashCommandSpec | null,
    };
  }

  const parts = body.trimStart().split(/\s+/, 2);
  const commandName = String(parts[0] || "").toLowerCase();
  const argsStart = body.trimStart().slice(parts[0].length).trimStart();
  const command = findSlashCommand(commandName);

  return {
    active,
    raw,
    commandName,
    args: argsStart,
    command,
  };
}

export function getSlashCommandMatches(value: string): SlashCommandSpec[] {
  if (!isSlashCommandInput(value)) return [];
  const parsed = parseSlashCommandInput(value);
  const query = parsed.commandName.toLowerCase();
  if (!query) return [...SLASH_COMMANDS];
  return SLASH_COMMANDS.filter((command) => {
    const haystack = [command.name, command.usage, ...command.aliases].join(" ").toLowerCase();
    return haystack.includes(query);
  });
}

export function resolveSlashTarget(target: string): string | null {
  const normalized = target.trim().toLowerCase();
  if (!normalized) return null;
  if (normalized in ROUTE_TARGETS) return ROUTE_TARGETS[normalized];
  return null;
}

export function findSlashCommand(name: string): SlashCommandSpec | null {
  const normalized = name.trim().toLowerCase();
  if (!normalized) return null;
  return SLASH_COMMANDS.find((command) => {
    if (command.name === normalized) return true;
    return command.aliases.some((alias) => alias.toLowerCase() === normalized);
  }) || null;
}

export function buildSlashHelpMessage(): string {
  const lines = [
    t("slash.help_title"),
    t("slash.help_body"),
    "",
    ...SLASH_COMMANDS.map((command) => `${command.usage} - ${t(command.descKey)}`),
  ];
  return lines.join("\n");
}
