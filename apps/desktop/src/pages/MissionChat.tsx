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

interface GovernanceState {
  track: string;
  contractHash: string;
  planHash: string;
  agents: string[];
  riskDecision: string;
  approvalRequired: boolean;
  traceparent: string;
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
  const { state: govState, set: setGovState } = useGovernance();
  const chatEnd = useRef<HTMLDivElement>(null);

  useEffect(() => { chatEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [messages]);

  async function handleSend(intent?: string) {
    const text = (intent ?? input).trim();
    if (!text || loading) return;
    setInput("");
    setLoading(true);

    const userMsg: ChatMessage = { id: crypto.randomUUID(), role: "user", content: text };
    setMessages((prev) => [...prev, userMsg]);

    try {
      // Step 1: Compile
      appendSystem("Compiling MCL contract from intent...");
      const compileRes = await compileContract(text, "DRAFT");
      appendSystem(
        "MCL contract compiled",
        `hash: ${compileRes.contract_hash.slice(0, 16)}... | ambiguity: ${compileRes.ambiguity_score.toFixed(2)}`
      );

      // Step 2: Route
      appendSystem("Querying Agent Registry and computing execution plan...");
      const agents = ["agent-synthesizer-01", "agent-critic-01"];
      const routeRes = await routePlan(compileRes.contract, agents);
      appendSystem(
        "Execution plan generated",
        `plan: ${(routeRes as {plan_hash:string}).plan_hash.slice(0, 16)}... | agents: ${agents.join(", ")}`
      );

      // Step 3: Determine risk track
      const riskLevel = text.toLowerCase().includes("production") || text.toLowerCase().includes("critical")
        ? "red" : text.toLowerCase().includes("deploy") || text.toLowerCase().includes("notification")
        ? "yellow" : "green";

      const trackLabel = { green: "Green Track (low risk, auto-execute)", yellow: "Yellow Track (moderate risk, approval window)", red: "Red Track (high risk, emergency lease)" }[riskLevel];

      appendSystem(
        `RiskGate classified as: ${trackLabel}`,
        `coevo never creates unconstrained agents. It selects from the Agent Registry: ${agents[0]}, ${agents[1]}. A short-lived Task Agent Instance is spawned with minimal permissions — it can only write Hypothesis and Suggestion, never Fact or Decision directly.`,
        riskLevel as "green" | "yellow" | "red"
      );

      // Step 4: Execute demo
      appendSystem(`Executing ${riskLevel.toUpperCase()} Track...`);
      const demoRes = await runDemo(riskLevel as "green" | "yellow" | "red");
      appendSystem(
        `${riskLevel.toUpperCase()} Track completed`,
        `contract: ${demoRes.contract_hash.slice(0, 12)}... | plan: ${demoRes.plan_hash.slice(0, 12)}... | entries: ${demoRes.entries_created.length} | ${demoRes.elapsed_ms}ms`,
        riskLevel as "green" | "yellow" | "red"
      );

      // Step 5: Update governance state
      setGovState({
        track: riskLevel,
        contractHash: demoRes.contract_hash,
        planHash: demoRes.plan_hash,
        agents,
        riskDecision: riskLevel === "red" ? "ALLOW_WITH_LEASE" : "ALLOW",
        approvalRequired: riskLevel === "yellow",
        traceparent: demoRes.traceparent,
      });

      appendSystem(
        "coevo Governance Mesh will never create agents without constraints. It selects from the Agent Registry. It only creates short-lived Task Agent Instances. Temporary Ephemeral Sub-Agents default to low privilege — write Hypothesis/Suggestion only, cannot write Fact/Decision directly. Human responsibility anchoring is always enforced via ADR-A."
      );
    } catch (e: unknown) {
      appendSystem(`Error: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }

  function appendSystem(content: string, detail?: string, track?: "green" | "yellow" | "red") {
    setMessages((prev) => [...prev, { id: crypto.randomUUID(), role: "system", content, detail, track }]);
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-5 py-3 border-b" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">Mission Chat</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>
          coevo Agent Governance Mesh — 内部推理自由，外部行为受治
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-3xl mb-3" style={{ color: "var(--accent)" }}>◈</div>
            <h1 className="text-xl font-bold mb-1">你想让 coevo 治理什么任务？</h1>
            <p className="text-sm mb-6" style={{ color: "var(--text-muted)" }}>
              Describe your mission. coevo will compile, route, and execute with full governance.
            </p>
            <div className="grid grid-cols-2 gap-2 max-w-lg">
              {EXAMPLES.map((ex) => (
                <button
                  key={ex}
                  onClick={() => handleSend(ex)}
                  disabled={loading}
                  className="text-left text-xs p-3 rounded-lg border transition-colors hover:border-indigo-300"
                  style={{ borderColor: "var(--border-subtle)", background: "#fff", color: "var(--text-secondary)" }}
                >
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
              <div style={{ color: m.role === "user" ? "var(--text-primary)" : "var(--text-secondary)" }}>
                {m.content}
              </div>
              {m.detail && (
                <div className="text-xs mt-1 font-mono" style={{ color: "var(--text-muted)" }}>
                  {m.detail}
                </div>
              )}
            </div>
          </div>
        ))}

        {loading && (
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
          <button
            onClick={() => handleSend()}
            disabled={loading || !input.trim()}
            className="px-5 py-3 rounded-xl text-sm font-semibold transition-colors disabled:opacity-30 self-end"
            style={{ background: "var(--accent)", color: "#fff" }}
          >
            {loading ? "···" : "Send"}
          </button>
        </div>
        <div className="text-xs mt-2 text-center" style={{ color: "var(--text-muted)" }}>
          coevo selects from Agent Registry. Creates short-lived Task Agent Instances. Ephemeral Sub-Agents: Hypothesis/Suggestion only, no direct Fact/Decision.
        </div>
      </div>
    </div>
  );
}
