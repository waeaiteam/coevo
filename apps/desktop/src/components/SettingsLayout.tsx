import { useState } from "react";
import SettingsSidebar from "./SettingsSidebar";
import SettingsSearch from "./SettingsSearch";
import SaveBar from "./SaveBar";
import { useSettings } from "../hooks/useSettings";
import { t } from "../settings/i18n";
import type { CoevoSettings } from "../settings/types";

type SectionKey = keyof CoevoSettings | "data_management";

const SECTIONS: { key: SectionKey; label: string; icon: string }[] = [
  { key: "general", label: t("settings.general"), icon: "O" },
  { key: "model_provider", label: t("settings.model_provider"), icon: "M" },
  { key: "appearance", label: t("settings.appearance"), icon: "A" },
  { key: "data_management", label: "Data Management", icon: "D" },
  { key: "developer", label: "Developer Mode", icon: "</>" },
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
      <div className="w-48 border-r flex flex-col" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
        <div className="p-3">
          <SettingsSearch value={search} onChange={setSearch} />
        </div>
        <SettingsSidebar sections={SECTIONS} active={section} search={search} />
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
