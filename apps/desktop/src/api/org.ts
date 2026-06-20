import {
  get,
  post,
  put,
  getAgentGrowth,
  approveSkillProposal,
} from "./client";

const companyPath = (opcId: string) => `/companies/${encodeURIComponent(opcId)}`;
const companyOptions = (opcId: string) => ({ opcId });

type Row = Record<string, unknown>;

function isRecord(value: unknown): value is Row {
  return typeof value === "object" && value !== null;
}

function asArray<T>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}

function stringValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : value == null ? fallback : String(value);
}

function numberValue(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export type MeetingSummary = {
  meeting_id: string;
  topic: string;
  status: string;
  created_at_ms: number;
};

export type MeetingTurn = {
  agent_id: string;
  stance: string;
  text: string;
};

export type MeetingDetail = {
  meeting_id: string;
  topic: string;
  status: string;
  agenda: string;
  transcript: MeetingTurn[];
  resolution_md: string;
  responsibility_anchor: string;
  created_at_ms: number;
};

export type KpiRecord = {
  work_order_id: string;
  scores: Record<string, number>;
  reviewer: string;
  comment?: string;
  created_at_ms: number;
};

export type ReportSummary = {
  report_id: string;
  period: string;
  created_at_ms: number;
};

export type ReportAlert = {
  severity: "info" | "warning" | "critical";
  message: string;
};

export type ReportDetail = {
  report_id: string;
  period: string;
  report_md: string;
  kpi_summary: Array<{ department: string; score: number }>;
  token_usage: Array<{ department: string; tokens: number; cost_usd: number }>;
  alerts: ReportAlert[];
  created_at_ms: number;
};

export type CostDepartment = {
  dept: string;
  tokens: number;
  cost_usd: number;
  quota?: number;
};

export type CostOverview = {
  by_department: CostDepartment[];
  total: number;
};

export type CompanyAuditEvent = {
  id: string;
  event_type: string;
  contract_hash?: string | null;
  agent_id?: string | null;
  traceparent?: string | null;
  tenant_id: string;
  event_data_json: string;
  recorded_at_ms: number;
};

export async function listCompanyEmployees(opcId: string): Promise<Row[]> {
  return asArray<Row>(await get<Row[]>(`${companyPath(opcId)}/employees`, companyOptions(opcId)));
}

export async function listCompanySkills(opcId: string): Promise<Row[]> {
  try {
    return asArray<Row>(await get<Row[]>(`${companyPath(opcId)}/skills`, companyOptions(opcId)));
  } catch {
    return [];
  }
}

export async function getCompanyEmployee(
  opcId: string,
  agentId: string,
): Promise<Row | null> {
  try {
    const detail = await get<Row>(`${companyPath(opcId)}/employees/${encodeURIComponent(agentId)}`, companyOptions(opcId));
    return isRecord(detail) ? detail : null;
  } catch {
    return null;
  }
}

export async function getCompanyProfileById(opcId: string): Promise<Row> {
  const detail = await get<Row>(`${companyPath(opcId)}/profile/company`, companyOptions(opcId));
  return isRecord(detail) ? detail : {};
}

export async function listCompanyConversations(opcId: string): Promise<Row[]> {
  return asArray<Row>(await get<Row[]>(`${companyPath(opcId)}/conversations`, companyOptions(opcId)));
}

export async function createCompanyConversation(
  opcId: string,
  payload: Row,
): Promise<Row> {
  const created = await post<Row>(`${companyPath(opcId)}/conversations`, payload, companyOptions(opcId));
  return isRecord(created) ? created : {};
}

export async function listCompanyConversationMessages(
  opcId: string,
  conversationId: string,
): Promise<Row[]> {
  return asArray<Row>(
    await get<Row[]>(
      `${companyPath(opcId)}/conversations/${encodeURIComponent(conversationId)}/messages`,
      companyOptions(opcId),
    ),
  );
}

export async function appendCompanyConversationMessage(
  opcId: string,
  conversationId: string,
  payload: Row,
): Promise<Row> {
  const appended = await post<Row>(
    `${companyPath(opcId)}/conversations/${encodeURIComponent(conversationId)}/messages`,
    payload,
    companyOptions(opcId),
  );
  return isRecord(appended) ? appended : {};
}

export async function createCompanyWorkOrder(opcId: string, payload: Row): Promise<Row> {
  const created = await post<Row>(`${companyPath(opcId)}/work-orders`, payload, companyOptions(opcId));
  return isRecord(created) ? created : {};
}

export type DispatchSubtask = {
  department: string;
  assignee_agent_id: string;
  goal: string;
  rationale: string;
};

export type DispatchPlan = {
  understanding: string;
  subtasks: DispatchSubtask[];
  model_backed: boolean;
  secretary_agent_id: string;
};

/**
 * Ask the company secretary (intelligent dispatcher) to understand the founder's intent
 * and propose which department head(s) should handle it. Returns null if the endpoint is
 * unavailable, so the caller can fall back to the plain compile flow.
 */
export async function dispatchPlan(opcId: string, intent: string): Promise<DispatchPlan | null> {
  try {
    const plan = await post<DispatchPlan>(`${companyPath(opcId)}/dispatch`, { intent }, companyOptions(opcId));
    return plan && Array.isArray(plan.subtasks) ? plan : null;
  } catch {
    return null;
  }
}

export async function listMeetings(opcId: string): Promise<MeetingSummary[]> {
  const rows = await get<MeetingSummary[]>(`${companyPath(opcId)}/meetings`, companyOptions(opcId));
  return asArray<MeetingSummary>(rows);
}

export async function getMeeting(opcId: string, meetingId: string): Promise<MeetingDetail | null> {
  const detail = await get<MeetingDetail>(`${companyPath(opcId)}/meetings/${encodeURIComponent(meetingId)}`, companyOptions(opcId));
  return detail && detail.meeting_id ? detail : null;
}

export async function startMeeting(
  opcId: string,
  req: { topic: string; participants: string[]; close_mode: "vote" | "chair" },
): Promise<{ meeting_id: string; status: string }> {
  return post<{ meeting_id: string; status: string }>(`${companyPath(opcId)}/meetings`, req, companyOptions(opcId));
}

export async function listKpi(opcId: string, agentId: string): Promise<KpiRecord[]> {
  const rows = await get<KpiRecord[]>(
    `${companyPath(opcId)}/employees/${encodeURIComponent(agentId)}/kpi`,
    companyOptions(opcId),
  );
  return asArray<KpiRecord>(rows);
}

export async function listReports(opcId: string): Promise<ReportSummary[]> {
  const rows = await get<ReportSummary[]>(`${companyPath(opcId)}/reports`, companyOptions(opcId));
  return asArray<ReportSummary>(rows);
}

export async function getReport(opcId: string, reportId: string): Promise<ReportDetail | null> {
  const detail = await get<ReportDetail>(
    `${companyPath(opcId)}/reports/${encodeURIComponent(reportId)}`,
    companyOptions(opcId),
  );
  return detail && detail.report_id ? detail : null;
}

export async function generateReport(
  opcId: string,
  period: "daily" | "monthly",
): Promise<{ report_id: string }> {
  return post<{ report_id: string }>(`${companyPath(opcId)}/reports/generate`, { period }, companyOptions(opcId));
}

export async function getCost(opcId: string): Promise<CostOverview> {
  const res = await get<CostOverview>(`${companyPath(opcId)}/cost`, companyOptions(opcId));
  return res && Array.isArray(res.by_department) ? res : { by_department: [], total: 0 };
}

export async function setCostQuota(
  opcId: string,
  department: string,
  tokenQuota: number,
): Promise<{ ok: boolean }> {
  return put<{ ok: boolean }>(`${companyPath(opcId)}/cost/quota`, {
    department,
    token_quota: tokenQuota,
  }, companyOptions(opcId));
}

export async function listCompanyWorkOrders(opcId: string): Promise<Row[]> {
  return asArray<Row>(await get<Row[]>(`${companyPath(opcId)}/work-orders`, companyOptions(opcId)));
}

export async function executeCompanyWorkOrder(
  opcId: string,
  workOrderId: string,
  payload: Row = {},
): Promise<Row> {
  const result = await post<Row>(
    `${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/execute`,
    payload,
    companyOptions(opcId),
  );
  return isRecord(result) ? result : {};
}

export async function decideCompanyWorkOrderApproval(
  opcId: string,
  workOrderId: string,
  payload: { approval_id: string; decision: "approve" | "reject"; comment?: string },
): Promise<Row> {
  const result = await post<Row>(
    `${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/approval`,
    payload,
    companyOptions(opcId),
  );
  return isRecord(result) ? result : {};
}

export async function cancelCompanyWorkOrder(opcId: string, workOrderId: string): Promise<Row> {
  const result = await post<Row>(
    `${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/cancel`,
    {},
    companyOptions(opcId),
  );
  return isRecord(result) ? result : {};
}

export async function submitCompanyWorkOrderFeedback(
  opcId: string,
  workOrderId: string,
  feedback: string,
  agentId?: string,
): Promise<Row> {
  const payload = { feedback, agent_id: agentId };
  const result = await post<Row>(
    `${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/feedback`,
    payload,
    companyOptions(opcId),
  );
  return isRecord(result) ? result : {};
}

export async function getCompanyWorkOrderTimeline(opcId: string, workOrderId: string): Promise<Row[]> {
  try {
    return asArray<Row>(
      await get<Row[]>(`${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/timeline`, companyOptions(opcId)),
    );
  } catch {
    return [];
  }
}

export async function getCompanyWorkOrderAuditExport(
  opcId: string,
  workOrderId: string,
): Promise<Row> {
  const result = await get<Row>(
    `${companyPath(opcId)}/work-orders/${encodeURIComponent(workOrderId)}/audit-export`,
    companyOptions(opcId),
  );
  return isRecord(result) ? result : {};
}

export async function listCompanyAuditEvents(
  opcId: string,
  options?: { limit?: number; workOrderId?: string; runId?: string },
): Promise<CompanyAuditEvent[]> {
  const params = new URLSearchParams();
  if (options?.limit != null) params.set("limit", String(options.limit));
  if (options?.workOrderId) params.set("work_order_id", options.workOrderId);
  if (options?.runId) params.set("run_id", options.runId);
  const suffix = params.size > 0 ? `?${params.toString()}` : "";

  return asArray<CompanyAuditEvent>(
    await get<CompanyAuditEvent[]>(`${companyPath(opcId)}/audit${suffix}`, companyOptions(opcId)),
  );
}

export async function listCompanyMemory(opcId: string): Promise<Row[]> {
  return asArray<Row>(await get<Row[]>(`${companyPath(opcId)}/memory`, companyOptions(opcId)));
}

export async function getCompanyGrowth(agentId: string) {
  return getAgentGrowth(agentId);
}

export async function approveCompanyImprovement(proposalId: string) {
  return approveSkillProposal(proposalId);
}

export function normalizeMeetingSummary(row: Row): MeetingSummary {
  return {
    meeting_id: stringValue(row.meeting_id),
    topic: stringValue(row.topic),
    status: stringValue(row.status),
    created_at_ms: numberValue(row.created_at_ms),
  };
}
