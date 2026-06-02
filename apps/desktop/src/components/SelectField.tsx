export default function SelectField({
  id,
  value,
  options,
  onChange,
}: {
  id?: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <select
      id={id}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none"
      style={{ borderColor: "var(--border-accent)", background: "var(--bg-card)", color: "var(--text-primary)" }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}
