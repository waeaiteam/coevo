import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { createLocalOpc } from "../settings/identity";
import { setLanguage } from "../settings/i18n";

export default function FirstRun({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate();
  const [opcName, setOpcName] = useState("My OPC");
  const [ownerName, setOwnerName] = useState("Founder");
  const [language, setLocalLanguage] = useState<"en" | "zh">("en");

  function createOpc() {
    createLocalOpc({ opcName, userName: ownerName, language });
    setLanguage(language);
    onDone();
    navigate("/settings/model_provider");
  }

  return (
    <div
      className="flex flex-col items-center justify-center h-screen"
      style={{ background: "var(--bg-primary)", color: "var(--text-primary)" }}
    >
      <div className="text-4xl mb-4" style={{ color: "var(--accent)" }}>*</div>
      <h1 className="text-2xl font-bold mb-2">Create your OPC</h1>
      <p className="text-sm mb-6 text-center max-w-sm" style={{ color: "var(--text-secondary)" }}>
        Start a local governed AI company, then connect your model provider.
      </p>
      <div className="w-80 space-y-3">
        <label className="block text-xs font-semibold" htmlFor="first-run-opc-name">OPC name</label>
        <input
          id="first-run-opc-name"
          className="input w-full"
          value={opcName}
          onChange={(e) => setOpcName(e.target.value)}
        />
        <label className="block text-xs font-semibold" htmlFor="first-run-owner-name">Owner name</label>
        <input
          id="first-run-owner-name"
          className="input w-full"
          value={ownerName}
          onChange={(e) => setOwnerName(e.target.value)}
        />
        <label className="block text-xs font-semibold" htmlFor="first-run-language">Language</label>
        <select
          id="first-run-language"
          className="input w-full"
          value={language}
          onChange={(e) => setLocalLanguage(e.target.value === "zh" ? "zh" : "en")}
        >
          <option value="en">English</option>
          <option value="zh">中文</option>
        </select>
        <button
          onClick={createOpc}
          className="w-full py-3 text-sm rounded-md text-white font-semibold"
          style={{ background: "var(--accent)" }}
        >
          Create OPC
        </button>
      </div>
    </div>
  );
}
