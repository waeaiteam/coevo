// Frontend-only preview. The server verdict is the only authority for execution.

export type InferTrackResult = {
  track: "green" | "yellow" | "red";
  reason: string;
};

const RED_TRIGGERS = [
  "production", "prod", "critical", "database mutation",
  "rollback", "payment", "delete", "p1", "emergency",
  "financial", "customer data", "drop table",
];

const YELLOW_TRIGGERS = [
  "deploy", "notification", "staging", "send",
  "create ticket", "write", "update", "changelog",
  "internal", "modify",
];

export function inferTrackFromIntent(intent: string): InferTrackResult {
  const lower = intent.toLowerCase();

  for (const trigger of RED_TRIGGERS) {
    if (lower.includes(trigger)) {
      return {
        track: "red",
        reason: `Intent matches high-risk trigger: "${trigger}". This is only a gray preview.`,
      };
    }
  }

  for (const trigger of YELLOW_TRIGGERS) {
    if (lower.includes(trigger)) {
      return {
        track: "yellow",
        reason: `Intent matches moderate-risk trigger: "${trigger}". This is only a gray preview.`,
      };
    }
  }

  return {
    track: "green",
    reason: "Low-risk read/analyze intent. This is only a gray preview.",
  };
}

export function trackLabel(track: "green" | "yellow" | "red"): string {
  return {
    green: "预估：可直接整理",
    yellow: "预估：需要确认",
    red: "预估：需要人工处理",
  }[track];
}
