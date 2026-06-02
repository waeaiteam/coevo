export default function NumberField({
  id,
  value,
  onChange,
  min,
  max,
  step,
}: {
  id?: string;
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <input
      id={id}
      type="number"
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      min={min}
      max={max}
      step={step}
      className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none font-mono"
      style={{ borderColor: "var(--border-accent)", background: "var(--bg-card)", color: "var(--text-primary)" }}
    />
  );
}
