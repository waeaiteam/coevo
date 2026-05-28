import { useState } from "react";
import SettingsSidebar from "./SettingsSidebar";
import SettingsSearch from "./SettingsSearch";
import SaveBar from "./SaveBar";
import { useSettings } from "../hooks/useSettings";
import { t } from "../settings/i18n";
import type { CoevoSettings } from "../settings/types";

type SectionKey = keyof CoevoSettings;

const SECTIONS: { key: SectionKey; label: string; icon: string }[] = [
  { key: "general", label: t("settings.general"), icon: "⚙" },
  { key: "appearance", label: t("settings.appearance"), icon: "⊡" },
  { key: "model_provider", label: t("settings.model_provider"), icon: "◎" },
  { key: "agent_runtime", label: t("settings.agent_runtime"), icon: "◈" },
  { key: "governance", label: t("settings.governance"), icon: "⚖" },
  { key: "risk_gate", label: t("settings.risk_gate"), icon: "⚠" },
  { key: "cognitive_customs", label: t("settings.cognitive_customs"), icon: "⊞" },
  { key: "policy_engine", label: t("settings.policy_engine"), icon: "☷" },
  { key: "privacy", label: t("settings.privacy"), icon: "🔒" },
  { key: "developer", label: t("settings.developer"), icon: "</>" },
];

interface Props {
  section: SectionKey;
  content: React.ReactNode;
}

export default function SettingsLayout({ section, content }: Props) {
  const [search, setSearch] = useState("");
  const { dirty, saved, saveNow, reset } = useSettings();

  return (
    <div className="flex h-full" style={{ background: "var(--bg-primary)" }}>
      {/* Left sidebar */}
      <div className="w-48 border-r flex flex-col" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="p-3">
          <SettingsSearch value={search} onChange={setSearch} />
        </div>
        <SettingsSidebar sections={SECTIONS} active={section} search={search} />
      </div>

      {/* Right content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-y-auto">
          <div className="max-w-2xl mx-auto p-6">
            {content}
          </div>
        </div>
        <SaveBar dirty={dirty} saved={saved} onSave={saveNow} onReset={reset} />
      </div>
    </div>
  );
}
