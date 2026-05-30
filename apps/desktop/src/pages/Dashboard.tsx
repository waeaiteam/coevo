import MetricCard from "../components/MetricCard";
import RiskApprovalPanel from "../components/RiskApprovalPanel";
import TrackStatusCard from "../components/TrackStatusCard";

export default function Dashboard() {
  return (
    <div className="space-y-5">
      <h2 className="text-lg font-bold">Dashboard</h2>
      <div className="text-xs" style={{ color: "var(--text-muted)" }}>
        Governance descriptors for the local Alpha workspace. Live counters will be wired to audit telemetry in the next release slice.
      </div>

      <div className="grid grid-cols-6 gap-3">
        <MetricCard label="Governed WorkOrders" value="Ready" sub="MissionChat creates tasks" accent="green" />
        <MetricCard label="AI Employees" value="Bootstrap" sub="created after model setup" accent="blue" />
        <MetricCard label="Timeline Audit" value="Required" sub="sessions, steps, events" accent="purple" />
        <MetricCard label="Red Track" value="Blocked" accent="red" sub="Alpha hard stop" />
        <MetricCard label="COEVO_HOME" value="Local" sub="data and logs" accent="blue" />
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
