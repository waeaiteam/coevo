import { useEffect, useMemo, useState } from "react";
import { ensureWorkspaceDefaults } from "../api/bootstrap";
import {
  appendConversationMessage,
  compileContract,
  createConversation,
  createWorkOrder,
  listConversationMessages,
  listConversations,
  listEmployees,
  modelChat,
  routePlan,
} from "../api/client";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { useGovernance } from "../hooks/useGovernance";
import { getLocalIdentity } from "../settings/identity";
import { useLanguage } from "../settings/i18n";
import { inferTrackFromIntent } from "../utils/trackInference";

type Msg = { role: "user" | "system"; text: string };
type AutonomyCeiling = "read_only" | "workspace_write" | "full_access";
type ModelPreference = "fast" | "standard" | "reasoning";
type Employee = Record<string, unknown>;
type GovernanceVerdict = {
  effective_track: "green" | "yellow" | "red";
  effective_tier: AutonomyCeiling;
  requested_ceiling: AutonomyCeiling;
  downgraded: boolean;
  downgrade_reason?: string | null;
  blocked: boolean;
  block_reason?: string | null;
  resolved_agent_id?: string | null;
};

const ACTIVE_CONVERSATION_KEY = "coevo-active-conversation-id";

const autonomyOptions: Array<{ value: AutonomyCeiling; label: string }> = [
  { value: "read_only", label: "只读" },
  { value: "workspace_write", label: "工作区可写" },
  { value: "full_access", label: "完全访问" },
];

const modelOptions: Array<{ value: ModelPreference; label: string }> = [
  { value: "fast", label: "快速" },
  { value: "standard", label: "标准" },
  { value: "reasoning", label: "推理" },
];

function publicText(text: string) {
  return text
    .replace(/\bWorkOrder\b/g, "任务")
    .replace(/\bGovernance\b/g, "执行裁定")
    .replace(/\bRiskGate\b/g, "风险检查")
    .replace(/\bTrack\b/g, "级别")
    .replace(/\bAgentSubHarness\b|\bReAct\b|\bGovernGate\b|\bharness\b/gi, "执行系统")
    .replace(/\bsandbox\b/gi, "本地保护")
    .replace(/\bPolicy\b/g, "规则")
    .replace(/\btoken\b/gi, "用量")
    .replace(/\btraceparent\b/gi, "追踪编号");
}

function toMsg(row: Record<string, unknown>): Msg {
  const role = row.role === "user" ? "user" : "system";
  const text = String(row.content || "");
  return { role, text: role === "system" ? publicText(text) : text };
}

function conversationTitle(text: string): string {
  const clean = text.trim().replace(/\s+/g, " ");
  return clean.length > 48 ? `${clean.slice(0, 45)}...` : clean || "新任务";
}

function departmentLabel(value: unknown) {
  const raw = String(value || "custom").toLowerCase();
  const labels: Record<string, string> = {
    founderoffice: "创始人办公室",
    founder_office: "创始人办公室",
    product: "产品",
    engineering: "工程",
    research: "研究",
    growth: "增长",
    finance: "财务",
    legal: "法务",
    sre: "稳定性",
    design: "设计",
    content: "内容",
    governance: "风控",
    custom: "其他",
  };
  return labels[raw] || String(value || "其他");
}

function employeeName(employee: Employee) {
  return String(employee.display_name || employee.agent_id || "AI 员工");
}

function groupEmployees(employees: Employee[]) {
  return employees.reduce<Record<string, Employee[]>>((groups, employee) => {
    const key = departmentLabel(employee.department);
    groups[key] = groups[key] || [];
    groups[key].push(employee);
    return groups;
  }, {});
}

function tierLabel(tier?: AutonomyCeiling) {
  return autonomyOptions.find((option) => option.value === tier)?.label || "只读";
}

function previewLabel(track: string) {
  if (track === "red") return "预估会先停止并请求人工处理";
  if (track === "yellow") return "预估需要你确认后继续";
  return "预估可直接开始整理";
}

function buildLocalSpans(verdict: GovernanceVerdict | undefined, cognitionText: string): TimelineSpan[] {
  return [
    {
      id: "span-intake",
      type: "BuildContext",
      label: "理解任务",
      round: 0,
      durationMs: 180,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: { outcome: "allow" },
      output: "任务目标已保存到本地会话。",
    },
    {
      id: "span-model",
      type: "ModelCall",
      label: "生成执行摘要",
      round: 0,
      durationMs: 640,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: { outcome: "allow" },
      thought: cognitionText,
      proposal: cognitionText || "等待模型摘要",
      confidence: cognitionText ? 0.74 : 0.5,
      usage: { display: "创建后会显示真实用量" },
      output: cognitionText,
    },
    {
      id: "span-verdict",
      type: "SelectTool",
      label: "确认执行边界",
      round: 0,
      durationMs: 90,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: {
        outcome: verdict?.blocked ? "blocked" : verdict?.downgraded ? "need_approval" : "allow",
        reason: verdict?.blocked ? "任务需要人工处理" : verdict?.downgraded ? "已按本地安全边界自动收紧" : "可在当前边界内继续",
      },
      overlays: verdict?.blocked ? ["sandbox_blocked"] : verdict?.downgraded ? ["need_approval"] : [],
      output: verdict ? `有效权限：${tierLabel(verdict.effective_tier)}` : "等待服务端裁定",
    },
  ];
}

export default function MissionChat() {
  useLanguage();
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [creating, setCreating] = useState(false);
  const [lastWorkOrderId, setLastWorkOrderId] = useState("");
  const [conversationId, setConversationId] = useState("");
  const [autonomy, setAutonomy] = useState<AutonomyCeiling>("read_only");
  const [modelPreference, setModelPreference] = useState<ModelPreference>("standard");
  const [assignedAgentId, setAssignedAgentId] = useState("auto");
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [lastVerdict, setLastVerdict] = useState<GovernanceVerdict | undefined>();
  const [timelineSpans, setTimelineSpans] = useState<TimelineSpan[]>([]);
  const { set: setGovernance } = useGovernance();

  const preview = useMemo(() => inferTrackFromIntent(input), [input]);
  const employeeGroups = useMemo(() => groupEmployees(employees), [employees]);
  const hasTask = Boolean(lastWorkOrderId || messages.length > 0);

  useEffect(() => {
    listEmployees().then(setEmployees).catch(() => setEmployees([]));
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function loadActiveConversation() {
      let active = "";
      try {
        active = localStorage.getItem(ACTIVE_CONVERSATION_KEY) || "";
      } catch {
        active = "";
      }
      try {
        if (!active) {
          const threads = await listConversations();
          active = String(threads[0]?.conversation_id || "");
          if (active) localStorage.setItem(ACTIVE_CONVERSATION_KEY, active);
        }
        if (!active || cancelled) return;
        const rows = await listConversationMessages(active);
        if (cancelled) return;
        setConversationId(active);
        setMessages(rows.map(toMsg).filter((m) => m.text));
        const linked = [...rows].reverse().find((row) => typeof row.linked_work_order_id === "string");
        setLastWorkOrderId(String(linked?.linked_work_order_id || ""));
      } catch {
        if (!cancelled) setConversationId(active);
      }
    }
    loadActiveConversation();
    return () => {
      cancelled = true;
    };
  }, []);

  async function ensureConversation(text: string): Promise<string> {
    if (conversationId) return conversationId;
    let active = "";
    try {
      active = localStorage.getItem(ACTIVE_CONVERSATION_KEY) || "";
    } catch {
      active = "";
    }
    if (active) {
      setConversationId(active);
      return active;
    }
    const identity = getLocalIdentity();
    const created = await createConversation({
      opc_id: identity.opcId,
      user_id: identity.userId,
      title: conversationTitle(text),
    }) as Record<string, unknown>;
    active = String(created.conversation_id || "");
    if (!active) throw new Error("会话创建失败");
    try {
      localStorage.setItem(ACTIVE_CONVERSATION_KEY, active);
    } catch {
      // Server persistence remains the source of truth.
    }
    setConversationId(active);
    return active;
  }

  async function send() {
    const text = input.trim();
    if (!text || creating) return;
    setInput("");
    setCreating(true);
    setMessages((prev) => [
      ...prev,
      { role: "user", text },
      { role: "system", text: "正在理解任务并准备执行边界..." },
    ]);

    let activeConversationId = conversationId;
    try {
      activeConversationId = await ensureConversation(text);
      await appendConversationMessage(activeConversationId, { role: "user", content: text });
      const bootstrap = await ensureWorkspaceDefaults();
      const selectedAgentIds = assignedAgentId === "auto" ? bootstrap.selectedAgentIds : [assignedAgentId];
      const compiled = await compileContract(text, "DRAFT");
      const contract = compiled.contract;
      const contractHash = compiled.contract_hash;
      const routed = await routePlan(contract, selectedAgentIds, contractHash) as { plan_hash?: string };
      const planHash = String(routed.plan_hash || "");
      let cognitionError = "";
      const cognition = await modelChat({
        role: "MissionDraft",
        messages: [
          {
            role: "system",
            content: "用面向客户的中文概括任务执行思路。不要提到授权、Track、WorkOrder、沙箱、治理网关或内部实现。",
          },
          { role: "user", content: text },
        ],
        temperature: 0.2,
        max_tokens: 240,
      }).catch((e: unknown) => {
        cognitionError = e instanceof Error ? e.message : String(e);
        return null;
      }) as Record<string, unknown> | null;
      const cognitionText = publicText(String(cognition?.content || "").trim());
      const identity = getLocalIdentity();
      const created = await createWorkOrder({
        conversation_id: activeConversationId,
        contract_hash: contractHash,
        plan_hash: planHash,
        user_id: identity.userId,
        opc_id: identity.opcId,
        mission_intent: text,
        selected_agents: selectedAgentIds,
        selected_executors: [],
        required_skills: bootstrap.requiredSkillIds,
        governance_proposal: {
          autonomy_ceiling: autonomy,
          model_preference: modelPreference,
          assigned_agent_id: assignedAgentId === "auto" ? null : assignedAgentId,
        },
      }) as Record<string, unknown>;

      const workOrderId = String(created.work_order_id || "");
      const verdict = created.governance_verdict as GovernanceVerdict | undefined;
      const systemMessages: Msg[] = [
        ...(cognitionText ? [{ role: "system" as const, text: cognitionText }] : []),
        ...(!cognitionText && cognitionError ? [{ role: "system" as const, text: `模型摘要暂不可用：${publicText(cognitionError)}` }] : []),
        { role: "system", text: "任务已创建，正在准备给你确认的执行方案。" },
      ];
      for (const msg of systemMessages) {
        await appendConversationMessage(activeConversationId, {
          role: "assistant",
          content: msg.text,
          linked_work_order_id: msg.text.includes("任务已创建") ? workOrderId : undefined,
        });
      }
      setLastWorkOrderId(workOrderId);
      setLastVerdict(verdict);
      setTimelineSpans(buildLocalSpans(verdict, cognitionText));
      if (verdict) {
        setGovernance({
          phase: "review",
          track: verdict.effective_track,
          contractHash,
          planHash,
          contract,
          agents: verdict.resolved_agent_id ? [verdict.resolved_agent_id] : selectedAgentIds,
          riskDecision: verdict.blocked ? "blocked" : verdict.downgraded ? "downgraded" : "allowed",
          approvalMode: verdict.effective_track === "green" ? "AUTO_GREEN" : verdict.effective_track === "yellow" ? "NEGATIVE_CONSENT" : "RED_BLOCKED",
          actionModes: [],
          approvalRequired: verdict.effective_track !== "green",
          traceparent: crypto.randomUUID(),
        });
      }
      setMessages((prev) => [...prev, ...systemMessages]);
    } catch (e: unknown) {
      const msg = publicText(e instanceof Error ? e.message : String(e));
      if (activeConversationId) {
        await appendConversationMessage(activeConversationId, {
          role: "assistant",
          content: `创建任务时遇到问题：${msg}`,
        }).catch(() => undefined);
      }
      setMessages((prev) => [...prev, { role: "system", text: `创建任务时遇到问题：${msg}` }]);
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="mission-page">
      <div className={`mission-stage ${hasTask ? "has-task" : ""}`}>
        <section className="mission-left">
          <div className="mission-thread">
            {messages.length === 0 ? (
              <div className="mission-hero">
                <h1 className="mission-title">今天让 AI 员工帮你做什么？</h1>
                <p className="mission-subtitle">
                  输入一个真实任务，coevo 会帮你分配合适的 AI 员工、确认可执行边界，并把过程保存在本机。
                </p>
                <div className="mission-cards">
                  <div className="mission-card">
                    <div className="mission-card-title">今日进展</div>
                    <div className="mission-card-text">跟踪正在推进的任务和下一步确认事项。</div>
                  </div>
                  <div className="mission-card">
                    <div className="mission-card-title">交付物</div>
                    <div className="mission-card-text">把整理结果、方案和文件沉淀成可复用成果。</div>
                  </div>
                  <div className="mission-card">
                    <div className="mission-card-title">数据保存在本机</div>
                    <div className="mission-card-text">默认本地优先，执行边界由服务端统一裁定。</div>
                  </div>
                </div>
              </div>
            ) : (
              <>
                {lastVerdict && (
                  <div className="mb-4">
                    <span className={`verdict-chip ${lastVerdict.blocked ? "blocked" : ""}`}>
                      <strong>治理裁定</strong>
                      <span>{lastVerdict.blocked ? "需要人工处理" : `有效权限：${tierLabel(lastVerdict.effective_tier)}`}</span>
                      {lastVerdict.downgraded && <span>已自动收紧</span>}
                    </span>
                  </div>
                )}
                {messages.map((message, index) => (
                  <div key={`${message.role}-${index}`} className={`message-row ${message.role}`}>
                    <div className={`chat-msg ${message.role}`}>
                      <div className="message-author">{message.role === "user" ? "你" : "coevo"}</div>
                      <div>{message.text}</div>
                    </div>
                  </div>
                ))}
              </>
            )}
          </div>
          <Composer
            input={input}
            creating={creating}
            autonomy={autonomy}
            modelPreference={modelPreference}
            assignedAgentId={assignedAgentId}
            employeeGroups={employeeGroups}
            previewText={input.trim() ? previewLabel(preview.track) : "输入后会先给出灰色预估，最终以服务端裁定为准"}
            onInput={setInput}
            onAutonomy={setAutonomy}
            onModelPreference={setModelPreference}
            onAssignedAgent={setAssignedAgentId}
            onSend={send}
          />
        </section>
        {hasTask && (
          <aside className="mission-right">
            <GovernanceTimeline spans={timelineSpans} title="执行时间线" />
          </aside>
        )}
      </div>
    </div>
  );
}

function Composer({
  input,
  creating,
  autonomy,
  modelPreference,
  assignedAgentId,
  employeeGroups,
  previewText,
  onInput,
  onAutonomy,
  onModelPreference,
  onAssignedAgent,
  onSend,
}: {
  input: string;
  creating: boolean;
  autonomy: AutonomyCeiling;
  modelPreference: ModelPreference;
  assignedAgentId: string;
  employeeGroups: Record<string, Employee[]>;
  previewText: string;
  onInput: (value: string) => void;
  onAutonomy: (value: AutonomyCeiling) => void;
  onModelPreference: (value: ModelPreference) => void;
  onAssignedAgent: (value: string) => void;
  onSend: () => void;
}) {
  return (
    <div className="composer-wrap">
      <div className="composer">
        <textarea
          className="composer-textarea"
          placeholder="例如：整理本周客户线索，并生成跟进清单"
          value={input}
          onChange={(event) => onInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              onSend();
            }
          }}
        />
        <div className="composer-bar">
          <button type="button" className="icon-button" aria-label="添加附件">＋</button>
          <select className="select-control" aria-label="自主度" value={autonomy} onChange={(event) => onAutonomy(event.target.value as AutonomyCeiling)}>
            {autonomyOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
          <select className="select-control" aria-label="指派员工" value={assignedAgentId} onChange={(event) => onAssignedAgent(event.target.value)}>
            <option value="auto">自动派单</option>
            {Object.entries(employeeGroups).map(([department, employees]) => (
              <optgroup key={department} label={department}>
                {employees.map((employee) => {
                  const id = String(employee.agent_id || "");
                  return <option key={id} value={id}>{employeeName(employee)}</option>;
                })}
              </optgroup>
            ))}
          </select>
          <select className="select-control" aria-label="模型" value={modelPreference} onChange={(event) => onModelPreference(event.target.value as ModelPreference)}>
            {modelOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
          <span className="intent-preview">{previewText}</span>
          <button type="button" disabled={creating || !input.trim()} className="primary-button" onClick={onSend}>
            {creating ? "创建中" : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}
