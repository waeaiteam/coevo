export default function ToggleField({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className="relative w-9 h-5 rounded-full transition-colors duration-150"
      style={{ background: checked ? "var(--accent)" : "var(--border-accent)" }}
    >
      <span
        className="absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform duration-150"
        style={{ left: checked ? "calc(100% - 17px)" : "2px" }}
      />
    </button>
  );
}
