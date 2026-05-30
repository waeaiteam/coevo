export default function TextField({
  id,
  value,
  onChange,
  placeholder,
  monospace,
}: {
  id?: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  monospace?: boolean;
}) {
  return (
    <input
      id={id}
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={`w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200 ${monospace ? "font-mono" : ""}`}
      style={{ borderColor: "var(--border-accent)", background: "#fff", color: "var(--text-primary)" }}
    />
  );
}
