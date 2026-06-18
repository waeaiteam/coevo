import { useState } from "react";
import { t } from "../settings/i18n";

export default function PasswordField({
  id,
  value,
  onChange,
}: {
  id?: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const [show, setShow] = useState(false);
  const label = show ? t("common.hide") : t("common.show");
  return (
    <div className="flex min-w-0 items-center gap-1">
      <input
        id={id}
        type={show ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none font-mono"
        style={{ borderColor: "var(--border-accent)", background: "var(--bg-card)", color: "var(--text-primary)" }}
        placeholder="sk-..."
      />
      <button
        type="button"
        onClick={() => setShow(!show)}
        aria-label={label}
        aria-pressed={show}
        className="px-2 py-1 text-xs rounded"
        style={{ color: "var(--text-muted)" }}
      >
        {label}
      </button>
    </div>
  );
}

