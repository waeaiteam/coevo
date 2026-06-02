import type { ReactNode } from "react";

export type IconName =
  | "plus"
  | "paperclip"
  | "folder"
  | "send"
  | "sparkles"
  | "brain"
  | "wrench"
  | "external"
  | "layers"
  | "filter"
  | "shield-check"
  | "clipboard"
  | "git-branch"
  | "calendar"
  | "file-text"
  | "badge-check"
  | "gauge"
  | "command"
  | "search"
  | "sun"
  | "moon"
  | "monitor"
  | "check"
  | "x"
  | "alert"
  | "clock"
  | "chevron-right"
  | "chevron-down"
  | "building"
  | "folder-tree"
  | "list-checks"
  | "history"
  | "settings"
  | "user"
  | "database"
  | "users"
  | "puzzle"
  | "spinner"
  | "sliders"
  | "info";

const PATHS: Record<IconName, ReactNode> = {
  plus: <path d="M12 5v14M5 12h14" />,
  paperclip: (
    <path d="M21 11.5l-8.95 8.96a5 5 0 0 1-7.07-7.07l8.49-8.49a3.33 3.33 0 0 1 4.71 4.71l-8.5 8.49a1.67 1.67 0 0 1-2.36-2.36l7.78-7.78" />
  ),
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />,
  send: (
    <>
      <path d="M22 2L11 13" />
      <path d="M22 2l-7 20-4-9-9-4 20-7z" />
    </>
  ),
  sparkles: (
    <>
      <path d="M12 3l1.8 4.9L18.7 9.7l-4.9 1.8L12 16.4l-1.8-4.9L5.3 9.7l4.9-1.8L12 3z" />
      <path d="M19 14l.8 2.2L22 17l-2.2.8L19 20l-.8-2.2L16 17l2.2-.8L19 14z" />
    </>
  ),
  brain: (
    <path d="M9.5 4a2.5 2.5 0 0 0-2.5 2.5 2.5 2.5 0 0 0-1 4.79V13a2.5 2.5 0 0 0 2.5 2.5h.5V20m4.5-16a2.5 2.5 0 0 1 2.5 2.5 2.5 2.5 0 0 1 1 4.79V13a2.5 2.5 0 0 1-2.5 2.5H14V20M9.5 4A2.5 2.5 0 0 1 12 6.5 2.5 2.5 0 0 1 14.5 4" />
  ),
  wrench: (
    <path d="M14.7 6.3a4 4 0 0 0 5 5l-9 9a2.83 2.83 0 1 1-4-4l9-9a4 4 0 0 0-1-1z" />
  ),
  external: (
    <>
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
      <path d="M15 3h6v6M10 14L21 3" />
    </>
  ),
  layers: (
    <>
      <path d="M12 2l9 5-9 5-9-5 9-5z" />
      <path d="M3 12l9 5 9-5M3 17l9 5 9-5" />
    </>
  ),
  filter: <path d="M3 4h18l-7 8.5V19l-4 2v-8.5L3 4z" />,
  "shield-check": (
    <>
      <path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6l8-3z" />
      <path d="M9 12l2 2 4-4" />
    </>
  ),
  clipboard: (
    <>
      <path d="M9 4h6a1 1 0 0 1 1 1v1H8V5a1 1 0 0 1 1-1z" />
      <path d="M8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2" />
    </>
  ),
  "git-branch": (
    <>
      <path d="M6 3v12" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="6" cy="6" r="3" />
      <circle cx="18" cy="6" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </>
  ),
  calendar: (
    <>
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M16 2v4M8 2v4M3 10h18" />
    </>
  ),
  "file-text": (
    <>
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6z" />
      <path d="M14 2v6h6M8 13h8M8 17h8M8 9h2" />
    </>
  ),
  "badge-check": (
    <>
      <path d="M12 2l2.4 2.1 3.1-.5 1 3 2.9 1.3-1.1 3 1.1 3-2.9 1.3-1 3-3.1-.5L12 22l-2.4-2.1-3.1.5-1-3L2.6 16l1.1-3-1.1-3 2.9-1.3 1-3 3.1.5L12 2z" />
      <path d="M9 12l2 2 4-4" />
    </>
  ),
  gauge: (
    <>
      <path d="M12 14l4-4" />
      <path d="M3.5 18a9 9 0 1 1 17 0" />
    </>
  ),
  command: (
    <path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6z" />
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>
  ),
  moon: <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />,
  monitor: (
    <>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8M12 16v4" />
    </>
  ),
  check: <path d="M20 6L9 17l-5-5" />,
  x: <path d="M18 6L6 18M6 6l12 12" />,
  alert: (
    <>
      <path d="M12 3l9.5 16.5a1 1 0 0 1-.87 1.5H3.37a1 1 0 0 1-.87-1.5L12 3z" />
      <path d="M12 9v5M12 17.5h.01" />
    </>
  ),
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  "chevron-right": <path d="M9 6l6 6-6 6" />,
  "chevron-down": <path d="M6 9l6 6 6-6" />,
  building: (
    <>
      <rect x="4" y="3" width="16" height="18" rx="1.5" />
      <path d="M9 7h.01M15 7h.01M9 11h.01M15 11h.01M9 15h.01M15 15h.01M10 21v-3h4v3" />
    </>
  ),
  "folder-tree": (
    <>
      <path d="M3 4h4l1.5 2H13a1 1 0 0 1 1 1v2H3V4z" />
      <path d="M3 12h4l1.5 2H21a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H3v-9z" />
    </>
  ),
  "list-checks": (
    <>
      <path d="M3 6l1.5 1.5L7 5M3 13l1.5 1.5L7 12M3 19l1.5 1.5L7 17" />
      <path d="M11 6h10M11 13h10M11 19h10" />
    </>
  ),
  history: (
    <>
      <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
      <path d="M3 3v5h5M12 7v5l3.5 2" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 13a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V20a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-2.7-1.1l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.6 1.6 0 0 0 4 13a2 2 0 1 1 0-4 1.6 1.6 0 0 0 1.1-2.7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.6 1.6 0 0 0 11 4a2 2 0 1 1 4 0 1.6 1.6 0 0 0 2.7 1.1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A1.6 1.6 0 0 0 20 9a2 2 0 1 1 0 4z" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21a8 8 0 0 1 16 0" />
    </>
  ),
  database: (
    <>
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v6c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
      <path d="M4 11v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6" />
    </>
  ),
  users: (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M2.5 21a6.5 6.5 0 0 1 13 0" />
      <path d="M16 5.2a3.5 3.5 0 0 1 0 6.6M17.5 14.3A6.5 6.5 0 0 1 21.5 21" />
    </>
  ),
  puzzle: (
    <path d="M9 4a2 2 0 0 1 4 0c0 .7.6 1.2 1.3 1H16a1 1 0 0 1 1 1v1.7c-.2.7.3 1.3 1 1.3a2 2 0 0 1 0 4c-.7 0-1.2.6-1 1.3V17a1 1 0 0 1-1 1h-1.7c-.7-.2-1.3.3-1.3 1a2 2 0 0 1-4 0c0-.7-.6-1.2-1.3-1H5a1 1 0 0 1-1-1v-2.7C4.2 13.6 3.7 13 3 13a2 2 0 0 1 0-4c.7 0 1.2-.6 1-1.3V6a1 1 0 0 1 1-1h2.7C8.4 5.2 9 4.7 9 4z" />
  ),
  spinner: <path d="M12 3a9 9 0 1 0 9 9" />,
  sliders: (
    <>
      <path d="M4 6h10M18 6h2M4 12h2M10 12h10M4 18h7M15 18h5" />
      <circle cx="16" cy="6" r="2" />
      <circle cx="8" cy="12" r="2" />
      <circle cx="13" cy="18" r="2" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5M12 8h.01" />
    </>
  ),
};

export default function Icon({
  name,
  className,
  size,
}: {
  name: IconName;
  className?: string;
  size?: number;
}) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      width={size ?? "1em"}
      height={size ?? "1em"}
      aria-hidden="true"
      className={className}
    >
      {PATHS[name]}
    </svg>
  );
}
