export default function SelectField({
  id,
  value,
  options,
  onChange,
  disabled = false,
  error = false,
}: {
  id?: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  disabled?: boolean;
  error?: boolean;
}) {
  return (
    <select
      id={id}
      value={value}
      disabled={disabled}
      aria-invalid={error || undefined}
      onChange={(e) => onChange(e.target.value)}
      className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none"
      style={{
        borderColor: error ? "var(--red)" : "var(--border-accent)",
        background: "var(--bg-card)",
        color: "var(--text-primary)",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}
