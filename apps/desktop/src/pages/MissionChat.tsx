import { useState, useRef, useEffect } from "react";
import { compileContract, routePlan, runDemo, createWorkOrder, executeWorkOrder } from "../api/client";
import { useGovernance } from "../hooks/useGovernance";
import { inferTrackFromIntent } from "../utils/trackInference";
import MissionDraftCard, { type MissionDraft } from "../components/mission/MissionDraftCard";

// ---- Types ----
type MissionPhase = "idle" | "drafting" | "review" | "executing" | "completed" | "cancelled" | "error";
type MessageKind = "text" | "draft" | "plan" | "execution_result" | "warning" | "error";
type Track = "green" | "yellow" | "red";

interface ChatMessage {
  id: string;
  role: "user" | "system";
  kind: MessageKind;
  content: string;
  detail?: string;
  track?: Track;
}

// ---- Constants ----
const EXAMPLES = [
  "Analyze production database anomalies and generate a remediation plan",
  "Review whether this PR is safe to merge",
  "Coordinate multiple agents to generate a daily market report",
  "Check if any agent exceeded its data access boundaries",
];

const DISCLAIMER =
  "coevo does not create unconstrained agents. It selects from Agent Registry and may spawn short-lived Task Agent Instances with limited permissions. Ephemeral Sub-Agents can only write Hypothesis or Suggestion, never Fact or Decision directly.";

// ---- Component ----
export default function MissionChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [phase, setPhase] = useState<MissionPhase>("idle");
  const [draft, setDraft] = useState<MissionDraft | null>(null);
  const [lastTrack, setLastTrack] = useState<Track | null>(null);
  const { set: setGov, reset: resetGov } = useGovernance();
  const chatEnd = useRef<HTMLDivElement>(null);

  useEffect(() => { chatEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [messages, draft, phase]);

  // ============ STAGE 1: Create Mission Draft ============
  async function createMissionDraft(intent?: string) {
    const text = (intent ?? input).trim();
    if (!text || phase === "drafting" || phase === "executing") return;
    setInput("");
    setPhase("drafting");
    setDraft(null);
    resetGov();

    appendMsg("user", "text", text);

    try {
      // Compile
      appendMsg("system", "text", "Compiling MCL contract from intent...");
      const compileRes = await compileContract(text, "DRAFT");
      const c = compileRes.contract as Record<string, unknown>;
      const hp = (c.human_approval_policy as Record<string, unknown>) || {};
      const actionModes = (c.allowed_action_modes as string[]) || [];

      // Route
      appendMsg("system", "text", "Querying Agent Registry and computing execution plan...");
      const agents = ["agent-synthesizer-01", "agent-critic-01"];
      const routeRes = await routePlan(compileRes.contract, agents);
      const planHash = (routeRes as { plan_hash: string }).plan_hash;

      // Infer track
      const inferred = inferTrackFromIntent(text);
      setLastTrack(inferred.track);

      // Allowed / restricted actions
      const allowed = actionModes.length > 0
        ? actionModes.map((a) => a.replace(/_/g, " ").toLowerCase())
        : ["read metrics", "analyze logs"];

      const restricted = inferred.track === "red"
        ? ["production rollback", "database mutation", "external write", "customer data delete"]
        : inferred.track === "yellow"
        ? ["production write", "financial transfer"]
        : ["any write operation"];

      // Build draft
      const dr: MissionDraft = {
        intent: text,
        suggestedTrack: inferred.track,
        reason: inferred.reason,
        contractHash: compileRes.contract_hash,
        planHash,
        ambiguityScore: compileRes.ambiguity_score,
        selectedAgents: agents,
        allowedActions: allowed,
        restrictedActions: restricted,
        approvalRequired: inferred.track === "yellow" || inferred.track === "red",
        approvalMode: (hp.approval_mode as string) || "NEGATIVE_CONSENT",
        compileResult: compileRes,
        routeResult: routeRes,
      };
      setDraft(dr);
      setPhase("review");

      // Governance panel
      setGov({
        phase: "review",
        track: inferred.track,
        contractHash: dr.contractHash,
        planHash: dr.planHash,
        contract: c,
        agents,
        riskDecision: "pending_human_choice",
        approvalMode: dr.approvalMode,
        actionModes: allowed,
        approvalRequired: dr.approvalRequired,
        traceparent: "",
      });
    } catch (e: unknown) {
      appendMsg("system", "error", `Error: ${e instanceof Error ? e.message : String(e)}`);
      setPhase("error");
    }
  }

  // ============ STAGE 2: Execute Mission ============
  async function executeMission(track: Track) {
    if (!draft) return;
    setPhase("executing");
    setDraft(null);
    setGov({
      phase: "executing", track, contractHash: draft.contractHash, planHash: draft.planHash,
      contract: null, agents: draft.selectedAgents,
      riskDecision: "executing", approvalMode: draft.approvalMode,
      actionModes: draft.allowedActions, approvalRequired: false, traceparent: "",
    });

    if (track === "red") {
      appendMsg("system", "warning",
        "Red Track requires caller identity proof and dual-sign. The current demo will use mock signatures.");
    }

    try {
      appendMsg("system", "text", `Executing ${track.toUpperCase()} Track via WorkOrder...`);
      // Create WorkOrder first
      const woId = `wo-${crypto.randomUUID()}`;
      const woResp = await createWorkOrder({
        work_order_id: woId, contract_hash: draft.contractHash, plan_hash: draft.planHash,
        user_id: "default-founder", opc_id: "default-opc", mission_intent: draft.intent,
        selected_agents: draft.selectedAgents, selected_executors: [], required_skills: [],
        track, allowed_actions: draft.allowedActions,
        restricted_actions: draft.restrictedActions, risk_summary: draft.reason,
      });
      const createdWoId = (woResp as Record<string,unknown>).work_order_id as string || woId;
      // Execute
      const execRes = await executeWorkOrder(createdWoId, track === "red" ? {
        caller_identity_proof: "mock-demo-proof",
        monitoring_signature: "mon-sig:demo",
        diagnostic_signature: "diag-sig:demo",
        lease_id: "lease-demo",
      } : {});
      const execStatus = (execRes as Record<string,unknown>).status as string;
      const memIds = (execRes as Record<string,unknown>).memory_ids as string[] || [];
      if (execStatus === "WaitingApproval") {
        appendMsg("system", "warning",
          `Yellow Track: ${(execRes as Record<string,unknown>).message || "Awaiting approval"}`,
          `approval_mode: ${(execRes as Record<string,unknown>).approval_mode} | status: ${execStatus}`,
          "yellow");
        setPhase("review");
        return;
      }
      appendMsg("system", "execution_result",
        `${track.toUpperCase()} Track completed via WorkOrder`,
        `status: ${execStatus} | memory_ids: ${memIds.length} | wo: ${createdWoId}`,
        track);
      setPhase("completed");
      setGov({
        phase: "done", track, contractHash: draft.contractHash, planHash: draft.planHash,
        contract: null, agents: draft.selectedAgents,
        riskDecision: track === "red" ? "ALLOW_WITH_LEASE" : "ALLOW",
        approvalMode: draft.approvalMode, actionModes: draft.allowedActions,
        approvalRequired: false, traceparent: "",
      });
      appendMsg("system", "text",
        "coevo Governance Mesh completed. Human responsibility is anchored via ADR-A. Audit trail preserved.");
    } catch (e: unknown) {
      appendMsg("system", "error", `Execution error: ${e instanceof Error ? e.message : String(e)}`);
      setPhase("review");
      setDraft(draft);
    }
  }

  // ============ Plan Only ============
  function planOnly() {
    if (!draft) return;
    setPhase("completed");
    setDraft(null);
    appendMsg("system", "plan", "Plan generated. No execution triggered.", `contract: ${draft.contractHash.slice(0, 14)}... | plan: ${draft.planHash.slice(0, 14)}...`);
    setGov({ phase: "done", track: draft.suggestedTrack, contractHash: draft.contractHash, planHash: draft.planHash, contract: null, agents: draft.selectedAgents, riskDecision: "PLANNED_ONLY", approvalMode: draft.approvalMode, actionModes: draft.allowedActions, approvalRequired: false, traceparent: "" });
  }

  // ============ Cancel ============
  function cancelMission() {
    setDraft(null);
    setPhase("cancelled");
    resetGov();
    appendMsg("system", "text", "Mission cancelled. No execution performed.");
    setTimeout(() => setPhase("idle"), 100);
  }

  // ============ Helpers ============
  function appendMsg(role: "user" | "system", kind: MessageKind, content: string, detail?: string, track?: Track) {
    setMessages((prev) => [...prev, { id: crypto.randomUUID(), role, kind, content, detail, track }]);
  }

  const isLoading = phase === "drafting" || phase === "executing";
  const showWelcome = messages.length === 0 && !draft && phase === "idle";

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-5 py-3 border-b" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">Mission Composer</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>
          Draft first, review boundaries, then execute — coevo governance
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {showWelcome && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-3xl mb-3" style={{ color: "var(--accent)" }}>◈</div>
            <h1 className="text-xl font-bold mb-1">你想让 coevo 治理什么任务？</h1>
            <p className="text-sm mb-6" style={{ color: "var(--text-muted)" }}>
              Describe your mission. coevo generates a draft for your review before any execution occurs.
            </p>
            <div className="grid grid-cols-2 gap-2 max-w-lg">
              {EXAMPLES.map((ex) => (
                <button key={ex} onClick={() => createMissionDraft(ex)} disabled={isLoading}
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
            <div className="chat-msg" style={{
              background: m.role === "user" ? "#f0f0ff" : m.kind === "error" ? "#fee2e2" : m.kind === "warning" ? "#fef9c3" : "#fff",
              border: m.role === "user" ? "none" : "1px solid var(--border-subtle)",
              maxWidth: m.role === "user" ? "85%" : "90%",
            }}>
              <div className="flex items-center gap-2 mb-0.5">
                <span className="text-xs font-semibold" style={{ color: m.role === "user" ? "var(--accent)" : "var(--text-muted)" }}>
                  {m.role === "user" ? "You" : "coevo"}
                </span>
                {m.track && <span className={`track-${m.track}`}>{m.track}</span>}
                {m.kind !== "text" && (
                  <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: "var(--bg-secondary)", color: "var(--text-muted)" }}>
                    {m.kind}
                  </span>
                )}
              </div>
              <div style={{ color: "var(--text-primary)" }}>{m.content}</div>
              {m.detail && <div className="text-xs mt-1 font-mono" style={{ color: "var(--text-muted)" }}>{m.detail}</div>}
            </div>
          </div>
        ))}

        {/* Mission Draft Card */}
        {draft && phase === "review" && (
          <MissionDraftCard
            draft={draft}
            loading={isLoading}
            onExecute={executeMission}
            onPlanOnly={planOnly}
            onCancel={cancelMission}
          />
        )}

        {isLoading && !draft && (
          <div className="flex justify-start">
            <div className="chat-msg" style={{ background: "#fff", border: "1px solid var(--border-subtle)" }}>
              <span className="text-xs" style={{ color: "var(--accent)" }}>
                {phase === "drafting" ? "Generating Mission Draft..." : "Executing..."}
              </span>
            </div>
          </div>
        )}
        <div ref={chatEnd} />
      </div>

      {/* Disclaimer */}
      <div className="px-5 py-1.5 text-center text-xs" style={{ color: "var(--text-muted)", background: "var(--bg-secondary)" }}>
        {DISCLAIMER}
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
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); createMissionDraft(); } }}
            style={{ borderColor: "var(--border-subtle)", background: "var(--bg-secondary)" }}
          />
          <button onClick={() => createMissionDraft()} disabled={isLoading || !input.trim()}
            className="px-5 py-3 rounded-xl text-sm font-semibold transition-colors disabled:opacity-30 self-end"
            style={{ background: "var(--accent)", color: "#fff" }}>
            {isLoading ? "···" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
