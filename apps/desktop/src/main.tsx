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

function renderFatalError(error: unknown) {
  const root = document.getElementById("root");
  if (!root) return;
  root.innerHTML = "";

  const shell = document.createElement("div");
  shell.style.minHeight = "100vh";
  shell.style.display = "flex";
  shell.style.alignItems = "center";
  shell.style.justifyContent = "center";
  shell.style.padding = "32px";
  shell.style.background = "#0b1020";
  shell.style.color = "#f8fafc";

  const card = document.createElement("div");
  card.style.width = "100%";
  card.style.maxWidth = "880px";
  card.style.border = "1px solid rgba(148, 163, 184, 0.24)";
  card.style.borderRadius = "12px";
  card.style.background = "rgba(15, 23, 42, 0.96)";
  card.style.padding = "24px";
  card.style.boxShadow = "0 24px 60px rgba(2, 6, 23, 0.4)";

  const title = document.createElement("h1");
  title.textContent = "coevo failed to start";
  title.style.margin = "0 0 12px";
  title.style.fontSize = "22px";

  const copy = document.createElement("p");
  copy.textContent = "A fatal startup error stopped the app before the workspace could render.";
  copy.style.margin = "0 0 16px";
  copy.style.color = "#cbd5e1";

  const pre = document.createElement("pre");
  pre.textContent = formatError(error);
  pre.style.margin = "0";
  pre.style.padding = "16px";
  pre.style.borderRadius = "10px";
  pre.style.overflow = "auto";
  pre.style.whiteSpace = "pre-wrap";
  pre.style.wordBreak = "break-word";
  pre.style.background = "rgba(15, 23, 42, 0.72)";
  pre.style.color = "#fda4af";
  pre.style.fontSize = "12px";
  pre.style.lineHeight = "1.6";

  card.append(title, copy, pre);
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
