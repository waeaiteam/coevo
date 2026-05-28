// TODO(v0.2): move track classification to backend MCLCompiler/RiskGate.

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
        reason: `Intent matches high-risk trigger: "${trigger}". Production/critical operations require Red Track.`,
      };
    }
  }

  for (const trigger of YELLOW_TRIGGERS) {
    if (lower.includes(trigger)) {
      return {
        track: "yellow",
        reason: `Intent matches moderate-risk trigger: "${trigger}". Internal write/notification requires Yellow Track with approval window.`,
      };
    }
  }

  return {
    track: "green",
    reason: "Low-risk read/analyze intent. Green Track auto-execution is safe.",
  };
}

export function trackLabel(track: "green" | "yellow" | "red"): string {
  return {
    green: "Green Track — read-only analysis, auto-execute",
    yellow: "Yellow Track — internal write, approval window required",
    red: "Red Track — production operation, emergency lease + MFA required",
  }[track];
}
