export default function SettingRow({
  label,
  desc,
  children,
  htmlFor,
}: {
  label: string;
  desc?: string;
  children: React.ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="flex items-start justify-between gap-4 px-4 py-3 border-b" style={{ borderColor: "var(--border-subtle)" }}>
      <div className="flex-1 min-w-0">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="text-sm" style={{ color: "var(--text-primary)" }}>{label}</label>
        ) : (
          <div className="text-sm" style={{ color: "var(--text-primary)" }}>{label}</div>
        )}
        {desc && <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>{desc}</div>}
      </div>
      <div className="flex-shrink-0 max-w-full">{children}</div>
    </div>
  );
}
