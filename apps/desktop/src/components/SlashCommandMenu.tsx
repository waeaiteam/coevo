import Icon from "./Icon";
import { t } from "../settings/i18n";
import type { SlashCommandSpec } from "../utils/slashCommands";

type Props = {
  open: boolean;
  query: string;
  commands: SlashCommandSpec[];
  activeIndex: number;
  onPick: (command: SlashCommandSpec) => void;
  onClose: () => void;
};

export default function SlashCommandMenu({
  open,
  query,
  commands,
  activeIndex,
  onPick,
  onClose,
}: Props) {
  if (!open) return null;

  return (
    <div
      className="slash-command-menu"
      role="listbox"
      aria-label={t("slash.menu_title")}
      aria-expanded="true"
      data-query={query}
      onMouseDown={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
      }}
    >
      {commands.length > 0 ? (
        commands.map((command, index) => {
          const selected = index === activeIndex;
          return (
            <button
              key={command.name}
              type="button"
              role="option"
              aria-selected={selected}
              className={`slash-command-item ${selected ? "active" : ""}`}
              onMouseEnter={() => void 0}
              onClick={() => onPick(command)}
            >
              <span className="slash-command-icon" aria-hidden="true">
                <Icon name="sparkles" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{command.usage}</span>
                <span className="block truncate text-xs muted">{t(command.descKey)}</span>
              </span>
            </button>
          );
        })
      ) : (
        <div className="slash-command-empty" role="status">
          {t("slash.no_results")}
        </div>
      )}
    </div>
  );
}
