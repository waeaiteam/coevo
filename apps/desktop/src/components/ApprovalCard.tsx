import { useState } from "react";
import Icon from "./Icon";
import { t } from "../settings/i18n";

export type ApprovalCardState = "pending" | "deciding" | "approved" | "rejected";

/**
 * Inline approval card rendered in the MissionChat thread for Yellow Track tasks.
 * Lets the user approve or reject a pending decision without leaving the chat.
 * Presentational: parent owns the decision flow (see utils/approvalFlow.ts).
 */
export function ApprovalCard({
  track,
  riskSummary,
  state,
  onApprove,
  onReject,
}: {
  track: "green" | "yellow" | "red";
  riskSummary?: string;
  state: ApprovalCardState;
  onApprove: (comment: string) => void;
  onReject: (comment: string) => void;
}) {
  const [comment, setComment] = useState("");
  const busy = state === "deciding";
  const resolved = state === "approved" || state === "rejected";

  return (
    <div className="approval-card" role="group" aria-label={t("approval.title")}>
      <div className="approval-head">
        <span className={`product-pill ${track}`}>{t(`approval.track_${track}`)}</span>
        <span className="approval-title">{t("approval.title")}</span>
      </div>
      <div className="approval-summary">{riskSummary || t("approval.default_summary")}</div>

      {state === "approved" && (
        <div className="approval-status approved">
          <Icon name="check" />
          <span>{t("approval.approved_notice")}</span>
        </div>
      )}
      {state === "rejected" && (
        <div className="approval-status rejected">
          <Icon name="x" />
          <span>{t("approval.rejected_notice")}</span>
        </div>
      )}

      {!resolved && (
        <>
          <textarea
            className="approval-comment"
            placeholder={t("approval.comment_placeholder")}
            value={comment}
            disabled={busy}
            onChange={(event) => setComment(event.target.value)}
          />
          <div className="approval-actions">
            <button
              type="button"
              className="approval-approve"
              disabled={busy}
              onClick={() => onApprove(comment.trim())}
            >
              {busy ? <Icon name="spinner" className="icon-spin" /> : <Icon name="check" />}
              {t("approval.approve")}
            </button>
            <button
              type="button"
              className="approval-reject"
              disabled={busy}
              onClick={() => onReject(comment.trim())}
            >
              <Icon name="x" />
              {t("approval.reject")}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
