import { useState, useCallback } from "react";
import MetricCard from "../components/MetricCard";
import TrackStatusCard from "../components/TrackStatusCard";
import GovernanceTimeline, { type TimelineEvent } from "../components/GovernanceTimeline";
import RiskApprovalPanel from "../components/RiskApprovalPanel";
import DemoActionPanel from "../components/DemoActionPanel";
import type { DemoResponse } from "../types";

let eventId = 0;
function makeEvent(
  type: TimelineEvent["type"],
  message: string,
  detail?: string,
  track?: "green" | "yellow" | "red"
): TimelineEvent {
  return {
    id: String(++eventId),
    time: new Date().toLocaleTimeString(),
    type,
    message,
    detail,
    track,
  };
}

export default function Dashboard() {
  const [events, setEvents] = useState<TimelineEvent[]>([]);

  const handleDemoResult = useCallback((r: DemoResponse) => {
    setEvents((prev) => [
      makeEvent("demo", `${r.track.toUpperCase()} Track executed`, `contract: ${r.contract_hash.slice(0, 12)}... | plan: ${r.plan_hash.slice(0, 12)}... | entries: ${r.entries_created.length}`, r.track as "green" | "yellow" | "red"),
      makeEvent(r.track === "green" ? "compile" : r.track === "yellow" ? "propose" : "risk",
        r.track === "green" ? "MCL compiled" : r.track === "yellow" ? "CognitiveCustoms proposed" : "RiskGate evaluated",
        r.track === "red" ? `decision: lease granted, ${r.entries_created.length} operations` : `hash: ${r.contract_hash.slice(0, 12)}...`,
        r.track as "green" | "yellow" | "red"),
      ...prev,
    ]);
  }, []);

  return (
    <div className="space-y-5">
      <h2 className="text-lg font-bold">Dashboard</h2>
      {/* KPI Row */}
      <div className="grid grid-cols-6 gap-3">
        <MetricCard label="Active Contracts" value={12} sub="+3 this hour" accent="purple" />
        <MetricCard label="Running Plans" value={4} sub="2 in Yellow Track" accent="blue" />
        <MetricCard label="Pending Approvals" value={2} sub="1 explicit" accent="yellow" />
        <MetricCard label="Red Blocks" value={1} accent="red" sub="leased" />
        <MetricCard label="ADR-A Records" value={8} sub="today" accent="purple" />
        <MetricCard label="Audit Events" value={247} sub="24h" accent="green" />
      </div>

      {/* Three Tracks + Risk Panel */}
      <div className="grid grid-cols-4 gap-3">
        <TrackStatusCard
          track="green"
          metrics={[
            { label: "Auto executions", value: "156" },
            { label: "Avg latency", value: "43ms" },
            { label: "Success rate", value: "99.4%" },
            { label: "Last run", value: "2s ago" },
          ]}
        />
        <TrackStatusCard
          track="yellow"
          metrics={[
            { label: "Pending approvals", value: "2" },
            { label: "Negative consent", value: "1" },
            { label: "Explicit approval", value: "1" },
            { label: "Timeout window", value: "3m 12s" },
          ]}
        />
        <TrackStatusCard
          track="red"
          metrics={[
            { label: "Circuit breaks", value: "3" },
            { label: "Emergency leases", value: "1" },
            { label: "Human overrides", value: "0" },
            { label: "Blocked actions", value: "2" },
          ]}
        />
        <RiskApprovalPanel />
      </div>

      {/* Timeline + Demo */}
      <div className="grid grid-cols-3 gap-3">
        <div className="col-span-2">
          <GovernanceTimeline events={events} />
        </div>
        <DemoActionPanel onResult={handleDemoResult} />
      </div>
    </div>
  );
}
