import { useState, useRef, useEffect } from "react";
import { compileContract, routePlan, runDemo } from "../api/client";
import { useGovernance } from "../hooks/useGovernance";

interface ChatMessage {
  id: string;
  role: "user" | "system";
  content: string;
  detail?: string;
  track?: "green" | "yellow" | "red";
}

interface DraftReview {
  contractHash: string;
  planHash: string;
  contract: Record<string, unknown>;
  agents: string[];
  track: "green" | "yellow" | "red";
  approvalMode: string;
  actionModes: string[];
  riskSummary: string;
}

const EXAMPLES = [
  "Analyze production database anomalies and generate a remediation plan",
  "Review whether this PR is safe to merge",
  "Coordinate multiple agents to generate a daily market report",
  "Check if any agent exceeded its data access boundaries",
];

export default function MissionChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [draft, setDraft] = useState<DraftReview | null>(null);
  const { state: govState, set: setGov, reset: resetGov } = useGovernance();
  const chatEnd = useRef<HTMLDivElement>(null);

  useEffect(() => { chatEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [messages, draft]);

  async function handleSend(intent?: string) {
    const text = (intent ?? input).trim();
    if (!text || loading) return;
    setInput("");
    setLoading(true);
    setDraft(null);
    resetGov();

    const userMsg: ChatMessage = { id: crypto.randomUUID(), role: "user", content: text };
    setMessages((prev) => [...prev, userMsg]);

    try {
      // Step 1: Compile
      appendSystem("Compiling MCL contract from intent...");
      const compileRes = await compileContract(text, "DRAFT");
      const c = compileRes.contract as Record<string, unknown>;
      const hp = (c.human_approval_policy as Record<string,unknown>) || {};
      const actionModes = (c.allowed_action_modes as string[]) || [];
      appendSystem(
        "MCL contract compiled — Review below",
        `hash: ${compileRes.contract_hash.slice(0, 16)}... | ambiguity: ${compileRes.ambiguity_score.toFixed(2)} | approval: ${hp.approval_mode || "NEGATIVE_CONSENT"}`
      );

      // Step 2: Route
      appendSystem("Querying Agent Registry and computing execution plan...");
      const agents = ["agent-synthesizer-01", "agent-critic-01"];
      const routeRes = await routePlan(compileRes.contract, agents);
      appendSystem(
        "Execution plan generated",
        `plan: ${(routeRes as {plan_hash:string}).plan_hash.slice(0, 16)}... | agents: ${agents.join(", ")}`
      );

      // Step 3: Classify track
      const track = text.toLowerCase().includes("production") || text.toLowerCase().includes("critical") ? "red" as const
        : text.toLowerCase().includes("deploy") || text.toLowerCase().includes("notification") ? "yellow" as const
        : "green" as const;

      const riskSummary = {
        green: "Low risk — read/analyze only. Auto-execute, no human approval required.",
        yellow: "Moderate risk — internal write/notification. Requires approval window (NEGATIVE_CONSENT or EXPLICIT_APPROVAL).",
        red: "High risk — production write. Emergency lease with MFA dual-sign required.",
      }[track];

      appendSystem(
        `RiskGate classified as ${track.toUpperCase()} Track`,
        riskSummary,
        track
      );

      appendSystem(
        "coevo never creates unconstrained agents. It selects from the Agent Registry. A short-lived Task Agent Instance will be spawned with minimal permissions — write Hypothesis/Suggestion only, never Fact/Decision directly."
      );

      // Set draft for review
      const dr: DraftReview = {
        contractHash: compileRes.contract_hash,
        planHash: (routeRes as {plan_hash:string}).plan_hash,
        contract: c,
        agents,
        track,
        approvalMode: (hp.approval_mode as string) || "NEGATIVE_CONSENT",
        actionModes,
        riskSummary,
      };
      setDraft(dr);

      // Update governance panel (review phase)
      setGov({
        phase: "review",
        track,
        contractHash: dr.contractHash,
        planHash: dr.planHash,
        contract: dr.contract,
        agents,
        riskDecision: "pending_human_choice",
        approvalMode: dr.approvalMode,
        actionModes: dr.actionModes,
        approvalRequired: track === "yellow",
        traceparent: "",
      });
    } catch (e: unknown) {
      appendSystem(`Error: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }

  async function handleExecute(track: "green" | "yellow" | "red") {
    if (!draft) return;
    setLoading(true);
    setDraft(null);
    setGov({ ...govState!, phase: "executing" });

    try {
      appendSystem(`Executing ${track.toUpperCase()} Track per human decision...`);
      const demoRes = await runDemo(track);
      appendSystem(
        `${track.toUpperCase()} Track completed`,
        `contract: ${demoRes.contract_hash.slice(0, 12)}... | plan: ${demoRes.plan_hash.slice(0, 12)}... | entries: ${demoRes.entries_created.length} | ${demoRes.elapsed_ms}ms`,
        track
      );
      setGov({
        ...govState!,
        phase: "done",
        contractHash: demoRes.contract_hash,
        planHash: demoRes.plan_hash,
        traceparent: demoRes.traceparent,
      });
      appendSystem(
        "coevo Governance Mesh completed. Human responsibility is anchored via ADR-A. Audit trail preserved."
      );
    } catch (e: unknown) {
      appendSystem(`Execution error: ${e instanceof Error ? e.message : String(e)}`);
      setGov({ ...govState!, phase: "review" });
    } finally {
      setLoading(false);
    }
  }

  function handleCancel() {
    setDraft(null);
    resetGov();
    appendSystem("Mission cancelled by user. No actions executed.");
  }

  function appendSystem(content: string, detail?: string, track?: "green" | "yellow" | "red") {
    setMessages((prev) => [...prev, { id: crypto.randomUUID(), role: "system", content, detail, track }]);
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-5 py-3 border-b" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">Mission Review</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>
          coevo Agent Governance Mesh — Review before execution
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {messages.length === 0 && !draft && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-3xl mb-3" style={{ color: "var(--accent)" }}>◈</div>
            <h1 className="text-xl font-bold mb-1">你想让 coevo 治理什么任务？</h1>
            <p className="text-sm mb-6" style={{ color: "var(--text-muted)" }}>
              Describe your mission. coevo will compile a draft for your review before execution.
            </p>
            <div className="grid grid-cols-2 gap-2 max-w-lg">
              {EXAMPLES.map((ex) => (
                <button key={ex} onClick={() => handleSend(ex)} disabled={loading}
                  className="text-left text-xs p-3 rounded-lg border transition-colors hover:border-indigo-300"
                  style={{ borderColor: "var(--border-subtle)", background: "#fff", color: "var(--text-secondary)" }}>
                  {ex}
                </button>
              ))}
            </div>
          </div>
        )}

        {messages.map((m) => (
          <div key={m.id} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
            <div className={`chat-msg ${m.role}`}>
              <div className="flex items-center gap-2 mb-0.5">
                <span className="text-xs font-semibold" style={{ color: m.role === "user" ? "var(--accent)" : "var(--text-muted)" }}>
                  {m.role === "user" ? "You" : "coevo"}
                </span>
                {m.track && <span className={`track-${m.track}`}>{m.track}</span>}
              </div>
              <div style={{ color: m.role === "user" ? "var(--text-primary)" : "var(--text-secondary)" }}>{m.content}</div>
              {m.detail && <div className="text-xs mt-1 font-mono" style={{ color: "var(--text-muted)" }}>{m.detail}</div>}
            </div>
          </div>
        ))}

        {/* Review Card */}
        {draft && (
          <div className="flex justify-start">
            <div className="chat-msg system w-full max-w-full">
              <div className="text-xs font-semibold mb-3" style={{ color: "var(--accent)" }}>
                ⚡ Mission Draft Ready — Review before execution
              </div>
              <div className="grid grid-cols-2 gap-2 text-xs mb-3">
                <div className="p-2 rounded" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Contract:</span>
                  <span className="font-mono ml-1" style={{ color: "var(--accent)" }}>{draft.contractHash.slice(0,14)}...</span>
                </div>
                <div className="p-2 rounded" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Plan:</span>
                  <span className="font-mono ml-1" style={{ color: "var(--accent)" }}>{draft.planHash.slice(0,14)}...</span>
                </div>
                <div className="p-2 rounded" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Track:</span>
                  <span className={`track-${draft.track} ml-1`}>{draft.track.toUpperCase()}</span>
                </div>
                <div className="p-2 rounded" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Approval:</span>
                  <span className="font-mono ml-1" style={{ color: "var(--text-secondary)" }}>{draft.approvalMode}</span>
                </div>
                <div className="p-2 rounded col-span-2" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Agents:</span>
                  <span className="font-mono ml-1" style={{ color: "var(--accent)" }}>{draft.agents.join(", ")}</span>
                </div>
                <div className="p-2 rounded col-span-2" style={{ background: "var(--bg-secondary)" }}>
                  <span style={{ color: "var(--text-muted)" }}>Actions:</span>
                  <span className="font-mono ml-1" style={{ color: "var(--text-secondary)" }}>{draft.actionModes.join(", ") || "DRAFT_ONLY"}</span>
                </div>
              </div>
              <div className="text-xs mb-3 p-2 rounded" style={{ background: draft.track==="green"?"var(--green-dim)":draft.track==="yellow"?"var(--yellow-dim)":"var(--red-dim)", color: "var(--text-primary)" }}>
                {draft.riskSummary}
              </div>
              <div className="flex flex-wrap gap-2">
                <button onClick={() => handleExecute("green")} disabled={loading}
                  className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors"
                  style={{ borderColor:"rgba(34,197,94,0.4)", color:"var(--green)", background:loading?"":"rgba(34,197,94,0.04)" }}>
                  ◈ 只读分析 Green
                </button>
                <button onClick={() => handleExecute("yellow")} disabled={loading}
                  className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors"
                  style={{ borderColor:"rgba(234,179,8,0.4)", color:"var(--yellow)", background:loading?"":"rgba(234,179,8,0.04)" }}>
                  ⚡ 协作审批 Yellow
                </button>
                <button onClick={() => handleExecute("red")} disabled={loading}
                  className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors"
                  style={{ borderColor:"rgba(239,68,68,0.4)", color:"var(--red)", background:loading?"":"rgba(239,68,68,0.04)" }}>
                  ⚠ 高风险执行 Red
                </button>
                <button onClick={() => { setDraft(null); appendSystem("Plan generated. No execution triggered."); }}
                  className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors"
                  style={{ borderColor:"var(--border-accent)", color:"var(--text-secondary)" }}>
                  ⊡ 只生成计划
                </button>
                <button onClick={handleCancel}
                  className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors"
                  style={{ borderColor:"var(--border-accent)", color:"var(--text-muted)" }}>
                  ✕ 取消
                </button>
              </div>
            </div>
          </div>
        )}

        {loading && !draft && (
          <div className="flex justify-start">
            <div className="chat-msg system">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold" style={{ color: "var(--text-muted)" }}>coevo</span>
                <span className="text-xs" style={{ color: "var(--accent)" }}>processing...</span>
              </div>
            </div>
          </div>
        )}
        <div ref={chatEnd} />
      </div>

      {/* Input */}
      <div className="px-5 py-3 border-t" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="flex gap-2 max-w-3xl mx-auto">
          <textarea
            className="flex-1 p-3 rounded-xl border text-sm resize-none input-glow focus:outline-none"
            rows={2}
            placeholder="Describe your governance mission..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
            style={{ borderColor: "var(--border-subtle)", background: "var(--bg-secondary)" }}
          />
          <button onClick={() => handleSend()} disabled={loading || !input.trim()}
            className="px-5 py-3 rounded-xl text-sm font-semibold transition-colors disabled:opacity-30 self-end"
            style={{ background: "var(--accent)", color: "#fff" }}>
            {loading ? "···" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
