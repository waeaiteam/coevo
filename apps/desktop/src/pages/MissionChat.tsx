import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ensureWorkspaceDefaults } from "../api/bootstrap";
import {
  compileContract,
  modelChat,
  routePlan,
} from "../api/client";
import {
  appendCompanyConversationMessage,
  createCompanyConversation,
  createCompanyWorkOrder,
  cancelCompanyWorkOrder,
  executeCompanyWorkOrder,
  getCompanyProfileById,
  listCompanyConversationMessages,
  listCompanyConversations,
  listCompanyEmployees,
  listCompanyWorkOrders,
} from "../api/org";
import { getActiveOpcId } from "../api/companies";
import { getTauriInvoke } from "../api/tauri";
import Icon from "../components/Icon";
import SlashCommandMenu from "../components/SlashCommandMenu";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { WorkerStreamView } from "../components/WorkerStreamView";
import { useGovernance } from "../hooks/useGovernance";
import { getLocalIdentity } from "../settings/identity";
import { t, useLanguage } from "../settings/i18n";
import { listField } from "../utils/productSurface";
import { inferTrackFromIntent } from "../utils/trackInference";
import { decideAndResume } from "../utils/approvalFlow";
import {
  buildSlashHelpMessage,
  getSlashCommandMatches,
  isSlashCommandInput,
  parseSlashCommandInput,
  resolveSlashTarget,
  type SlashCommandSpec,
} from "../utils/slashCommands";
import {
  clearMissionSession,
  MISSION_SESSION_CLEARED_EVENT,
  missionStateKey,
  readActiveConversationId,
  writeActiveConversationId,
} from "../utils/missionSession";

type Msg = { role: "user" | "system"; text: string };
type AutonomyCeiling = "read_only" | "workspace_write" | "full_access";
type ModelPreference = "fast" | "standard" | "reasoning";
type Employee = Record<string, unknown>;
type AttachmentMeta = { name: string; size: number; type?: string };
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

const autonomyOptions: AutonomyCeiling[] = ["read_only", "workspace_write", "full_access"];
const modelOptions: ModelPreference[] = ["fast", "standard", "reasoning"];

function publicText(text: string) {
  return text
    .replace(/\bWorkOrder\b/g, t("mission.public_task"))
    .replace(/\bGovernance\b/g, t("mission.public_decision"))
    .replace(/\bRiskGate\b/g, t("mission.public_risk_check"))
    .replace(/\bTrack\b/g, t("mission.public_level"))
    .replace(/\bAgentSubHarness\b|\bReAct\b|\bGovernGate\b|\bharness\b/gi, t("mission.public_execution_system"))
    .replace(/\bsandbox\b/gi, t("mission.public_local_guard"))
    .replace(/\bPolicy\b/g, t("mission.public_rule"))
    .replace(/\btoken\b/gi, t("mission.public_usage"))
    .replace(/\btraceparent\b/gi, t("mission.public_trace"));
}

function toMsg(row: Record<string, unknown>): Msg {
  const role = row.role === "user" ? "user" : "system";
  const text = String(row.content || "");
  return { role, text: role === "system" ? publicText(text) : text };
}

function conversationTitle(text: string): string {
  const clean = text.trim().replace(/\s+/g, " ");
  return clean.length > 48 ? `${clean.slice(0, 45)}...` : clean || t("nav.new_task");
}

function canSubmitMission(text: string): boolean {
  const normalized = text.trim();
  if (!normalized) return false;

  // Keep concise Chinese/Japanese/Korean prompts usable, but block single-character
  // ASCII noise like "A" or punctuation-only inputs that would otherwise create a task.
  if (/[\u3400-\u9FFF\u3040-\u30FF\uAC00-\uD7AF]/.test(normalized)) return true;

  return normalized.replace(/[^A-Za-z0-9]+/g, "").length >= 2;
}

function formatBytes(size: number): string {
  if (!Number.isFinite(size) || size <= 0) return "0 B";
  if (size < 1024) return `${size} B`;
  const kb = size / 1024;
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  const mb = kb / 1024;
  return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
}

function buildTaskIntent(text: string, attachments: AttachmentMeta[], projectFolder: string, projectName: string): string {
  const lines: string[] = [];
  if (projectName) lines.push(`${t("mission.context_project")}: ${projectName}`);
  if (projectFolder) lines.push(`${t("mission.context_project_folder")}: ${projectFolder}`);
  if (attachments.length > 0) {
    lines.push(`${t("mission.context_attachments")}: ${attachments.map((file) => `${file.name} (${formatBytes(file.size)})`).join(", ")}`);
  }
  if (lines.length === 0) return text;
  return `${text}\n\n${t("mission.context_header")}:\n${lines.map((line) => `- ${line}`).join("\n")}`;
}

function toPersistentMessages(messages: Msg[]): Msg[] {
  return messages
    .filter((m) => m.role === "user" || m.role === "system")
    .map((m) => ({ role: m.role, text: String(m.text || "").slice(0, 4000) }))
    .filter((m) => m.text.trim().length > 0);
}

function departmentLabel(value: unknown) {
  const raw = String(value || "custom").toLowerCase();
  const labels: Record<string, string> = {
    founderoffice: t("mission.department_founder"),
    founder_office: t("mission.department_founder"),
    product: t("mission.department_product"),
    engineering: t("mission.department_engineering"),
    research: t("mission.department_research"),
    growth: t("mission.department_growth"),
    finance: t("mission.department_finance"),
    legal: t("mission.department_legal"),
    sre: t("mission.department_sre"),
    design: t("mission.department_design"),
    content: t("mission.department_content"),
    governance: t("mission.department_governance"),
    custom: t("mission.department_custom"),
  };
  return labels[raw] || String(value || t("mission.department_custom"));
}

function employeeName(employee: Employee) {
  return String(employee.display_name || employee.agent_id || t("mission.default_employee"));
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
  if (tier === "workspace_write") return t("mission.autonomy_workspace_write");
  if (tier === "full_access") return t("mission.autonomy_full_access");
  return t("mission.autonomy_read_only");
}

function previewLabel(track: string) {
  if (track === "red") return t("mission.preview_red");
  if (track === "yellow") return t("mission.preview_yellow");
  return t("mission.preview_green");
}

function extractExecuteRunId(result: Record<string, unknown>): string {
  const direct = String(result.run_id || result.worker_run_id || "").trim();
  if (direct) return direct;
  const runs = Array.isArray(result.worker_runs) ? result.worker_runs : [];
  for (const item of runs) {
    if (!item || typeof item !== "object") continue;
    const runId = String((item as Record<string, unknown>).run_id || "").trim();
    if (runId) return runId;
  }
  return "";
}

function workOrderStatusRank(status: string): number {
  if (status === "Completed" || status === "Failed" || status === "Cancelled" || status === "Blocked") return 4;
  if (status === "WaitingApproval") return 3;
  if (status === "Running") return 2;
  if (status === "Planned") return 1;
  return 0;
}

function mergeWorkOrderStatus(current: string, next: unknown): string {
  const currentStatus = String(current || "").trim();
  const nextStatus = String(next || "").trim();
  if (!nextStatus) return currentStatus;
  if (!currentStatus) return nextStatus;
  return workOrderStatusRank(nextStatus) >= workOrderStatusRank(currentStatus)
    ? nextStatus
    : currentStatus;
}

function buildLocalSpans(verdict: GovernanceVerdict | undefined, cognitionText: string): TimelineSpan[] {
  return [
    {
      id: "span-intake",
      type: "BuildContext",
      label: t("mission.step_understand"),
      round: 0,
      durationMs: 180,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: { outcome: "allow" },
      output: t("mission.step_saved_local"),
    },
    {
      id: "span-model",
      type: "ModelCall",
      label: t("mission.step_summary"),
      round: 0,
      durationMs: 640,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: { outcome: "allow" },
      thought: cognitionText,
      proposal: cognitionText || t("mission.step_summary_waiting"),
      confidence: cognitionText ? 0.74 : 0.5,
      usage: { display: t("mission.step_usage_after_create") },
      output: cognitionText,
    },
    {
      id: "span-verdict",
      type: "SelectTool",
      label: t("mission.step_boundary"),
      round: 0,
      durationMs: 90,
      tokens: 0,
      costUsd: 0,
      trust: "native",
      gate: {
        outcome: verdict?.blocked ? "blocked" : verdict?.downgraded ? "need_approval" : "allow",
        reason: verdict?.blocked ? t("mission.needs_human") : verdict?.downgraded ? t("mission.auto_tightened") : t("mission.current_boundary_continue"),
      },
      overlays: verdict?.blocked ? ["sandbox_blocked"] : verdict?.downgraded ? ["need_approval"] : [],
      output: verdict ? `${t("mission.effective_permission")}${tierLabel(verdict.effective_tier)}` : t("mission.waiting_server_verdict"),
    },
  ];
}

export default function MissionChat() {
  const language = useLanguage();
  const navigate = useNavigate();
  const params = useParams();
  const routeConversationId = params.conversationId ? decodeURIComponent(params.conversationId) : "";
  const identity = useMemo(() => getLocalIdentity(), []);
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const stateKey = useMemo(() => missionStateKey(activeOpcId, identity.userId), [activeOpcId, identity.userId]);
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [creating, setCreating] = useState(false);
  const [lastWorkOrderId, setLastWorkOrderId] = useState("");
  const [activeRunId, setActiveRunId] = useState("");
  const [conversationId, setConversationId] = useState("");
  const [lastApprovalId, setLastApprovalId] = useState("");
  const [autonomy, setAutonomy] = useState<AutonomyCeiling>("read_only");
  const [modelPreference, setModelPreference] = useState<ModelPreference>("standard");
  const [assignedAgentId, setAssignedAgentId] = useState("auto");
  const [attachments, setAttachments] = useState<AttachmentMeta[]>([]);
  const [projectName, setProjectName] = useState("");
  const [knownProjects, setKnownProjects] = useState<string[]>([]);
  const [projectFolder, setProjectFolder] = useState("");
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [lastVerdict, setLastVerdict] = useState<GovernanceVerdict | undefined>();
  const [timelineSpans, setTimelineSpans] = useState<TimelineSpan[]>([]);
  const [lastWorkOrderStatus, setLastWorkOrderStatus] = useState("");
  const [slashDismissedQuery, setSlashDismissedQuery] = useState("");
  const [slashIndex, setSlashIndex] = useState(0);
  const { set: setGovernance } = useGovernance();

  const preview = useMemo(() => inferTrackFromIntent(input), [input]);
  const employeeGroups = useMemo(() => groupEmployees(employees), [employees]);
  const hasTask = Boolean(lastWorkOrderId || messages.length > 0);
  const slashMenuInput = input.trimStart();
  const slashMenuOpen = isSlashCommandInput(input) && slashMenuInput !== slashDismissedQuery;
  const slashCommands = useMemo(() => getSlashCommandMatches(input), [input]);

  useEffect(() => {
    setSlashIndex(0);
    if (slashDismissedQuery && slashMenuInput !== slashDismissedQuery) {
      setSlashDismissedQuery("");
    }
  }, [slashDismissedQuery, slashMenuInput]);

  useEffect(() => {
    listCompanyEmployees(activeOpcId).then(setEmployees).catch(() => setEmployees([]));
  }, [activeOpcId]);

  useEffect(() => {
    let cancelled = false;
    async function loadWorkOrderStatus() {
      if (!lastWorkOrderId) {
        if (!cancelled) setLastWorkOrderStatus("");
        return;
      }
      try {
        const rows = await listCompanyWorkOrders(activeOpcId);
        if (cancelled) return;
        const matched = rows.find((row) => String(row.work_order_id || "") === lastWorkOrderId);
        setLastWorkOrderStatus((current) => mergeWorkOrderStatus(current, matched?.status));
        const approvalId = String(matched?.approval_id || matched?.approval_receipt || "").trim();
        if (approvalId) setLastApprovalId(approvalId);
      } catch {
        // Keep the latest known status when the refresh fails transiently.
      }
    }
    void loadWorkOrderStatus();
    return () => {
      cancelled = true;
    };
  }, [activeOpcId, lastWorkOrderId]);

  useEffect(() => {
    getCompanyProfileById(activeOpcId)
      .then((profile) => {
        const projects = listField(profile as Record<string, unknown>, "active_projects");
        setKnownProjects(projects);
      })
      .catch(() => setKnownProjects([]));
  }, [activeOpcId]);

  useEffect(() => {
    try {
      const raw = localStorage.getItem(stateKey);
      if (!raw) return;
      const parsed = JSON.parse(raw) as {
        messages?: Msg[];
        last_work_order_id?: string;
        conversation_id?: string;
        last_approval_id?: string;
      };
      const restoredMessages = Array.isArray(parsed.messages) ? parsed.messages.filter((m) => typeof m?.text === "string") : [];
      if (restoredMessages.length > 0) setMessages(restoredMessages);
      if (typeof parsed.last_work_order_id === "string") setLastWorkOrderId(parsed.last_work_order_id);
      if (typeof parsed.conversation_id === "string") setConversationId(parsed.conversation_id);
      if (typeof parsed.last_approval_id === "string") setLastApprovalId(parsed.last_approval_id);
    } catch {
      // Ignore local snapshot parse failures.
    }
  }, [stateKey]);

  useEffect(() => {
    try {
      const persistentMessages = toPersistentMessages(messages);
      const hasState =
        persistentMessages.length > 0 ||
        Boolean(lastWorkOrderId.trim()) ||
        Boolean(conversationId.trim()) ||
        Boolean(lastApprovalId.trim());
      if (!hasState) {
        localStorage.removeItem(stateKey);
        return;
      }
      localStorage.setItem(stateKey, JSON.stringify({
        messages: persistentMessages,
        last_work_order_id: lastWorkOrderId,
        conversation_id: conversationId,
        last_approval_id: lastApprovalId,
      }));
    } catch {
      // Ignore local persistence failures.
    }
  }, [stateKey, messages, lastWorkOrderId, conversationId, lastApprovalId]);

  useEffect(() => {
    let cancelled = false;
    async function loadActiveConversation() {
      let active = "";
      try {
        active = routeConversationId || readActiveConversationId(activeOpcId, identity.userId) || "";
        if (routeConversationId) writeActiveConversationId(activeOpcId, identity.userId, routeConversationId);
      } catch {
        active = routeConversationId;
      }
      try {
        if (!active) {
          const threads = await listCompanyConversations(activeOpcId);
          active = String(threads[0]?.conversation_id || "");
          if (active) writeActiveConversationId(activeOpcId, identity.userId, active);
        }
        if (!active || cancelled) return;
        const rows = await listCompanyConversationMessages(activeOpcId, active);
        if (cancelled) return;
        setConversationId(active);
        const mapped = rows.map(toMsg).filter((m) => m.text);
        if (mapped.length > 0) setMessages(mapped);
        const linked = [...rows].reverse().find((row) => typeof row.linked_work_order_id === "string");
        if (linked?.linked_work_order_id) setLastWorkOrderId(String(linked.linked_work_order_id));
      } catch {
        if (!cancelled) setConversationId(active);
      }
    }
    loadActiveConversation();
    return () => {
      cancelled = true;
    };
  }, [activeOpcId, identity.userId, routeConversationId]);

  async function ensureConversation(text: string): Promise<string> {
    if (conversationId) return conversationId;
    if (routeConversationId) {
      setConversationId(routeConversationId);
      return routeConversationId;
    }
    // Always create a fresh conversation for a new chat — avoids stale localStorage references
    const created = await createCompanyConversation(activeOpcId, {
      opc_id: activeOpcId,
      user_id: identity.userId,
      title: conversationTitle(text),
    }) as Record<string, unknown>;
    const active = String(created.conversation_id || "");
    if (!active) throw new Error(t("mission.conversation_failed"));
    writeActiveConversationId(activeOpcId, identity.userId, active);
    setConversationId(active);
    return active;
  }

  function resetMissionView() {
    setConversationId("");
    setMessages([]);
    setLastWorkOrderId("");
    setActiveRunId("");
    setLastApprovalId("");
    setLastVerdict(undefined);
    setTimelineSpans([]);
  }

  function startFreshConversation() {
    clearMissionSession(activeOpcId, identity.userId);
    resetMissionView();
  }

  function appendSystemMessage(text: string) {
    setMessages((prev) => [...prev, { role: "system", text }]);
  }

  function buildStatusMessage() {
    const lines: string[] = [t("slash.status_title")];
    if (!lastWorkOrderId) {
      lines.push(t("slash.status_empty"));
    } else {
      lines.push(`${t("slash.status_work_order")}: ${lastWorkOrderId}`);
      lines.push(`${t("slash.status_value")}: ${lastWorkOrderStatus || t("slash.status_pending")}`);
      if (lastApprovalId) lines.push(`${t("slash.status_approval")}: ${lastApprovalId}`);
      if (activeRunId) lines.push(`${t("slash.status_run")}: ${activeRunId}`);
    }
    return lines.join("\n");
  }

  async function resolveCurrentWorkOrderId() {
    if (lastWorkOrderId) return lastWorkOrderId;
    try {
      const rows = await listCompanyWorkOrders(activeOpcId);
      return String(rows[0]?.work_order_id || "");
    } catch {
      return "";
    }
  }

  async function resolveCurrentApprovalId(workOrderId: string) {
    if (lastApprovalId) return lastApprovalId;
    try {
      const rows = await listCompanyWorkOrders(activeOpcId);
      const matched = rows.find((row) => String(row.work_order_id || "") === workOrderId);
      return String(matched?.approval_id || matched?.approval_receipt || "");
    } catch {
      return "";
    }
  }

  async function runCurrentWorkOrder() {
    const workOrderId = await resolveCurrentWorkOrderId();
    if (!workOrderId) {
      appendSystemMessage(t("slash.no_work_order"));
      return;
    }
    const result = await executeCompanyWorkOrder(activeOpcId, workOrderId, {}) as Record<string, unknown>;
    setLastWorkOrderId(workOrderId);
    setLastWorkOrderStatus(String(result.status || lastWorkOrderStatus || "Running").trim() || "Running");
    setActiveRunId(extractExecuteRunId(result) || activeRunId);
    const approvalId = String(result.approval_id || result.approval_receipt || "").trim();
    if (approvalId) setLastApprovalId(approvalId);
    appendSystemMessage(t("slash.run_done"));
  }

  async function decideCurrentWorkOrder(decision: "approve" | "reject", comment: string) {
    const workOrderId = await resolveCurrentWorkOrderId();
    if (!workOrderId) {
      appendSystemMessage(t("slash.no_work_order"));
      return;
    }
    const approvalId = await resolveCurrentApprovalId(workOrderId);
    if (!approvalId) {
      appendSystemMessage(t("slash.approval_missing"));
      return;
    }
    const result = await decideAndResume(activeOpcId, workOrderId, { approvalId, decision, comment });
    setLastWorkOrderId(workOrderId);
    if (decision === "approve") {
      setActiveRunId(result.runId || activeRunId);
      if (result.runId) setLastWorkOrderStatus(result.status || "Running");
    }
    appendSystemMessage(decision === "approve" ? t("slash.approved_done") : t("slash.rejected_done"));
  }

  async function cancelCurrentWorkOrder() {
    const workOrderId = await resolveCurrentWorkOrderId();
    if (!workOrderId) {
      appendSystemMessage(t("slash.no_work_order"));
      return;
    }
    await cancelCompanyWorkOrder(activeOpcId, workOrderId);
    setLastWorkOrderId(workOrderId);
    setLastWorkOrderStatus("Cancelled");
    appendSystemMessage(t("slash.cancelled_done"));
  }

  async function runSlashCommand(command: SlashCommandSpec, args: string) {
    setInput("");
    setSlashDismissedQuery("");
    setSlashIndex(0);

    if (command.name === "status") {
      appendSystemMessage(buildStatusMessage());
      return;
    }
    if (command.name === "help") {
      appendSystemMessage(buildSlashHelpMessage());
      return;
    }
    if (command.name === "clear") {
      startFreshConversation();
      appendSystemMessage(t("slash.cleared_done"));
      return;
    }
    if (command.name === "go") {
      const target = resolveSlashTarget(args);
      if (target) {
        navigate(target);
        appendSystemMessage(`${t("slash.opened")} ${target}`);
        return;
      }
      appendSystemMessage(`${t("slash.unknown_target")}: ${args || "/"}`);
      return;
    }
    if (command.name === "run") {
      await runCurrentWorkOrder();
      return;
    }
    if (command.name === "cancel") {
      await cancelCurrentWorkOrder();
      return;
    }
    if (command.name === "approve") {
      await decideCurrentWorkOrder("approve", args);
      return;
    }
    if (command.name === "reject") {
      await decideCurrentWorkOrder("reject", args);
      return;
    }
  }

  async function submitSlashInput(value: string) {
    const parsed = parseSlashCommandInput(value);
    if (!parsed.active) return false;
    const selected = parsed.command || (slashCommands.length > 0 ? slashCommands[Math.min(slashIndex, slashCommands.length - 1)] : null);
    if (!selected) {
      appendSystemMessage(`${t("slash.unknown_command")}: ${parsed.raw.trim() || "/"}`);
      setInput("");
      setSlashDismissedQuery("");
      setSlashIndex(0);
      return true;
    }
    const commandName = selected.name;
    const args = parsed.command ? parsed.args : parsed.raw.trimStart().slice(1).trimStart().slice(commandName.length).trimStart();
    await runSlashCommand(selected, args);
    return true;
  }

  useEffect(() => {
    function handleMissionSessionCleared(event: Event) {
      const detail = (event as CustomEvent<{ opcId?: string; userId?: string }>).detail;
      if (!detail) return;
      if (detail.opcId !== activeOpcId || detail.userId !== identity.userId) return;
      resetMissionView();
    }

    window.addEventListener(MISSION_SESSION_CLEARED_EVENT, handleMissionSessionCleared);
    return () => {
      window.removeEventListener(MISSION_SESSION_CLEARED_EVENT, handleMissionSessionCleared);
    };
  }, [activeOpcId, identity.userId]);

  async function send() {
    const text = input.trim();
    if (!canSubmitMission(text) || creating || isSlashCommandInput(text)) return;
    const taskIntent = buildTaskIntent(text, attachments, projectFolder, projectName);
    setInput("");
    setCreating(true);
    setActiveRunId("");
    setLastVerdict(undefined);
    setLastWorkOrderStatus("");
    setTimelineSpans([]);
    setMessages((prev) => [...prev, { role: "user", text }, { role: "system", text: t("mission.compiling") }]);

    let activeConversationId = conversationId;
    let modelUnavailable = false;
    try {
      activeConversationId = await ensureConversation(text);
      await appendCompanyConversationMessage(activeOpcId, activeConversationId, { role: "user", content: text });
      const bootstrap = await ensureWorkspaceDefaults();
      const selectedAgentIds = assignedAgentId === "auto" ? bootstrap.selectedAgentIds : [assignedAgentId];
      setMessages((prev) => [...prev, { role: "system", text: t("mission.progress_compile") }]);
      const compiled = await compileContract(taskIntent, "DRAFT");
      const contract = compiled.contract;
      const contractHash = compiled.contract_hash;
      setMessages((prev) => [...prev, { role: "system", text: t("mission.progress_route") }]);
      const routed = await routePlan(contract, selectedAgentIds, contractHash) as { plan_hash?: string };
      const planHash = String(routed.plan_hash || "");
      let cognitionError = "";
      setMessages((prev) => [...prev, { role: "system", text: t("mission.progress_model") }]);
      const cognition = await modelChat({
        role: "MissionDraft",
        messages: [
          {
            role: "system",
            content: language === "zh"
              ? "用面向客户的中文给出最多 5 条短清单，每条不超过 18 个字，句子必须完整。不要提到授权、Track、WorkOrder、沙箱、治理网关或内部实现。"
              : "Return at most 5 short checklist bullets for the founder, each under 18 words and complete. Do not mention authorization, Track, WorkOrder, sandbox, governance gateway, or internal implementation.",
          },
          { role: "user", content: taskIntent },
        ],
        temperature: 0.2,
        max_tokens: 420,
      }).catch((e: unknown) => {
        cognitionError = e instanceof Error ? e.message : String(e);
        modelUnavailable = true;
        return null;
      }) as Record<string, unknown> | null;
      const cognitionText = publicText(String(cognition?.content || "").trim());
      setMessages((prev) => [...prev, { role: "system", text: t("mission.progress_workorder") }]);
      const created = await createCompanyWorkOrder(activeOpcId, {
        conversation_id: activeConversationId,
        contract_hash: contractHash,
        plan_hash: planHash,
        user_id: identity.userId,
        opc_id: activeOpcId,
        mission_intent: taskIntent,
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
      const approvalId = String(created.approval_id || created.approval_receipt || "").trim();
      const verdict = created.governance_verdict as GovernanceVerdict | undefined;
      const createdStatus = String(created.status || "").trim();
      const systemMessages: Msg[] = [
        ...(cognitionText ? [{ role: "system" as const, text: cognitionText }] : []),
        ...(!cognitionText && cognitionError ? [{ role: "system" as const, text: `${t("mission.task_created_model_unavailable")}: ${publicText(cognitionError)}` }] : []),
        { role: "system", text: t("mission.created_review") },
      ];
      for (const msg of systemMessages) {
        await appendCompanyConversationMessage(activeOpcId, activeConversationId, {
          role: "assistant",
          content: msg.text,
          linked_work_order_id: msg.text === t("mission.created_review") ? workOrderId : undefined,
        });
      }
      setLastWorkOrderId(workOrderId);
      if (approvalId) setLastApprovalId(approvalId);
      setLastWorkOrderStatus(createdStatus);
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
      setAttachments([]);
      setProjectFolder("");

      // Auto-execute green-track tasks and wire streaming
      if (verdict && !verdict.blocked && verdict.effective_track === "green" && workOrderId) {
        try {
          const execResult = await executeCompanyWorkOrder(activeOpcId, workOrderId, {}) as Record<string, unknown>;
          const runId = extractExecuteRunId(execResult);
          const executeStatus = String(execResult.status || "").trim();
          if (runId) setActiveRunId(runId);
          if (executeStatus) setLastWorkOrderStatus(executeStatus);
        } catch {
          // Execution may not be ready — user can still trigger from the task page
        }
      }
    } catch (e: unknown) {
      const msg = publicText(e instanceof Error ? e.message : String(e));
      const userFacing = modelUnavailable
        ? `${t("mission.task_not_created_model_unavailable")}: ${msg}`
        : `${t("mission.create_problem")}${msg}`;
      if (activeConversationId) {
        await appendCompanyConversationMessage(activeOpcId, activeConversationId, {
          role: "assistant",
          content: userFacing,
        }).catch(() => undefined);
      }
      setMessages((prev) => [...prev, { role: "system", text: userFacing }]);
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
                <h1 className="mission-title">{t("mission.hero_title")}</h1>
                <p className="mission-subtitle">
                  {t("mission.hero_desc")}
                </p>
                <div className="mission-cards">
                  <Link to="/work-orders" className="mission-card mission-card-link">
                    <div className="mission-card-title">{t("mission.card_progress_title")}</div>
                    <div className="mission-card-text">{t("mission.card_progress_desc")}</div>
                  </Link>
                  <Link to="/company" className="mission-card mission-card-link">
                    <div className="mission-card-title">{t("mission.card_company_title")}</div>
                    <div className="mission-card-text">{t("mission.card_company_desc")}</div>
                  </Link>
                  <Link to="/projects" className="mission-card mission-card-link">
                    <div className="mission-card-title">{t("mission.card_projects_title")}</div>
                    <div className="mission-card-text">{t("mission.card_projects_desc")}</div>
                  </Link>
                </div>
              </div>
            ) : (
              <>
                {lastVerdict && (
                  <div className="mb-4">
                    <span className={`verdict-chip ${lastVerdict.blocked ? "blocked" : ""}`}>
                      <strong>{t("mission.verdict")}</strong>
                      <span>{lastVerdict.blocked ? t("mission.needs_human") : `${t("mission.effective_permission")}${tierLabel(lastVerdict.effective_tier)}`}</span>
                      {lastVerdict.downgraded && <span>{t("mission.auto_tightened")}</span>}
                    </span>
                  </div>
                )}
                {messages.map((message, index) => (
                  <div key={`${message.role}-${index}`} className={`message-row ${message.role}`}>
                    <div className={`chat-msg ${message.role}`}>
                      <div className="message-author">{message.role === "user" ? t("mission.user") : t("mission.system")}</div>
                      <div>{message.text}</div>
                    </div>
                  </div>
                ))}
                {activeRunId && (
                  <div className="mission-stream-section">
                    <WorkerStreamView runId={activeRunId} />
                  </div>
                )}
                {!creating && lastWorkOrderId && (
                  <TaskNextStep workOrderId={lastWorkOrderId} verdict={lastVerdict} status={lastWorkOrderStatus} />
                )}
              </>
            )}
          </div>
          <Composer
            input={input}
            creating={creating}
            onNewChat={startFreshConversation}
            slashMenuOpen={slashMenuOpen}
            slashCommands={slashCommands}
            slashIndex={slashIndex}
            autonomy={autonomy}
            modelPreference={modelPreference}
            assignedAgentId={assignedAgentId}
            attachments={attachments}
            projectName={projectName}
            knownProjects={knownProjects}
            projectFolder={projectFolder}
            employeeGroups={employeeGroups}
            previewText={input.trim() ? previewLabel(preview.track) : t("mission.preview_waiting")}
            onInput={setInput}
            onAutonomy={setAutonomy}
            onModelPreference={setModelPreference}
            onAssignedAgent={setAssignedAgentId}
            onAttachments={setAttachments}
            onProjectName={setProjectName}
            onProjectFolder={setProjectFolder}
            onSend={send}
            onSlashIndex={setSlashIndex}
            onSlashDismiss={(value) => setSlashDismissedQuery(value)}
            onSlashExecute={runSlashCommand}
            onSlashUnknown={(raw) => {
              appendSystemMessage(`${t("slash.unknown_command")}: ${raw || "/"}`);
              setInput("");
              setSlashDismissedQuery("");
              setSlashIndex(0);
            }}
          />
        </section>
        {hasTask && (
          <aside className="mission-right">
            <GovernanceTimeline spans={timelineSpans} title={t("mission.timeline_title")} />
          </aside>
        )}
      </div>
    </div>
  );
}

function TaskNextStep({
  workOrderId,
  verdict,
  status,
}: {
  workOrderId: string;
  verdict?: GovernanceVerdict;
  status?: string;
}) {
  const blocked = Boolean(verdict?.blocked || verdict?.effective_track === "red");
  const needsConfirmation = Boolean(!blocked && verdict?.effective_track === "yellow");
  const completed = status === "Completed";
  const running = status === "Running";
  const title = completed
    ? t("tasks.next_result")
    : running
      ? t("workorders.next_running")
      : blocked
    ? t("mission.next_blocked_title")
    : needsConfirmation
      ? t("mission.next_confirm_title")
      : t("mission.next_ready_title");
  const desc = completed
    ? t("mission.next_result_desc")
    : running
      ? t("mission.next_running_desc")
      : blocked
    ? t("mission.next_blocked_desc")
    : needsConfirmation
      ? t("mission.next_confirm_desc")
      : t("mission.next_ready_desc");
  return (
    <div className="task-next-card">
      <div>
        <div className="task-next-title">{title}</div>
        <div className="task-next-desc">{desc}</div>
      </div>
      <div className="task-next-actions">
        <Link to={`/tasks/${encodeURIComponent(workOrderId)}`} className="product-link-button">{t("mission.open_task")}</Link>
        {completed ? (
          <Link to={`/tasks/${encodeURIComponent(workOrderId)}`} className="primary-button product-action">
            {t("workorders.view_result")}
          </Link>
        ) : !blocked && (
          <Link to="/work-orders" className="primary-button product-action">
            {needsConfirmation ? t("mission.confirm_task") : t("mission.start_task")}
          </Link>
        )}
      </div>
    </div>
  );
}

function Composer({
  input,
  creating,
  onNewChat,
  slashMenuOpen,
  slashCommands,
  slashIndex,
  autonomy,
  modelPreference,
  assignedAgentId,
  attachments,
  projectName,
  knownProjects,
  projectFolder,
  employeeGroups,
  previewText,
  onInput,
  onAutonomy,
  onModelPreference,
  onAssignedAgent,
  onAttachments,
  onProjectName,
  onProjectFolder,
  onSend,
  onSlashIndex,
  onSlashDismiss,
  onSlashExecute,
  onSlashUnknown,
}: {
  input: string;
  creating: boolean;
  onNewChat: () => void;
  slashMenuOpen: boolean;
  slashCommands: SlashCommandSpec[];
  slashIndex: number;
  autonomy: AutonomyCeiling;
  modelPreference: ModelPreference;
  assignedAgentId: string;
  attachments: AttachmentMeta[];
  projectName: string;
  knownProjects: string[];
  projectFolder: string;
  employeeGroups: Record<string, Employee[]>;
  previewText: string;
  onInput: (value: string) => void;
  onAutonomy: (value: AutonomyCeiling) => void;
  onModelPreference: (value: ModelPreference) => void;
  onAssignedAgent: (value: string) => void;
  onAttachments: (value: AttachmentMeta[]) => void;
  onProjectName: (value: string) => void;
  onProjectFolder: (value: string) => void;
  onSend: () => void;
  onSlashIndex: (value: number) => void;
  onSlashDismiss: (value: string) => void;
  onSlashExecute: (command: SlashCommandSpec, args: string) => Promise<void>;
  onSlashUnknown: (raw: string) => void;
}) {
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const folderInputRef = useRef<HTMLInputElement | null>(null);

  async function chooseProjectFolder() {
    const invoke = getTauriInvoke();
    if (invoke) {
      try {
        const selected = await invoke<string | null>("choose_project_folder");
        if (selected) {
          onProjectFolder(selected);
          return;
        }
      } catch {
        // Fall back to the browser directory input in web/dev mode.
      }
    }
    folderInputRef.current?.click();
  }

  function resolveSlashArgs(command: SlashCommandSpec) {
    const parsed = parseSlashCommandInput(input);
    if (parsed.command?.name === command.name) return parsed.args;
    const raw = input.trimStart().slice(1).trimStart();
    if (!raw.toLowerCase().startsWith(command.name)) return "";
    return raw.slice(command.name.length).trimStart();
  }

  return (
    <div className="composer-wrap">
      <div className="composer">
        <textarea
          className="composer-textarea"
          placeholder={t("mission.placeholder")}
          value={input}
          onChange={(event) => onInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape" && slashMenuOpen) {
              event.preventDefault();
              onSlashDismiss(input.trimStart());
              return;
            }
            if (slashMenuOpen && event.key === "ArrowDown") {
              event.preventDefault();
              onSlashIndex(Math.min(slashIndex + 1, Math.max(slashCommands.length - 1, 0)));
              return;
            }
            if (slashMenuOpen && event.key === "ArrowUp") {
              event.preventDefault();
              onSlashIndex(Math.max(slashIndex - 1, 0));
              return;
            }
            if (isSlashCommandInput(input) && event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              const parsed = parseSlashCommandInput(input);
              const command = parsed.command || slashCommands[Math.min(slashIndex, Math.max(slashCommands.length - 1, 0))];
              if (!command) {
                onSlashUnknown(input.trimStart());
                return;
              }
              void onSlashExecute(command, resolveSlashArgs(command));
              return;
            }
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              onSend();
            }
          }}
        />
        <SlashCommandMenu
          open={slashMenuOpen}
          query={input.trimStart()}
          commands={slashCommands}
          activeIndex={slashIndex}
          onPick={(command) => {
            void onSlashExecute(command, resolveSlashArgs(command));
          }}
          onClose={() => onSlashDismiss(input.trimStart())}
        />
        <div className="composer-bar">
          <input
            ref={attachmentInputRef}
            aria-hidden="true"
            className="sr-only"
            type="file"
            multiple
            onChange={(event) => {
              const files = Array.from(event.target.files || []);
              onAttachments(files.map((file) => ({ name: file.name, size: file.size, type: file.type || "" })));
            }}
          />
          <input
            ref={folderInputRef}
            aria-hidden="true"
            className="sr-only"
            type="file"
            multiple
            {...{ webkitdirectory: "", directory: "" }}
            onChange={(event) => {
              const file = event.target.files?.[0];
              const relPath = String((file as File & { webkitRelativePath?: string } | undefined)?.webkitRelativePath || file?.name || "");
              onProjectFolder(relPath ? relPath.split("/")[0] : "");
            }}
          />
          <button type="button" className="icon-button" aria-label={t("mission.add_attachment")} title={t("mission.add_attachment")} onClick={() => attachmentInputRef.current?.click()}>
            <Icon name="plus" />
          </button>
          <button type="button" className="icon-button" aria-label={t("nav.new_chat")} title={t("nav.new_chat")} onClick={onNewChat}>
            <Icon name="sparkles" />
          </button>
          <button type="button" className="icon-button" aria-label={t("mission.choose_folder")} title={t("mission.choose_folder")} onClick={chooseProjectFolder}>
            <Icon name="folder" />
          </button>
          <select className="select-control model-select" aria-label={t("mission.model")} value={modelPreference} onChange={(event) => onModelPreference(event.target.value as ModelPreference)}>
            {modelOptions.map((option) => <option key={option} value={option}>{t(`mission.model_${option}`)}</option>)}
          </select>
          <details className="composer-options">
            <summary>{t("mission.task_options")}</summary>
            <div className="composer-options-panel">
              <select className="select-control" aria-label={t("mission.autonomy")} value={autonomy} onChange={(event) => onAutonomy(event.target.value as AutonomyCeiling)}>
                {autonomyOptions.map((option) => <option key={option} value={option}>{tierLabel(option)}</option>)}
              </select>
              <select className="select-control" aria-label={t("mission.assign_employee")} value={assignedAgentId} onChange={(event) => onAssignedAgent(event.target.value)}>
                <option value="auto">{t("mission.auto_assign")}</option>
                {Object.entries(employeeGroups).map(([department, employees]) => (
                  <optgroup key={department} label={department}>
                    {employees.map((employee) => {
                      const id = String(employee.agent_id || "");
                      return <option key={id} value={id}>{employeeName(employee)}</option>;
                    })}
                  </optgroup>
                ))}
              </select>
              <select className="select-control project-select" aria-label={t("mission.project")} value={projectName} onChange={(event) => onProjectName(event.target.value)}>
                <option value="">{t("mission.no_project")}</option>
                {knownProjects.map((project) => <option key={project} value={project}>{project}</option>)}
              </select>
            </div>
          </details>
          {(attachments.length > 0 || projectFolder || projectName) && (
            <span className="intent-preview context-chips">
              {projectName && <span className="context-chip">{t("mission.project")}: {projectName}</span>}
              {attachments.length > 0 && attachments.map((file) => (
                <span key={`${file.name}-${file.size}`} className="context-chip">{file.name}</span>
              ))}
              {projectFolder && <span className="context-chip">{t("mission.folder_selected")}: {projectFolder}</span>}
            </span>
          )}
          <span className="intent-preview">{previewText}</span>
          {creating && (
            <span className="streaming-indicator" aria-live="polite">
              <Icon name="spinner" className="icon-spin" />
              {t("mission.streaming")}
            </span>
          )}
          <button type="button" disabled={creating || !canSubmitMission(input) || isSlashCommandInput(input)} className="primary-button" onClick={onSend}>
            {creating ? t("mission.creating") : t("mission.send")}
          </button>
        </div>
      </div>
    </div>
  );
}
