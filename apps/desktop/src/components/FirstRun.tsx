import { useNavigate } from "react-router-dom";

export default function FirstRun({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate();

  function goToModels() {
    onDone();
    navigate("/settings/model_provider");
  }

  return (
    <div
      className="flex flex-col items-center justify-center h-screen"
      style={{ background: "var(--bg-primary)", color: "var(--text-primary)" }}
    >
      <div className="text-4xl mb-4" style={{ color: "var(--accent)" }}>
        *
      </div>
      <h1 className="text-2xl font-bold mb-2">Welcome to coevo</h1>
      <p className="text-sm mb-6 text-center max-w-sm" style={{ color: "var(--text-secondary)" }}>
        Connect your model provider to start running coevo with your own API key.
      </p>
      <div className="w-80">
        <button
          onClick={goToModels}
          className="w-full py-3 text-sm rounded-md text-white font-semibold"
          style={{ background: "var(--accent)" }}
        >
          Configure Model Provider
        </button>
      </div>
    </div>
  );
}
