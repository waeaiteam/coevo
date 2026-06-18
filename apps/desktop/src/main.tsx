import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import "./styles/globals.css";

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.stack || error.message;
  }
  return String(error);
}

// Friendly, jargon-free fatal-error screen. The diagnostic stack is preserved but
// collapsed inside a <details> so ordinary users see a calm message + clear actions,
// while power users can still expand and copy the full trace.
function renderFatalError(error: unknown) {
  const root = document.getElementById("root");
  if (!root) return;
  const detail = formatError(error);
  const zh = (navigator.language || "").toLowerCase().startsWith("zh");
  const text = zh
    ? {
        title: "coevo 遇到问题，暂时无法启动",
        body: "应用在准备工作区时遇到了一个错误。你可以重启试试，或者把错误信息发给技术支持。",
        copy: "复制错误信息",
        copied: "已复制",
        logs: "打开日志",
        restart: "重启",
        details: "查看技术详情",
      }
    : {
        title: "coevo couldn't start",
        body: "Something went wrong while preparing your workspace. Try restarting, or send the error details to support.",
        copy: "Copy error details",
        copied: "Copied",
        logs: "Open logs",
        restart: "Restart",
        details: "Technical details",
      };
  root.innerHTML = "";

  const shell = document.createElement("div");
  shell.style.minHeight = "100vh";
  shell.style.display = "flex";
  shell.style.alignItems = "center";
  shell.style.justifyContent = "center";
  shell.style.padding = "32px";
  shell.style.background = "#0b1020";
  shell.style.color = "#f8fafc";
  shell.style.fontFamily = "system-ui, -apple-system, Segoe UI, sans-serif";

  const card = document.createElement("div");
  card.style.width = "100%";
  card.style.maxWidth = "560px";
  card.style.border = "1px solid rgba(148, 163, 184, 0.24)";
  card.style.borderRadius = "12px";
  card.style.background = "rgba(15, 23, 42, 0.96)";
  card.style.padding = "28px";
  card.style.boxShadow = "0 24px 60px rgba(2, 6, 23, 0.4)";

  const title = document.createElement("h1");
  title.textContent = text.title;
  title.style.margin = "0 0 12px";
  title.style.fontSize = "20px";

  const copy = document.createElement("p");
  copy.textContent = text.body;
  copy.style.margin = "0 0 20px";
  copy.style.color = "#cbd5e1";
  copy.style.fontSize = "14px";
  copy.style.lineHeight = "1.6";

  const actions = document.createElement("div");
  actions.style.display = "flex";
  actions.style.flexWrap = "wrap";
  actions.style.gap = "10px";
  actions.style.marginBottom = "18px";

  function makeButton(label: string, primary: boolean) {
    const b = document.createElement("button");
    b.textContent = label;
    b.style.cursor = "pointer";
    b.style.border = "1px solid rgba(148, 163, 184, 0.3)";
    b.style.borderRadius = "8px";
    b.style.padding = "9px 14px";
    b.style.fontSize = "13px";
    b.style.fontWeight = "600";
    b.style.background = primary ? "#f8fafc" : "transparent";
    b.style.color = primary ? "#0b1020" : "#e2e8f0";
    return b;
  }

  const restartBtn = makeButton(text.restart, true);
  restartBtn.addEventListener("click", () => window.location.reload());

  const copyBtn = makeButton(text.copy, false);
  copyBtn.addEventListener("click", () => {
    void navigator.clipboard?.writeText(detail).then(() => {
      copyBtn.textContent = text.copied;
      setTimeout(() => (copyBtn.textContent = text.copy), 1500);
    });
  });

  const logsBtn = makeButton(text.logs, false);
  logsBtn.addEventListener("click", () => {
    const invoke = (window as unknown as { __TAURI__?: { core?: { invoke?: (cmd: string) => Promise<unknown> } } }).__TAURI__?.core?.invoke;
    if (invoke) void invoke("open_logs_dir").catch(() => undefined);
  });

  actions.append(restartBtn, copyBtn, logsBtn);

  const details = document.createElement("details");
  const summary = document.createElement("summary");
  summary.textContent = text.details;
  summary.style.cursor = "pointer";
  summary.style.color = "#94a3b8";
  summary.style.fontSize = "12px";
  details.append(summary);

  const pre = document.createElement("pre");
  pre.textContent = detail;
  pre.style.margin = "12px 0 0";
  pre.style.padding = "14px";
  pre.style.borderRadius = "10px";
  pre.style.overflow = "auto";
  pre.style.maxHeight = "240px";
  pre.style.whiteSpace = "pre-wrap";
  pre.style.wordBreak = "break-word";
  pre.style.background = "rgba(2, 6, 23, 0.72)";
  pre.style.color = "#94a3b8";
  pre.style.fontSize = "11px";
  pre.style.lineHeight = "1.6";
  details.append(pre);

  card.append(title, copy, actions, details);
  shell.append(card);
  root.append(shell);
}

window.addEventListener("error", (event) => {
  if (event.error) {
    renderFatalError(event.error);
  }
});

window.addEventListener("unhandledrejection", (event) => {
  renderFatalError(event.reason);
});

async function startApp() {
  try {
    const { default: App } = await import("./App");
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </React.StrictMode>
    );
  } catch (error) {
    console.error("Fatal startup error", error);
    renderFatalError(error);
  }
}

void startApp();
