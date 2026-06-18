export default function ToggleField({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="toggle-field relative w-9 h-5 rounded-full transition-colors duration-150"
      style={{ background: checked ? "var(--accent)" : "var(--border-accent)", opacity: disabled ? 0.5 : 1 }}
    >
      <span
        aria-hidden="true"
        className="toggle-knob absolute top-0.5 w-4 h-4 rounded-full shadow transition-transform duration-150"
        style={{ left: checked ? "calc(100% - 17px)" : "2px", background: "var(--cv-surface)" }}
      />
    </button>
  );
}
