import MetricCard from "../components/MetricCard";
import RiskApprovalPanel from "../components/RiskApprovalPanel";
import TrackStatusCard from "../components/TrackStatusCard";
import { getLocalIdentity } from "../settings/identity";

export default function Dashboard() {
  const identity = getLocalIdentity();

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-bold">{identity.opcName}</h2>
        <div className="text-xs mt-1" style={{ color: "var(--text-muted)" }}>
          Local OPC identity and governance posture.
        </div>
      </div>

      <div className="grid grid-cols-6 gap-3">
        <MetricCard label="Owner" value={identity.userName} sub={identity.userId} accent="blue" />
        <MetricCard label="OPC ID" value="Local" sub={identity.opcId} accent="purple" />
        <MetricCard label="Governed WorkOrders" value="Ready" sub="MissionChat creates tasks" accent="green" />
        <MetricCard label="Timeline Audit" value="Required" sub="sessions, steps, events" accent="purple" />
        <MetricCard label="Red Track" value="Blocked" accent="red" sub="Alpha hard stop" />
        <MetricCard label="Model Gateway" value="Required" sub="configured provider" accent="green" />
      </div>

      <div className="grid grid-cols-4 gap-3">
        <TrackStatusCard
          track="green"
          metrics={[
            { label: "Default behavior", value: "Auto executable" },
            { label: "Allowed actions", value: "read/analyze" },
            { label: "Worker audit", value: "required" },
            { label: "Memory writes", value: "Hypothesis" },
          ]}
        />
        <TrackStatusCard
          track="yellow"
          metrics={[
            { label: "Default behavior", value: "Approval required" },
            { label: "Approval mode", value: "Negative consent" },
            { label: "Writes", value: "draft only" },
            { label: "Timeline", value: "required" },
          ]}
        />
        <TrackStatusCard
          track="red"
          metrics={[
            { label: "Default behavior", value: "Blocked" },
            { label: "HTTP result", value: "403" },
            { label: "Requires", value: "MFA + lease" },
            { label: "Alpha execution", value: "disabled" },
          ]}
        />
        <RiskApprovalPanel />
      </div>

      <div className="card">
        <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
          First Mission Path
        </div>
        <div className="grid grid-cols-5 gap-3 text-xs">
          {[
            "Configure model provider",
            "Enter MissionChat intent",
            "Create governed WorkOrder",
            "Execute Green Track",
            "Inspect Timeline/Audit",
          ].map((step, i) => (
            <div key={step} className="p-3 rounded border" style={{ borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
              <div className="font-mono mb-1" style={{ color: "var(--accent)" }}>0{i + 1}</div>
              {step}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
