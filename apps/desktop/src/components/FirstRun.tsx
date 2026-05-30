import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { createLocalOpc } from "../settings/identity";
import { setLanguage, t, useLanguage } from "../settings/i18n";

export default function FirstRun({ onDone }: { onDone: () => void }) {
  useLanguage();
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
    <div className="flex min-h-screen items-center justify-center px-6" style={{ background: "var(--bg-primary)", color: "var(--text-primary)" }}>
      <div className="grid w-full max-w-5xl gap-8 md:grid-cols-[1fr_360px]">
        <section className="flex flex-col justify-center">
          <div className="mb-5 flex items-center gap-3">
            <span className="grid h-9 w-9 place-items-center rounded-md text-sm font-bold text-white" style={{ background: "var(--accent)" }}>c</span>
            <span className="text-sm font-semibold">{t("app.tagline")}</span>
          </div>
          <h1 className="text-3xl font-bold tracking-tight md:text-4xl">{t("first_run.title")}</h1>
          <p className="mt-3 max-w-xl text-sm leading-6" style={{ color: "var(--text-secondary)" }}>
            {t("first_run.subtitle")}
          </p>
          <div className="mt-8 grid max-w-2xl gap-3 sm:grid-cols-3">
            {[t("first_run.step_identity"), t("first_run.step_provider"), t("first_run.step_mission")].map((step, i) => (
              <div key={step} className="rounded-lg border bg-white p-3 text-xs" style={{ borderColor: "var(--border-subtle)", color: "var(--text-secondary)" }}>
                <div className="mb-1 font-mono" style={{ color: "var(--accent)" }}>0{i + 1}</div>
                {step}
              </div>
            ))}
          </div>
        </section>

        <section className="rounded-xl border bg-white p-5 shadow-sm" style={{ borderColor: "var(--border-subtle)" }}>
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-opc-name">{t("first_run.opc_name")}</label>
              <input
                id="first-run-opc-name"
                className="input w-full"
                value={opcName}
                onChange={(e) => setOpcName(e.target.value)}
              />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-owner-name">{t("first_run.owner_name")}</label>
              <input
                id="first-run-owner-name"
                className="input w-full"
                value={ownerName}
                onChange={(e) => setOwnerName(e.target.value)}
              />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-semibold" htmlFor="first-run-language">{t("first_run.language")}</label>
              <select
                id="first-run-language"
                className="input w-full"
                value={language}
                onChange={(e) => setLocalLanguage(e.target.value === "zh" ? "zh" : "en")}
              >
                <option value="en">English</option>
                <option value="zh">中文</option>
              </select>
            </div>
            <button
              onClick={createOpc}
              className="w-full rounded-md py-3 text-sm font-semibold text-white"
              style={{ background: "var(--accent)" }}
            >
              {t("first_run.create")}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
