import { t } from "../settings/i18n";

export default function SettingsSearch({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <input
      className="w-full px-3 py-2 rounded-md border text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200"
      style={{ borderColor: "var(--border-subtle)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
      placeholder={t("settings.search")}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
