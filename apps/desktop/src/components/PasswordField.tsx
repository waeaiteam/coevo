import { useState } from "react";

export default function PasswordField({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const [show, setShow] = useState(false);
  return (
    <div className="flex items-center gap-1">
      <input
        type={show ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="px-3 py-1.5 rounded-md border text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200 w-48 font-mono"
        style={{ borderColor: "var(--border-accent)", background: "#fff", color: "var(--text-primary)" }}
        placeholder="sk-..."
      />
      <button
        onClick={() => setShow(!show)}
        className="px-2 py-1 text-xs rounded"
        style={{ color: "var(--text-muted)" }}
      >
        {show ? "Hide" : "Show"}
      </button>
    </div>
  );
}
