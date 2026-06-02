export interface MissionDraft {
  intent: string;
  suggestedTrack: "green" | "yellow" | "red";
  reason: string;
  contractHash: string;
  planHash: string;
  ambiguityScore: number | null;
  selectedAgents: string[];
  allowedActions: string[];
  restrictedActions: string[];
  approvalRequired: boolean;
  approvalMode: string;
  compileResult: unknown;
  routeResult: unknown;
}

interface Props {
  draft: MissionDraft;
  loading: boolean;
  onExecute: (track: "green" | "yellow" | "red") => void;
  onPlanOnly: () => void;
  onCancel: () => void;
}

const trackStyle = (t: string) => {
  if (t === "green") return { border: "rgba(34,197,94,0.3)", bg: "rgba(34,197,94,0.04)", text: "var(--green)" };
  if (t === "yellow") return { border: "rgba(234,179,8,0.3)", bg: "rgba(234,179,8,0.04)", text: "var(--yellow)" };
  return { border: "rgba(239,68,68,0.3)", bg: "rgba(239,68,68,0.04)", text: "var(--red)" };
};

export default function MissionDraftCard({ draft, loading, onExecute, onPlanOnly, onCancel }: Props) {
  const s = trackStyle(draft.suggestedTrack);

  return (
    <div className="flex justify-start">
      <div
        className="w-full max-w-2xl rounded-xl border p-5 space-y-4"
        style={{ background: "var(--bg-card)", borderColor: s.border, borderLeftWidth: 4 }}
      >
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-bold" style={{ color: "var(--text-primary)" }}>
              Mission Draft Generated
            </div>
            <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>
              Review the draft before execution
            </div>
          </div>
          <span className="text-xs font-semibold px-2.5 py-1 rounded-full"
            style={{ background: s.bg, color: s.text, border: `1px solid ${s.border}` }}>
            {draft.suggestedTrack.toUpperCase()} Track
          </span>
        </div>

        {/* Reason */}
        <div className="text-xs p-2 rounded" style={{ background: s.bg, color: "var(--text-primary)" }}>
          {draft.reason}
        </div>

        {/* Details grid */}
        <div className="grid grid-cols-2 gap-2 text-xs">
          <Field label="Contract Hash" value={`${draft.contractHash.slice(0, 16)}...`} accent />
          <Field label="Plan Hash" value={`${draft.planHash.slice(0, 16)}...`} accent />
          <Field label="Ambiguity" value={draft.ambiguityScore?.toFixed(2) ?? "—"} />
          <Field label="Approval Mode" value={draft.approvalMode} />
          <Field label="Approval Required" value={draft.approvalRequired ? "Yes" : "No"}
            color={draft.approvalRequired ? "var(--yellow)" : "var(--green)"} />
          <Field label="Selected Agents" value={draft.selectedAgents.join(", ")} accent />
        </div>

        {/* Allowed & Restricted */}
        <div className="grid grid-cols-2 gap-3 text-xs">
          <div>
            <div className="font-semibold mb-1" style={{ color: "var(--green)" }}>Allowed</div>
            {draft.allowedActions.map((a) => (
              <div key={a} className="font-mono py-0.5" style={{ color: "var(--text-secondary)" }}>{a}</div>
            ))}
          </div>
          <div>
            <div className="font-semibold mb-1" style={{ color: "var(--red)" }}>Restricted</div>
            {draft.restrictedActions.map((a) => (
              <div key={a} className="font-mono py-0.5" style={{ color: "var(--text-muted)" }}>{a}</div>
            ))}
          </div>
        </div>

        {/* Description */}
        <div className="text-xs leading-relaxed p-2 rounded" style={{ background: "var(--bg-secondary)", color: "var(--text-secondary)" }}>
          coevo does not create unconstrained agents. It selects from Agent Registry and may spawn short-lived Task Agent Instances with limited permissions. Ephemeral Sub-Agents can only write Hypothesis or Suggestion, never Fact or Decision directly.
        </div>

        {/* Buttons */}
        <div className="flex flex-wrap gap-2 pt-1">
          <Btn label="◈ 只读分析 Green" color="var(--green)" border="rgba(34,197,94,0.3)" onClick={() => onExecute("green")} disabled={loading} />
          <Btn label="⚡ 协作审批 Yellow" color="var(--yellow)" border="rgba(234,179,8,0.3)" onClick={() => onExecute("yellow")} disabled={loading} />
          <Btn label="⚠ 高风险执行 Red" color="var(--red)" border="rgba(239,68,68,0.3)" onClick={() => onExecute("red")} disabled={loading} />
          <Btn label="⊡ 只生成计划" color="var(--text-secondary)" border="var(--border-accent)" onClick={onPlanOnly} disabled={loading} />
          <Btn label="✕ 取消任务" color="var(--text-muted)" border="var(--border-accent)" onClick={onCancel} disabled={loading} />
        </div>
      </div>
    </div>
  );
}

function Field({ label, value, accent, color }: { label: string; value: string; accent?: boolean; color?: string }) {
  return (
    <div>
      <span style={{ color: "var(--text-muted)" }}>{label}: </span>
      <span className={accent ? "font-mono" : "font-mono"} style={{ color: color || (accent ? "var(--accent)" : "var(--text-secondary)") }}>
        {value}
      </span>
    </div>
  );
}

function Btn({ label, color, border, onClick, disabled }: { label: string; color: string; border: string; onClick: () => void; disabled: boolean }) {
  return (
    <button onClick={onClick} disabled={disabled}
      className="px-3 py-1.5 text-xs font-semibold rounded-md border transition-colors disabled:opacity-30"
      style={{ borderColor: border, color, background: "transparent" }}>
      {label}
    </button>
  );
}
