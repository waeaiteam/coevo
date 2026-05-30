import { useState } from "react";

export default function PasswordField({
  id,
  value,
  onChange,
}: {
  id?: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const [show, setShow] = useState(false);
  return (
    <div className="flex min-w-0 items-center gap-1">
      <input
        id={id}
        type={show ? "text" : "password"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full min-w-0 px-3 py-1.5 rounded-md border text-xs focus:outline-none focus:ring-2 focus:ring-indigo-200 font-mono"
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
