import { useState } from "react";
import { runDemo, DemoResponse } from "../api/client";
import TrackMonitor from "../components/TrackMonitor";

export default function Demos() {
  const [result, setResult] = useState<DemoResponse | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleRun(track: "green" | "yellow" | "red") {
    setLoading(track);
    setError(null);
    setResult(null);
    try {
      const res = await runDemo(track);
      setResult(res);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(null);
    }
  }

  return (
    <div>
      <h2 className="text-2xl font-bold mb-6">Demo Scenarios</h2>
      <div className="flex gap-3 mb-6">
        <button
          onClick={() => handleRun("green")}
          disabled={loading !== null}
          className="px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700 disabled:opacity-50 text-sm"
        >
          {loading === "green" ? "Running..." : "▶ Green Track"}
        </button>
        <button
          onClick={() => handleRun("yellow")}
          disabled={loading !== null}
          className="px-4 py-2 bg-yellow-600 text-white rounded hover:bg-yellow-700 disabled:opacity-50 text-sm"
        >
          {loading === "yellow" ? "Running..." : "▶ Yellow Track"}
        </button>
        <button
          onClick={() => handleRun("red")}
          disabled={loading !== null}
          className="px-4 py-2 bg-red-600 text-white rounded hover:bg-red-700 disabled:opacity-50 text-sm"
        >
          {loading === "red" ? "Running..." : "▶ Red Track"}
        </button>
      </div>

      {error && (
        <div className="p-4 bg-red-50 border border-red-200 rounded text-red-700 text-sm mb-4">
          {error}
        </div>
      )}

      {result && <TrackMonitor result={result} />}
    </div>
  );
}
