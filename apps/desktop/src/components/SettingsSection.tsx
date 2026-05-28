export default function SettingsSection({
  title,
  desc,
  children,
}: {
  title: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold mb-1" style={{ color: "var(--text-primary)" }}>{title}</h3>
      {desc && <p className="text-xs mb-4" style={{ color: "var(--text-muted)" }}>{desc}</p>}
      <div className="card space-y-1" style={{ padding: 0 }}>
        {children}
      </div>
    </div>
  );
}
