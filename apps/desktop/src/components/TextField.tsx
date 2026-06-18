export default function TextField({
  id,
  value,
  onChange,
  placeholder,
  monospace,
  disabled = false,
  error = false,
}: {
  id?: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  monospace?: boolean;
  disabled?: boolean;
  error?: boolean;
}) {
  return (
    <input
      id={id}
      type="text"
      value={value}
      disabled={disabled}
      aria-invalid={error || undefined}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={`w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none ${monospace ? "font-mono" : ""}`}
      style={{
        borderColor: error ? "var(--red)" : "var(--border-accent)",
        background: "var(--bg-card)",
        color: "var(--text-primary)",
        opacity: disabled ? 0.5 : 1,
      }}
    />
  );
}
