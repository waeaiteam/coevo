import { t } from "../settings/i18n";

export default function SettingsSearch({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const id = "settings-search";
  return (
    <label htmlFor={id} className="block">
      <span className="sr-only">{t("settings.search")}</span>
      <input
        id={id}
        className="w-full rounded-md border px-3 py-2 text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200"
        style={{ borderColor: "var(--border-subtle)", background: "var(--bg-secondary)", color: "var(--text-primary)" }}
        placeholder={t("settings.search")}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
