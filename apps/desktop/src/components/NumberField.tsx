export default function NumberField({
  id,
  value,
  onChange,
  min,
  max,
  step,
  disabled = false,
  error = false,
}: {
  id?: string;
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  error?: boolean;
}) {
  return (
    <input
      id={id}
      type="number"
      value={value}
      disabled={disabled}
      aria-invalid={error || undefined}
      onChange={(e) => onChange(Number(e.target.value))}
      min={min}
      max={max}
      step={step}
      className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none font-mono"
      style={{
        borderColor: error ? "var(--red)" : "var(--border-accent)",
        background: "var(--bg-card)",
        color: "var(--text-primary)",
        opacity: disabled ? 0.5 : 1,
      }}
    />
  );
}
