import { useState, useRef, useEffect } from "react";
import { compileContract, routePlan, createWorkOrder, executeWorkOrder, listEmployees, seedEmployees, listExecutors, registerExecutor, modelStructured, modelChat } from "../api/client";
import { useGovernance } from "../hooks/useGovernance";
import { inferTrackFromIntent } from "../utils/trackInference";
import MissionDraftCard, { type MissionDraft } from "../components/mission/MissionDraftCard";

type MissionPhase = "idle" | "drafting" | "review" | "executing" | "completed" | "cancelled" | "error";
type MessageKind = "text" | "draft" | "plan" | "execution_result" | "warning" | "error";
type Track = "green" | "yellow" | "red";

interface ChatMessage { id: string; role: "user" | "system"; kind: MessageKind; content: string; detail?: string; track?: Track; }

const EXAMPLES = ["Analyze production database anomalies and generate a remediation plan","Review whether this PR is safe to merge","Coordinate multiple agents to generate a daily market report","Check if any agent exceeded its data access boundaries"];
const DISCLAIMER = "coevo does not create unconstrained agents. It selects from Agent Registry and may spawn short-lived Task Agent Instances with limited permissions. Ephemeral Sub-Agents can only write Hypothesis or Suggestion, never Fact or Decision directly.";

function selectAgents(intent: string, employees: Record<string,unknown>[], track: Track): string[] {
  const lower = intent.toLowerCase();
  const active = employees.filter(e => e.lifecycle_status === "Active");
  const picks: Set<string> = new Set();
  const name = (e:Record<string,unknown>) => e.display_name as string;
  const id = (e:Record<string,unknown>) => e.agent_id as string;
  // Founder/Synthesizer always
  const founder = active.find(e => id(e).includes("founder")) || active.find(e => id(e).includes("synth"));
  if (founder) picks.add(id(founder));
  // Red/Yellow need risk/critic
  if (track === "red" || track === "yellow") {
    const risk = active.find(e => id(e).includes("risk") || id(e).includes("critic"));
    if (risk) picks.add(id(risk));
  }
  // Code/PR → engineer
  if (lower.includes("code")||lower.includes("pr")||lower.includes("review")) {
    const eng = active.find(e => id(e).includes("engineer"));
    if (eng) picks.add(id(eng));
  }
  // Production/incident/database → SRE
  if (lower.includes("production")||lower.includes("incident")||lower.includes("database")||lower.includes("db")) {
    const sre = active.find(e => id(e).includes("sre"));
    if (sre) picks.add(id(sre));
  }
  // Report/summary → synthesizer
  if (lower.includes("report")||lower.includes("summary")||lower.includes("generate")) {
    const synth = active.find(e => id(e).includes("synth"));
    if (synth) picks.add(id(synth));
  }
  // Fill to 2-3
  for (const e of active) { if (picks.size >= 3) break; picks.add(id(e)); }
  return [...picks].slice(0, 3);
}

export default function MissionChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [phase, setPhase] = useState<MissionPhase>("idle");
  const [draft, setDraft] = useState<MissionDraft | null>(null);
  const [employees, setEmployees] = useState<Record<string,unknown>[]>([]);
  const [executors, setExecutors] = useState<Record<string,unknown>[]>([]);
  const [lastTrack, setLastTrack] = useState<Track | null>(null);
  const { set: setGov, reset: resetGov } = useGovernance();
  const chatEnd = useRef<HTMLDivElement>(null);

  useEffect(() => { chatEnd.current?.scrollIntoView({ behavior: "smooth" }); }, [messages, draft, phase]);
  useEffect(() => { loadEmployees(); loadExecutors(); }, []);

  async function loadEmployees() { try { setEmployees(await listEmployees()||[]); } catch { setEmployees([]); } }
  async function loadExecutors() { try { setExecutors(await listExecutors()||[]); } catch { setExecutors([]); } }

  function appendMsg(role: "user" | "system", kind: MessageKind, content: string, detail?: string, track?: Track) {
    setMessages((prev) => [...prev, { id: crypto.randomUUID(), role, kind, content, detail, track }]);
  }

  // ============ STAGE 1: Create Mission Draft ============
  async function createMissionDraft(intent?: string) {
    const text = (intent ?? input).trim();
    if (!text || phase === "drafting" || phase === "executing") return;
    setInput(""); setPhase("drafting"); setDraft(null); resetGov();
    appendMsg("user", "text", text);

    // Check employees
    let emps = employees;
    if (emps.length === 0) {
      appendMsg("system", "warning", "AI Employees are not initialized. Please seed employees first.");
      try { await seedEmployees(); await loadEmployees(); emps = await listEmployees()||[]; appendMsg("system", "text", `Seeded ${emps.length} AI Employees.`); }
      catch { appendMsg("system", "error", "Failed to seed employees. Please use AI Employees page."); setPhase("idle"); return; }
    }
    // Check executors
    let execs = executors;
    const registered = execs.filter(e => e.status === "Registered");
    if (registered.length === 0) {
      appendMsg("system", "warning", "No registered executor. Registering mock OpenClaw executor...");
      try {
        await registerExecutor({executor_id:"mock-openclaw-executor",display_name:"Mock OpenClaw Executor",source_type:"OpenClaw",runtime_endpoint:"http://localhost:0",capabilities:["mock"],required_credentials:[],permission_boundary:{max_risk_score:0.6,can_write_fact:false,can_write_decision:false,can_access_network:false,can_access_filesystem:false,can_call_external_executor:false,can_propose_skill:false},file_scope:[],network_scope:[],memory_scope:"Executor",risk_ceiling:0.6,supported_actions:["dry_run","execute"],sandbox_level:"Container",health_check_url:"",audit_callback_url:"",status:"Registered",created_at_ms:Date.now(),updated_at_ms:Date.now()});
        await loadExecutors(); execs = await listExecutors()||[];
        appendMsg("system", "text", "Registered mock OpenClaw executor.");
      } catch(e) { appendMsg("system", "error", "Failed to register executor: "+(e instanceof Error?e.message:String(e))); }
    }

    try {
      // Try model-enhanced Mission Draft
      let modelDraft: Record<string,unknown>|null = null;
      try {
        appendMsg("system", "text", "Generating model-enhanced Mission Draft...");
        const md = await modelStructured({
          role: "MissionDraft",
          messages: [
            {role:"system",content:"You are coevo Mission Draft assistant. Suggest mission boundaries only. Do not authorize actions."},
            {role:"user",content:text}
          ],
        }) as Record<string,unknown>;
        const mj = (md.json || md) as Record<string,unknown>;
        modelDraft = mj;
        const modelTrack = String(mj.suggested_track||"").toLowerCase();
        const detTrack = inferTrackFromIntent(text).track;
        appendMsg("system","text","Model-enhanced mission draft generated",
          `goal: ${mj.goal_summary||"N/A"} | track: ${modelTrack}`+(modelTrack!==detTrack?` (governance override: ${detTrack})`:""));
      } catch { appendMsg("system","warning","Model Gateway unavailable. Falling back to deterministic Mission Draft."); }

      appendMsg("system", "text", "Compiling MCL contract...");
      const compileRes = await compileContract(text, "DRAFT");
      const c = compileRes.contract as Record<string,unknown>;
      const hp = (c.human_approval_policy as Record<string,unknown>) || {};
      const actionModes = (c.allowed_action_modes as string[]) || [];

      const inferred = inferTrackFromIntent(text);
      setLastTrack(inferred.track);

      const selectedAgents = selectAgents(text, emps, inferred.track);
      appendMsg("system", "text", "Selected AI Employees from Registry",
        selectedAgents.map(a=>{const e=emps.find(x=>x.agent_id===a);return e?`${e.display_name} (${e.department})`:a;}).join(", "));

      appendMsg("system", "text", "Computing execution plan...");
      const routeRes = await routePlan(compileRes.contract, selectedAgents);
      const planHash = (routeRes as {plan_hash:string}).plan_hash;

      const allowed = actionModes.length>0?actionModes.map(a=>a.replace(/_/g," ").toLowerCase()):["read metrics","analyze logs"];
      const restricted = inferred.track==="red"?["production rollback","database mutation","external write","customer data delete"]:inferred.track==="yellow"?["production write","financial transfer"]:["any write operation"];

      const dr: MissionDraft = {
        intent:text,suggestedTrack:inferred.track,reason:inferred.reason,
        contractHash:compileRes.contract_hash,planHash,ambiguityScore:compileRes.ambiguity_score,
        selectedAgents,allowedActions:allowed,restrictedActions:restricted,
        approvalRequired:inferred.track==="yellow"||inferred.track==="red",
        approvalMode:(hp.approval_mode as string)||"NEGATIVE_CONSENT",compileResult:compileRes,routeResult:routeRes,
      };
      setDraft(dr); setPhase("review");
      setGov({phase:"review",track:inferred.track,contractHash:dr.contractHash,planHash:dr.planHash,contract:c,agents:dr.selectedAgents,riskDecision:"pending_human_choice",approvalMode:dr.approvalMode,actionModes:allowed,approvalRequired:dr.approvalRequired,traceparent:""});
    } catch (e: unknown) { appendMsg("system", "error", `Error: ${e instanceof Error?e.message:String(e)}`); setPhase("error"); }
  }

  // ============ STAGE 2: Execute ============
  async function executeMission(track: Track) {
    if (!draft) return;
    const execs = await listExecutors()||[];
    const registered = execs.filter((e:Record<string,unknown>) => e.status === "Registered");
    if (registered.length === 0) { appendMsg("system","error","No registered executor. Please register one before execution."); return; }
    const executorIds = registered.map(e => e.executor_id as string).slice(0, 2);

    if (track === "red") {
      appendMsg("system","warning","Red Approval Required",
        "Red Track needs: caller_identity_proof, monitoring_signature, diagnostic_signature, lease_id. The Alpha version blocks direct Red execution.",
        "red");
      appendMsg("system","text","Red Track blocked. Use the 'Create Approval Request' workflow or run in Demo mode from the Demos page.");
      return;
    }

    setPhase("executing"); setDraft(null);
    setGov({phase:"executing",track,contractHash:draft.contractHash,planHash:draft.planHash,contract:null,agents:draft.selectedAgents,riskDecision:"executing",approvalMode:draft.approvalMode,actionModes:draft.allowedActions,approvalRequired:false,traceparent:""});

    try {
      appendMsg("system","text",`Executing ${track.toUpperCase()} Track via WorkOrder...`);
      const woResp = await createWorkOrder({
        contract_hash:draft.contractHash,plan_hash:draft.planHash,
        user_id:"default-founder",opc_id:"default-opc",mission_intent:draft.intent,
        selected_agents:draft.selectedAgents,selected_executors:executorIds,required_skills:[],
        track,allowed_actions:draft.allowedActions,restricted_actions:draft.restrictedActions,risk_summary:draft.reason,
      });
      const woId = (woResp as Record<string,unknown>).work_order_id as string;
      const execRes = await executeWorkOrder(woId) as Record<string,unknown>;
      const execStatus = execRes.status as string;
      const memIds = execRes.memory_ids as string[] || [];

      if (execStatus === "WaitingApproval") {
        appendMsg("system","warning",`Yellow Track: ${execRes.message||"Awaiting approval"}`,
          `approval_mode: ${execRes.approval_mode} | status: ${execStatus}`,"yellow");
        setPhase("review"); return;
      }

      appendMsg("system","execution_result",`${track.toUpperCase()} Track completed`,
        `work_order_id: ${woId} | status: ${execStatus} | executors: ${executorIds.length} | memory_ids: ${memIds.length}`,
        track);

      if (memIds.length === 0) appendMsg("system","warning","Execution completed but no task memory was written.","Check executor results.","yellow");

      setPhase("completed");
      setGov({phase:"done",track,contractHash:draft.contractHash,planHash:draft.planHash,contract:null,agents:draft.selectedAgents,riskDecision:"ALLOW" as string,approvalMode:draft.approvalMode,actionModes:draft.allowedActions,approvalRequired:false,traceparent:""});
      // Synthesizer summary
      try {
        const synth = await modelChat({
          role:"Synthesizer",
          messages:[
            {role:"system",content:"You summarize coevo WorkOrder execution. Do not claim unauthorized actions."},
            {role:"user",content:JSON.stringify({intent:draft.intent,work_order_id:woId,track,selected_agents:draft.selectedAgents,selected_executors:executorIds,executor_results:execRes.executor_results||[],memory_ids:memIds,status:execStatus})},
          ]
        }) as Record<string,unknown>;
        appendMsg("system","text","Synthesizer Summary",(synth.content||"") as string);
      } catch { appendMsg("system","warning","Synthesizer model unavailable. Showing raw execution result."); }

      appendMsg("system","text","coevo Governance Mesh completed. Human responsibility is anchored via ADR-A. Audit trail preserved.");
    } catch (e: unknown) {
      appendMsg("system","error",`Execution error: ${e instanceof Error?e.message:String(e)}`);
      setPhase("review"); setDraft(draft);
    }
  }

  function planOnly() {
    if (!draft) return; setPhase("completed"); setDraft(null);
    appendMsg("system","plan","Plan generated. No execution triggered.",`contract: ${draft.contractHash.slice(0,14)}... | plan: ${draft.planHash.slice(0,14)}...`);
    setGov({phase:"done",track:draft.suggestedTrack,contractHash:draft.contractHash,planHash:draft.planHash,contract:null,agents:draft.selectedAgents,riskDecision:"PLANNED_ONLY",approvalMode:draft.approvalMode,actionModes:draft.allowedActions,approvalRequired:false,traceparent:""});
  }
  function cancelMission() { setDraft(null); setPhase("cancelled"); resetGov(); appendMsg("system","text","Mission cancelled. No execution performed."); setTimeout(()=>setPhase("idle"),100); }

  const isLoading = phase === "drafting" || phase === "executing";
  const showWelcome = messages.length === 0 && !draft && phase === "idle";

  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-3 border-b" style={{background:"#fff",borderColor:"var(--border-subtle)"}}>
        <div className="text-sm font-semibold">Mission Composer</div>
        <div className="text-xs" style={{color:"var(--text-muted)"}}>Draft first, review boundaries, then execute — coevo governance</div>
      </div>
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
        {showWelcome && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-3xl mb-3" style={{color:"var(--accent)"}}>◈</div>
            <h1 className="text-xl font-bold mb-1">你想让 coevo 治理什么任务？</h1>
            <p className="text-sm mb-6" style={{color:"var(--text-muted)"}}>Describe your mission. coevo generates a draft for your review before any execution occurs.</p>
            <div className="grid grid-cols-2 gap-2 max-w-lg">
              {EXAMPLES.map(ex=>(<button key={ex} onClick={()=>createMissionDraft(ex)} disabled={isLoading} className="text-left text-xs p-3 rounded-lg border transition-colors hover:border-indigo-300" style={{borderColor:"var(--border-subtle)",background:"#fff",color:"var(--text-secondary)"}}>{ex}</button>))}
            </div>
          </div>
        )}
        {messages.map(m=>(
          <div key={m.id} className={`flex ${m.role==="user"?"justify-end":"justify-start"}`}>
            <div className="chat-msg" style={{background:m.role==="user"?"#f0f0ff":m.kind==="error"?"#fee2e2":m.kind==="warning"?"#fef9c3":"#fff",border:m.role==="user"?"none":"1px solid var(--border-subtle)",maxWidth:m.role==="user"?"85%":"90%"}}>
              <div className="flex items-center gap-2 mb-0.5">
                <span className="text-xs font-semibold" style={{color:m.role==="user"?"var(--accent)":"var(--text-muted)"}}>{m.role==="user"?"You":"coevo"}</span>
                {m.track&&<span className={`track-${m.track}`}>{m.track}</span>}
                {m.kind!=="text"&&<span className="text-xs px-1.5 py-0.5 rounded" style={{background:"var(--bg-secondary)",color:"var(--text-muted)"}}>{m.kind}</span>}
              </div>
              <div style={{color:"var(--text-primary)"}}>{m.content}</div>
              {m.detail&&<div className="text-xs mt-1 font-mono" style={{color:"var(--text-muted)"}}>{m.detail}</div>}
            </div>
          </div>
        ))}
        {draft && phase==="review" && <MissionDraftCard draft={draft} loading={isLoading} onExecute={executeMission} onPlanOnly={planOnly} onCancel={cancelMission} />}
        {isLoading && !draft && <div className="flex justify-start"><div className="chat-msg" style={{background:"#fff",border:"1px solid var(--border-subtle)"}}><span className="text-xs" style={{color:"var(--accent)"}}>{phase==="drafting"?"Generating Mission Draft...":"Executing..."}</span></div></div>}
        <div ref={chatEnd} />
      </div>
      <div className="px-5 py-1.5 text-center text-xs" style={{color:"var(--text-muted)",background:"var(--bg-secondary)"}}>{DISCLAIMER}</div>
      <div className="px-5 py-3 border-t" style={{background:"#fff",borderColor:"var(--border-subtle)"}}>
        <div className="flex gap-2 max-w-3xl mx-auto">
          <textarea className="flex-1 p-3 rounded-xl border text-sm resize-none input-glow focus:outline-none" rows={2} placeholder="Describe your governance mission..." value={input} onChange={e=>setInput(e.target.value)} onKeyDown={e=>{if(e.key==="Enter"&&!e.shiftKey){e.preventDefault();createMissionDraft();}}} style={{borderColor:"var(--border-subtle)",background:"var(--bg-secondary)"}} />
          <button onClick={()=>createMissionDraft()} disabled={isLoading||!input.trim()} className="px-5 py-3 rounded-xl text-sm font-semibold transition-colors disabled:opacity-30 self-end" style={{background:"var(--accent)",color:"#fff"}}>{isLoading?"···":"Send"}</button>
        </div>
      </div>
    </div>
  );
}
