import { describe, expect, it } from "vitest";
import { extractWorkOrderResult } from "../utils/workOrderResult";

describe("extractWorkOrderResult", () => {
  it("prefers the newest run by timeline time_ms instead of the highest event_seq", () => {
    const result = extractWorkOrderResult([
      {
        time_ms: 1000,
        type: "ContentDelta",
        details: {
          run_id: "run-older",
          event_seq: 200,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              proposal: { content: "Older proposal content." },
            }),
          }),
        },
      },
      {
        time_ms: 1100,
        type: "Usage",
        details: {
          run_id: "run-older",
          event_seq: 299,
          payload_json: JSON.stringify({
            prompt_tokens: 1900,
            completion_tokens: 221,
            total_tokens: 2121,
          }),
        },
      },
      {
        time_ms: 2000,
        type: "ContentDelta",
        details: {
          run_id: "run-latest",
          event_seq: 1,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              proposal: { message: "Latest final answer." },
            }),
          }),
        },
      },
      {
        time_ms: 2100,
        type: "Usage",
        details: {
          run_id: "run-latest",
          event_seq: 2,
          payload_json: JSON.stringify({
            prompt_tokens: 2132,
            completion_tokens: 223,
            total_tokens: 2355,
          }),
        },
      },
    ]);

    expect(result.latestRunId).toBe("run-latest");
    expect(result.finalText).toBe("Latest final answer.");
    expect(result.totalTokens).toBe(2355);
    expect(result.promptTokens).toBe(2132);
    expect(result.completionTokens).toBe(223);
  });

  it("falls back to raw joined deltas when the content is plain markdown", () => {
    const result = extractWorkOrderResult([
      {
        time_ms: 3000,
        type: "ContentDelta",
        details: {
          run_id: "run-markdown",
          event_seq: 1,
          payload_json: JSON.stringify({ delta: "# Heading\n\n" }),
        },
      },
      {
        time_ms: 3100,
        type: "ContentDelta",
        details: {
          run_id: "run-markdown",
          event_seq: 2,
          payload_json: JSON.stringify({ delta: "Plain result body." }),
        },
      },
    ]);

    expect(result.latestRunId).toBe("run-markdown");
    expect(result.finalText).toBe("# Heading\n\nPlain result body.");
  });

  it("hides structured ask_human payloads and keeps the human-readable answer", () => {
    const result = extractWorkOrderResult([
      {
        time_ms: 4000,
        type: "ContentDelta",
        details: {
          run_id: "run-ask-human",
          event_seq: 1,
          payload_json: JSON.stringify({
            delta: "{",
          }),
        },
      },
      {
        time_ms: 4010,
        type: "ContentDelta",
        details: {
          run_id: "run-ask-human",
          event_seq: 2,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              ask_human: {
                message: "Please confirm the production rollout.",
                reason: "Approval required before deployment.",
              },
            }),
          }),
        },
      },
    ]);

    expect(result.latestRunId).toBe("run-ask-human");
    expect(result.finalText).toBe("Please confirm the production rollout.");
  });

  it("extracts ask_human questions from proposal payloads instead of leaking raw JSON", () => {
    const result = extractWorkOrderResult([
      {
        time_ms: 5000,
        type: "ContentDelta",
        details: {
          run_id: "run-proposal-question",
          event_seq: 1,
          payload_json: JSON.stringify({
            delta: JSON.stringify({
              reasoning: "Mission intent is ambiguous.",
              proposal: {
                type: "ask_human",
                payload: {
                  question: "Could you clarify which bug you want fixed first?",
                },
              },
            }),
          }),
        },
      },
    ]);

    expect(result.latestRunId).toBe("run-proposal-question");
    expect(result.finalText).toBe("Could you clarify which bug you want fixed first?");
    expect(result.finalText).not.toContain("reasoning");
    expect(result.finalText).not.toContain("\"proposal\"");
  });

  it("extracts the human question from pseudo-json streamed as fragmented content deltas", () => {
    const deltas = [
      "{",
      "\"reasoning\"",
      ":",
      "\"Mission intent is ambiguous (just 'A'). Need clarification to draft a proper mission.\"",
      ",",
      "\"proposal\"",
      ":",
      "{",
      "\"type\"",
      ":",
      "\"ask_human\"",
      ",",
      "\"payload\"",
      ":",
      "{",
      "\"question\"",
      ":",
      "\"The mission intent is listed as 'A'. Could you please provide more details on what mission you'd like me to draft? For example the goal scope or any specific requirements.\"",
      "}",
      "}",
      "}",
    ];

    const result = extractWorkOrderResult(
      deltas.map((delta, index) => ({
        time_ms: 6000 + index,
        type: "ContentDelta",
        details: {
          run_id: "run-pseudo-json",
          event_seq: index + 1,
          payload_json: JSON.stringify({ delta }),
        },
      })),
    );

    expect(result.latestRunId).toBe("run-pseudo-json");
    expect(result.finalText).toBe(
      "The mission intent is listed as 'A'. Could you please provide more details on what mission you'd like me to draft? For example the goal scope or any specific requirements.",
    );
    expect(result.finalText).not.toContain("reasoning");
    expect(result.finalText).not.toContain("payload");
    expect(result.finalText).not.toContain("\"question\"");
  });

  it("hides raw internal structured stream output when no human-facing final answer exists", () => {
    const deltas = [
      "{\"thought\":\"Explore the codebase before editing.\",",
      "\"proposal\":{\"type\":\"tool_call\",\"payload\":{\"tool\":\"file-readonly\",\"path\":\"apps/server/src/router.rs\"}},",
      "\"reasoning\":\"Need to inspect the router first.\"}",
    ];

    const result = extractWorkOrderResult(
      deltas.map((delta, index) => ({
        time_ms: 7000 + index,
        type: "ContentDelta",
        details: {
          run_id: "run-internal-structured-noise",
          event_seq: index + 1,
          payload_json: JSON.stringify({ delta }),
        },
      })),
    );

    expect(result.latestRunId).toBe("run-internal-structured-noise");
    expect(result.finalText).toBe("");
  });
});
