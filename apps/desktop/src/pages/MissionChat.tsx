import { useState } from "react";
import { Link } from "react-router-dom";
import { ensureWorkspaceDefaults } from "../api/bootstrap";
import { compileContract, createWorkOrder, modelChat, routePlan } from "../api/client";
import { useGovernance } from "../hooks/useGovernance";
import { getLocalIdentity } from "../settings/identity";
import { inferTrackFromIntent } from "../utils/trackInference";

type Msg = { role: "user" | "system"; text: string };

export default function MissionChat() {
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [creating, setCreating] = useState(false);
  const [lastWorkOrderId, setLastWorkOrderId] = useState("");
  const { set: setGovernance } = useGovernance();

  async function send() {
    const text = input.trim();
    if (!text || creating) return;
    setInput("");
    setCreating(true);
    setMessages((prev) => [
      ...prev,
      { role: "user", text },
      { role: "system", text: "Compiling mission contract and preparing a governed WorkOrder..." },
    ]);

    try {
      const track = inferTrackFromIntent(text);
      const bootstrap = await ensureWorkspaceDefaults(track.track);
      const compiled = await compileContract(text, "DRAFT");
      const contract = compiled.contract;
      const contractHash = compiled.contract_hash;
      const routed = await routePlan(contract, bootstrap.selectedAgentIds, contractHash) as { plan_hash?: string };
      const planHash = String(routed.plan_hash || "");
      let cognitionError = "";
      const cognition = await modelChat({
        role: "MissionDraft",
        messages: [
          {
            role: "system",
            content: "Summarize the user's mission for review. Do not authorize actions, assign permissions, or change governance tracks.",
          },
          { role: "user", content: text },
        ],
        temperature: 0.2,
        max_tokens: 240,
      }).catch((e: unknown) => {
        cognitionError = e instanceof Error ? e.message : String(e);
        return null;
      }) as Record<string, unknown> | null;
      const cognitionText = String(cognition?.content || "").trim();
      const identity = getLocalIdentity();
      const created = await createWorkOrder({
        contract_hash: contractHash,
        plan_hash: planHash,
        user_id: identity.userId,
        opc_id: identity.opcId,
        mission_intent: text,
        selected_agents: bootstrap.selectedAgentIds,
        selected_executors: [],
        required_skills: bootstrap.requiredSkillIds,
      }) as Record<string, unknown>;

      const workOrderId = String(created.work_order_id || "");
      const serverTrack = (["green", "yellow", "red"].includes(String(created.track))
        ? String(created.track)
        : track.track) as "green" | "yellow" | "red";
      const serverAllowedActions = Array.isArray(created.allowed_actions)
        ? created.allowed_actions.map(String)
        : [];
      const serverRiskSummary = String(created.risk_summary || track.reason);
      setLastWorkOrderId(workOrderId);
      setGovernance({
        phase: "review",
        track: serverTrack,
        contractHash,
        planHash,
        contract,
        agents: bootstrap.selectedAgentIds,
        riskDecision: serverRiskSummary,
        approvalMode: serverTrack === "green" ? "AUTO_GREEN" : serverTrack === "yellow" ? "NEGATIVE_CONSENT" : "RED_BLOCKED",
        actionModes: serverAllowedActions,
        approvalRequired: serverTrack !== "green",
        traceparent: crypto.randomUUID(),
      });
      setMessages((prev) => [
        ...prev,
        ...(cognitionText ? [{ role: "system" as const, text: `Model cognition: ${cognitionText}` }] : []),
        ...(!cognitionText && cognitionError ? [{ role: "system" as const, text: `Cognition summary unavailable: ${cognitionError}` }] : []),
        {
          role: "system",
          text: `WorkOrder ${workOrderId} created as ${serverTrack.toUpperCase()} Track. Review it in Work Orders before execution.`,
        },
      ]);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setMessages((prev) => [
        ...prev,
        { role: "system", text: `Unable to create WorkOrder: ${msg}` },
      ]);
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-3 border-b" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="text-sm font-semibold">Mission Composer</div>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>Natural language in, governed WorkOrder out</div>
      </div>
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {messages.length === 0 && (
          <div className="text-center pt-16">
            <div className="text-3xl mb-3" style={{ color: "var(--accent)" }}>coevo</div>
            <h1 className="text-xl font-bold mb-1">What mission should coevo govern?</h1>
            <p className="text-sm" style={{ color: "var(--text-muted)" }}>Enter a mission to create an auditable WorkOrder.</p>
          </div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={m.role === "user" ? "text-right" : "text-left"}>
            <div className="chat-msg inline-block" style={{ background: m.role === "user" ? "#f0f0ff" : "#fff", border: "1px solid var(--border-subtle)" }}>
              <div className="text-xs mb-1" style={{ color: "var(--text-muted)" }}>{m.role === "user" ? "You" : "coevo"}</div>
              <div>{m.text}</div>
            </div>
          </div>
        ))}
        {lastWorkOrderId && (
          <div className="text-center">
            <Link
              to="/work-orders"
              className="inline-flex px-3 py-2 rounded-md text-xs font-semibold border"
              style={{ borderColor: "var(--accent)", color: "var(--accent)" }}
            >
              Open Work Orders
            </Link>
          </div>
        )}
      </div>
      <div className="px-5 py-3 border-t" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="flex gap-2 max-w-3xl mx-auto">
          <textarea className="flex-1 p-3 rounded-xl border text-sm resize-none" rows={2} value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }} />
          <button disabled={creating} className="px-5 py-3 rounded-xl text-sm font-semibold disabled:opacity-50" style={{ background: "var(--accent)", color: "#fff" }} onClick={send}>
            {creating ? "Creating..." : "Create WorkOrder"}
          </button>
        </div>
      </div>
    </div>
  );
}
