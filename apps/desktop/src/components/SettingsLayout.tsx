import { useState } from "react";
import SettingsSidebar from "./SettingsSidebar";
import SettingsSearch from "./SettingsSearch";
import SaveBar from "./SaveBar";
import { useSettings } from "../hooks/useSettings";
import { t, useLanguage } from "../settings/i18n";
import type { CoevoSettings } from "../settings/types";

type SectionKey = keyof CoevoSettings | "data_management" | "mcp_servers";
type SectionGroup = "common" | "advanced";

const COMMON_SECTIONS: SectionKey[] = ["general", "model_provider", "mcp_servers", "appearance", "data_management"];
const ADVANCED_SECTIONS: SectionKey[] = ["agent_runtime", "governance", "risk_gate", "cognitive_customs", "policy_engine", "privacy", "developer"];

interface Props {
  section: SectionKey;
  content: React.ReactNode;
}

export default function SettingsLayout({ section, content }: Props) {
  useLanguage();
  const [search, setSearch] = useState("");
  const { dirty, saved, saveNow, reset } = useSettings();
  const sections: { key: SectionKey; label: string; icon: string; group: SectionGroup }[] = [
    ...COMMON_SECTIONS.map((key) => ({
      key,
      label: t(`settings.${key}`),
      icon: key === "general" ? "○" : key === "model_provider" ? "◉" : key === "appearance" ? "◐" : "◧",
      group: "common" as const,
    })),
    ...ADVANCED_SECTIONS.map((key) => ({
      key,
      label: t(`settings.${key}`),
      icon: "◆",
      group: "advanced" as const,
    })),
  ];

  return (
    <div className="flex h-full" style={{ background: "var(--bg-primary)" }}>
      <div className="w-56 border-r flex flex-col" style={{ background: "var(--surface-raised)", borderColor: "var(--border-subtle)" }}>
        <div className="p-3">
          <SettingsSearch value={search} onChange={setSearch} />
        </div>
        <SettingsSidebar sections={sections} active={section} search={search} />
      </div>

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
