import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTheme, type ThemeMode } from "../hooks/useTheme";

type Command = {
  id: string;
  label: string;
  hint: string;
  initials: string;
  run: () => void;
};

const pages = [
  { id: "workbench", label: "工作台", hint: "输入任务", path: "/", initials: "gzt" },
  { id: "employees", label: "AI 员工", hint: "查看团队", path: "/employees", initials: "aiyg" },
  { id: "tasks", label: "任务", hint: "查看任务", path: "/work-orders", initials: "rw" },
  { id: "clients", label: "客户", hint: "客户与记忆", path: "/memory", initials: "kh" },
  { id: "files", label: "文件", hint: "合同与文件", path: "/contracts", initials: "wj" },
  { id: "outcomes", label: "成果", hint: "方案与交付", path: "/plans", initials: "cg" },
  { id: "dashboard", label: "运营概览", hint: "本地状态", path: "/dashboard", initials: "yygl" },
  { id: "skills", label: "员工能力", hint: "技能管理", path: "/skills", initials: "ygnl" },
  { id: "audit", label: "活动记录", hint: "审计导出", path: "/audit", initials: "hdjl" },
  { id: "settings", label: "高级设置", hint: "模型与本地数据", path: "/settings/general", initials: "gjsz" },
  { id: "model", label: "模型设置", hint: "连接模型", path: "/settings/model_provider", initials: "mxsz" },
  { id: "storage", label: "本地数据", hint: "管理数据", path: "/settings/data", initials: "bdsj" },
];

function matches(command: Command, query: string) {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return `${command.label} ${command.hint} ${command.initials}`.toLowerCase().includes(q);
}

export default function CommandPalette() {
  const navigate = useNavigate();
  const { setMode } = useTheme();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const commands = useMemo<Command[]>(() => {
    const navCommands = pages.map((page) => ({
      ...page,
      run: () => navigate(page.path),
    }));
    const themeCommands = ([
      ["theme-system", "跟随系统", "明暗主题", "xt", "system"],
      ["theme-light", "浅色主题", "明亮模式", "qs", "light"],
      ["theme-dark", "深色主题", "夜间模式", "ss", "dark"],
    ] as const).map(([id, label, hint, initials, mode]) => ({
      id,
      label,
      hint,
      initials,
      run: () => setMode(mode as ThemeMode),
    }));
    return [...navCommands, ...themeCommands];
  }, [navigate, setMode]);

  const filtered = commands.filter((command) => matches(command, query));

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const isPaletteKey = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k";
      if (isPaletteKey) {
        event.preventDefault();
        setOpen((value) => !value);
      } else if (open && event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  if (!open) return null;

  function run(command: Command) {
    command.run();
    setOpen(false);
    setQuery("");
  }

  return (
    <div className="command-overlay" role="dialog" aria-modal="true" aria-label="命令面板" onMouseDown={() => setOpen(false)}>
      <div className="command-panel" onMouseDown={(event) => event.stopPropagation()}>
        <input
          autoFocus
          className="command-input"
          placeholder="搜索页面或输入拼音首字母..."
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActiveIndex((value) => Math.min(value + 1, filtered.length - 1));
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setActiveIndex((value) => Math.max(value - 1, 0));
            }
            if (event.key === "Enter" && filtered[activeIndex]) {
              event.preventDefault();
              run(filtered[activeIndex]);
            }
          }}
        />
        <div className="command-list">
          {filtered.map((command, index) => (
            <button
              key={command.id}
              type="button"
              className={`command-item ${index === activeIndex ? "active" : ""}`}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => run(command)}
            >
              <span className="span-icon" aria-hidden="true">⌘</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{command.label}</span>
                <span className="block truncate text-xs muted">{command.hint}</span>
              </span>
            </button>
          ))}
          {filtered.length === 0 && <div className="timeline-empty">没有匹配结果</div>}
        </div>
      </div>
    </div>
  );
}
