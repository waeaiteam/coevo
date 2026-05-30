import { t } from "../settings/i18n";

export default function SaveBar({
  dirty,
  saved,
  onSave,
  onReset,
}: {
  dirty: boolean;
  saved: boolean;
  onSave: () => void;
  onReset: () => void;
}) {
  if (!dirty && !saved) return null;

  return (
    <div className="flex items-center justify-between gap-3 border-t px-6 py-3" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
      <div className="text-xs" style={{ color: saved ? "var(--green)" : "var(--yellow)" }}>
        {saved ? t("settings.saved") : t("settings.unsaved")}
      </div>
      <div className="flex gap-2">
        <button
          onClick={onReset}
          className="rounded-md border px-4 py-1.5 text-xs transition-colors"
          style={{ borderColor: "var(--border-accent)", color: "var(--text-secondary)" }}
        >
          {t("settings.reset")}
        </button>
        <button
          onClick={onSave}
          className="rounded-md px-4 py-1.5 text-xs text-white transition-colors"
          style={{ background: "var(--accent)" }}
        >
          {t("settings.save")}
        </button>
      </div>
    </div>
  );
}
