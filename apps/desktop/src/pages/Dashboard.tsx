import { useHealth } from "../hooks/useApi";

export default function Dashboard() {
  const { health, error } = useHealth();

  return (
    <div>
      <h2 className="text-2xl font-bold mb-6">Dashboard</h2>
      <div className="grid grid-cols-3 gap-4">
        <StatusCard title="Server Status" value={health?.status ?? "unknown"} color="green" />
        <StatusCard title="Version" value={health?.version ?? "—"} color="blue" />
        <StatusCard
          title="Connection"
          value={error ? "Error" : health ? "OK" : "Connecting..."}
          color={error ? "red" : "green"}
        />
      </div>
      {error && (
        <div className="mt-6 p-4 bg-red-50 border border-red-200 rounded text-red-700 text-sm">
          Cannot connect to coevo server at http://127.0.0.1:8717 — make sure the server is running.
        </div>
      )}
      <div className="mt-8 grid grid-cols-3 gap-4 text-sm">
        <TrackCard track="Green" description="BR=0, IR=0 — Fast, no approval" color="green" />
        <TrackCard track="Yellow" description="IR=1 — Async with approval window" color="yellow" />
        <TrackCard track="Red" description="IR=3 — Circuit breaker, lease" color="red" />
      </div>
    </div>
  );
}

function StatusCard({ title, value, color }: { title: string; value: string; color: string }) {
  const colors: Record<string, string> = {
    green: "border-green-400 bg-green-50",
    blue: "border-blue-400 bg-blue-50",
    red: "border-red-400 bg-red-50",
  };
  return (
    <div className={`p-4 rounded border ${colors[color] || colors.blue}`}>
      <div className="text-xs font-medium uppercase text-gray-500">{title}</div>
      <div className="text-xl font-bold mt-1">{value}</div>
    </div>
  );
}

function TrackCard({ track, description, color }: { track: string; description: string; color: string }) {
  const colors: Record<string, string> = {
    green: "border-green-300 bg-green-50",
    yellow: "border-yellow-300 bg-yellow-50",
    red: "border-red-300 bg-red-50",
  };
  return (
    <div className={`p-4 rounded border ${colors[color]}`}>
      <span className={`track-${track.toLowerCase()}`}>{track} Track</span>
      <p className="mt-2 text-gray-600">{description}</p>
    </div>
  );
}
