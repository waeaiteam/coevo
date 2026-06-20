import { beforeEach, describe, expect, it, vi } from "vitest";

const org = vi.hoisted(() => ({
  decideCompanyWorkOrderApproval: vi.fn(),
  executeCompanyWorkOrder: vi.fn(),
}));

vi.mock("../api/org", () => ({
  decideCompanyWorkOrderApproval: org.decideCompanyWorkOrderApproval,
  executeCompanyWorkOrder: org.executeCompanyWorkOrder,
}));

describe("approvalFlow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("approves first and then resumes execution with the returned approval receipt", async () => {
    org.decideCompanyWorkOrderApproval.mockResolvedValue({
      ok: true,
      status: "ApprovalApproved",
      approval_receipt: "approval-123:digest-abc",
      next_action: "execute_work_order",
    });
    org.executeCompanyWorkOrder.mockResolvedValue({
      ok: true,
      status: "Running",
      run_id: "run-after-approval",
    });

    const { decideAndResume } = await import("../utils/approvalFlow");
    const result = await decideAndResume("opc-alpha", "wo-1", {
      approvalId: "approval-123",
      decision: "approve",
      comment: "ship it",
    });

    expect(org.decideCompanyWorkOrderApproval).toHaveBeenCalledWith("opc-alpha", "wo-1", {
      approval_id: "approval-123",
      decision: "approve",
      comment: "ship it",
    });
    expect(org.executeCompanyWorkOrder).toHaveBeenCalledWith("opc-alpha", "wo-1", {
      caller_identity_proof: "approval-123:digest-abc",
    });
    expect(result.runId).toBe("run-after-approval");
    expect(result.status).toBe("Running");
  });

  it("does not execute when the decision rejects the approval", async () => {
    org.decideCompanyWorkOrderApproval.mockResolvedValue({
      ok: true,
      status: "ApprovalDenied",
      approval_receipt: "approval-123",
    });

    const { decideAndResume } = await import("../utils/approvalFlow");
    const result = await decideAndResume("opc-alpha", "wo-1", {
      approvalId: "approval-123",
      decision: "reject",
    });

    expect(org.executeCompanyWorkOrder).not.toHaveBeenCalled();
    expect(result.runId).toBe("");
    expect(result.status).toBe("ApprovalDenied");
  });
});