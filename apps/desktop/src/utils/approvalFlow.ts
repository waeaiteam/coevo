import {
  decideCompanyWorkOrderApproval,
  executeCompanyWorkOrder,
} from "../api/org";

type Row = Record<string, unknown>;

/**
 * Shared "approve → resume → stream" logic for Yellow Track work orders.
 *
 * The server contract (apps/server/src/handlers/opc.rs):
 *  - executeCompanyWorkOrder on a Yellow task with no receipt returns
 *    `{ status: "WaitingApproval", approval_id, approval_mode: "NEGATIVE_CONSENT" }`.
 *  - decideCompanyWorkOrderApproval(approve) records the approval and returns an
 *    `approval_receipt`; the client then calls executeCompanyWorkOrder with that
 *    receipt so approval and execution stay separate operations.
 *
 * Both MissionChat (inline approval card) and WorkOrders use these helpers so the two
 * surfaces never diverge.
 */

/** Pull the run id out of an execute/approval payload (direct field or worker_runs[]). */
export function extractRunId(result: Row): string {
  const direct = String(result.run_id || result.worker_run_id || "").trim();
  if (direct) return direct;
  const runs = Array.isArray(result.worker_runs) ? result.worker_runs : [];
  for (const item of runs) {
    if (!item || typeof item !== "object") continue;
    const runId = String((item as Row).run_id || "").trim();
    if (runId) return runId;
  }
  return "";
}

/** Pull the approval id out of an execute payload. */
export function extractApprovalId(result: Row): string {
  return String(result.approval_id || result.approval_receipt || "").trim();
}

export type ApprovalRequest = {
  /** approval id created server-side when execute is first called on a Yellow task */
  approvalId: string;
  /** status returned by the execute call (typically "WaitingApproval") */
  status: string;
  /** raw payload for callers that need more fields */
  payload: Row;
};

/**
 * Kick off the approval for a Yellow task by executing it once. The server creates the
 * approval receipt and returns its id. Returns an empty approvalId if the server did not
 * produce one (e.g. the task auto-resolved or the endpoint is unavailable).
 */
export async function requestApproval(
  opcId: string,
  workOrderId: string,
): Promise<ApprovalRequest> {
  const payload = (await executeCompanyWorkOrder(opcId, workOrderId, {})) as Row;
  return {
    approvalId: extractApprovalId(payload),
    status: String(payload.status || "").trim(),
    payload,
  };
}

export type ApprovalResume = {
  /** terminal-ish status string returned by the decide call */
  status: string;
  /** run id to stream when the decision resumed execution (approve only) */
  runId: string;
  /** raw decide payload */
  payload: Row;
};

/**
 * Record an approval decision. On "approve", resume execution in a second API call
 * using the returned approval receipt. On "reject" no run is produced.
 */
export async function decideAndResume(
  opcId: string,
  workOrderId: string,
  options: { approvalId: string; decision: "approve" | "reject"; comment?: string },
): Promise<ApprovalResume> {
  const decisionPayload = (await decideCompanyWorkOrderApproval(opcId, workOrderId, {
    approval_id: options.approvalId,
    decision: options.decision,
    comment: options.comment,
  })) as Row;

  if (options.decision !== "approve") {
    return {
      status: String(decisionPayload.status || "").trim(),
      runId: "",
      payload: decisionPayload,
    };
  }

  const approvalReceipt = extractApprovalId(decisionPayload);
  if (!approvalReceipt) {
    return {
      status: String(decisionPayload.status || "").trim(),
      runId: "",
      payload: decisionPayload,
    };
  }

  const executePayload = (await executeCompanyWorkOrder(opcId, workOrderId, {
    caller_identity_proof: approvalReceipt,
  })) as Row;
  return {
    status: String(executePayload.status || decisionPayload.status || "").trim(),
    runId: extractRunId(executePayload),
    payload: {
      ...decisionPayload,
      ...executePayload,
      approval_receipt: approvalReceipt,
    },
  };
}
