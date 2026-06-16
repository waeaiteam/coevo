type Row = Record<string, unknown>;

function parseJson(value: unknown): unknown {
  if (typeof value !== "string" || !value.trim()) return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function detailsOf(row: Row): Row {
  return row.details && typeof row.details === "object" ? (row.details as Row) : {};
}

function numberField(row: Row, key: string): number {
  const value = Number(row[key] || 0);
  return Number.isFinite(value) ? value : 0;
}

function stringField(row: Row, key: string): string {
  const value = row[key];
  return value == null ? "" : String(value);
}

function isStructuredHumanPayload(value: unknown): value is Row {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function extractHumanReadableText(value: unknown): string {
  if (typeof value === "string") {
    if (isStructuralNoiseChunk(value)) return "";
    const parsed = parseJson(value);
    if (parsed !== value) return extractHumanReadableText(parsed);
    return value;
  }
  if (!isStructuredHumanPayload(value)) return "";

  const row = value as Row;
  if (isInternalStructuredPayload(row)) return "";
  const askHuman = isStructuredHumanPayload(row.ask_human) ? (row.ask_human as Row) : {};
  const proposal = isStructuredHumanPayload(row.proposal) ? (row.proposal as Row) : {};
  const askHumanPayload = isStructuredHumanPayload(askHuman.payload) ? (askHuman.payload as Row) : {};
  const proposalPayload = isStructuredHumanPayload(proposal.payload) ? (proposal.payload as Row) : {};
  const candidates = [
    stringField(askHumanPayload, "question"),
    stringField(askHuman, "message"),
    stringField(askHuman, "content"),
    stringField(askHuman, "summary"),
    stringField(proposalPayload, "question"),
    stringField(proposalPayload, "prompt"),
    stringField(proposal, "message"),
    stringField(proposal, "content"),
    stringField(row, "message"),
    stringField(row, "content"),
    stringField(row, "summary"),
  ];
  return candidates.find((candidate) => candidate.trim().length > 0)?.trim() || "";
}

function isInternalStructuredPayload(row: Row): boolean {
  const hasInternalThought = stringField(row, "thought").trim().length > 0
    || stringField(row, "reasoning").trim().length > 0;
  if (!hasInternalThought) return false;

  const proposal = isStructuredHumanPayload(row.proposal) ? (row.proposal as Row) : {};
  const proposalType = stringField(proposal, "type").trim().toLowerCase();
  if (proposalType === "tool_call") return true;

  const toolCall = isStructuredHumanPayload(row.tool_call) ? (row.tool_call as Row) : {};
  if (stringField(toolCall, "tool").trim()) return true;

  const proposalPayload = isStructuredHumanPayload(proposal.payload) ? (proposal.payload as Row) : {};
  return stringField(proposalPayload, "tool").trim().length > 0
    || stringField(proposalPayload, "path").trim().length > 0;
}

function isStructuralNoiseChunk(text: string): boolean {
  const trimmed = text.trim();
  return Boolean(trimmed) && /^[{}\[\],:]$/.test(trimmed);
}

function timeField(row: Row): number {
  const direct = Number(row.time_ms || 0);
  if (Number.isFinite(direct) && direct > 0) return direct;
  const details = detailsOf(row);
  const fallback = Number(details.time_ms || details.created_at_ms || 0);
  return Number.isFinite(fallback) ? fallback : 0;
}

function extractStructuredFinalText(rawText: string): string {
  const trimmed = rawText.trim();
  if (!trimmed) return "";
  const parsed = parseJson(trimmed);
  if (parsed !== trimmed) {
    if (typeof parsed === "string") return parsed.trim();
    const humanText = extractHumanReadableText(parsed);
    if (humanText) return humanText;
    if (isStructuredHumanPayload(parsed)) return "";
  }

  const pseudoJsonQuestion = extractPseudoJsonValue(trimmed, "question")
    || extractPseudoJsonValue(trimmed, "message")
    || extractPseudoJsonValue(trimmed, "prompt")
    || extractPseudoJsonValue(trimmed, "content")
    || extractPseudoJsonValue(trimmed, "summary");
  return pseudoJsonQuestion || trimmed;
}

function extractPseudoJsonValue(text: string, key: string): string {
  const quotedKeyPattern = new RegExp(`"${escapeRegExp(key)}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`);
  const bareKeyPattern = new RegExp(`${escapeRegExp(key)}\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`);
  const match = text.match(quotedKeyPattern) || text.match(bareKeyPattern);
  if (!match?.[1]) return "";
  return decodePseudoJsonString(match[1]).trim();
}

function decodePseudoJsonString(value: string): string {
  try {
    return JSON.parse(`"${value}"`) as string;
  } catch {
    return value.replace(/\\"/g, "\"").replace(/\\\\/g, "\\");
  }
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export type WorkOrderResultSummary = {
  finalText: string;
  latestRunId: string;
  eventCount: number;
  totalTokens: number;
  promptTokens: number;
  completionTokens: number;
};

export function extractWorkOrderResult(rows: Row[]): WorkOrderResultSummary {
  const items = Array.isArray(rows) ? rows : [];
  let latestRunId = "";
  let latestTime = -1;

  for (const row of items) {
    const details = detailsOf(row);
    const runId = stringField(details, "run_id");
    if (!runId) continue;
    const time = timeField(row);
    const seq = numberField(details, "event_seq");
    if (time > latestTime || (time === latestTime && seq >= 0 && runId !== latestRunId)) {
      latestTime = time;
      latestRunId = runId;
    }
  }

  const scoped = latestRunId
    ? items.filter((row) => stringField(detailsOf(row), "run_id") === latestRunId)
    : items;

  const rawFinalText = scoped
    .filter((row) => stringField(row, "type") === "ContentDelta")
    .sort((left, right) => numberField(detailsOf(left), "event_seq") - numberField(detailsOf(right), "event_seq"))
    .map((row) => {
      const payload = parseJson(detailsOf(row).payload_json) as Row;
      return (
        stringField(payload || {}, "delta")
        || stringField(payload || {}, "output")
        || stringField(payload || {}, "text")
      );
    })
    .join("")
    .trim();
  const finalText = extractStructuredFinalText(rawFinalText);

  const usageRow = [...scoped]
    .filter((row) => stringField(row, "type") === "Usage")
    .sort((left, right) => numberField(detailsOf(right), "event_seq") - numberField(detailsOf(left), "event_seq"))[0];

  const usage = usageRow ? (parseJson(detailsOf(usageRow).payload_json) as Row) : {};

  return {
    finalText,
    latestRunId,
    eventCount: scoped.length,
    totalTokens: numberField(usage || {}, "total_tokens"),
    promptTokens: numberField(usage || {}, "prompt_tokens"),
    completionTokens: numberField(usage || {}, "completion_tokens"),
  };
}
