export default function TextField({
  value,
  onChange,
  placeholder,
  monospace,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  monospace?: boolean;
}) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={`px-3 py-1.5 rounded-md border text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200 w-56 ${monospace ? "font-mono" : ""}`}
      style={{ borderColor: "var(--border-accent)", background: "#fff", color: "var(--text-primary)" }}
    />
  );
}
