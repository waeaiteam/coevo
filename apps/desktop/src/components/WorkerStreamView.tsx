import { useState } from "react";
import Icon from "./Icon";
import { StreamingText } from "./StreamingText";
import { useWorkerStream, type StreamController } from "../hooks/useWorkerStream";
import { t } from "../settings/i18n";

/**
 * WorkerStreamView renders the real-time execution of a worker run.
 * Shows: thinking collapsible + content streaming + tool call cards + token footnote.
 */
export function WorkerStreamView({ runId }: { runId: string }) {
  const stream = useWorkerStream({ runId, autoStart: true });
  return <StreamDisplay stream={stream} />;
}

export function StreamDisplay({ stream }: { stream: StreamController }) {
  const [thinkingOpen, setThinkingOpen] = useState(true);
  const isStreaming = stream.state === "streaming" || stream.state === "connecting";
  const isDone = stream.state === "completed";
  const hasReasoning = Boolean(stream.reasoning);

  return (
    <div className="stream-view">
      {hasReasoning && (
        <details className="stream-thinking" open={thinkingOpen}>
          <summary
            className="stream-thinking-summary"
            onClick={(event) => {
              event.preventDefault();
              setThinkingOpen((open) => !open);
            }}
          >
            <Icon name="sparkles" />
            <span>{isStreaming ? t("stream.thinking") : t("stream.thinking_done")}</span>
            {isStreaming && <Icon name="spinner" className="icon-spin" />}
          </summary>
          <div className="stream-thinking-body">
            <StreamingText content={stream.reasoning} isStreaming={isStreaming} />
          </div>
        </details>
      )}

      {stream.content && (
        <div className="stream-content">
          <StreamingText content={stream.content} isStreaming={isStreaming} />
        </div>
      )}

      {stream.toolExecutions.length > 0 && (
        <div className="stream-tools">
          {stream.toolExecutions.map((tool, idx) => (
            <ToolCard key={`${tool.tool_name}-${idx}`} tool={tool} />
          ))}
        </div>
      )}

      {isDone && stream.usage && (
        <div className="stream-usage">
          <span className="mono-chip">{stream.usage.prompt_tokens + stream.usage.completion_tokens} tokens</span>
          <span className="mono-chip">{t("stream.prompt_tokens")}: {stream.usage.prompt_tokens}</span>
          <span className="mono-chip">{t("stream.completion_tokens")}: {stream.usage.completion_tokens}</span>
        </div>
      )}

      {stream.reconnecting && (
        <div className="stream-reconnecting">
          <Icon name="spinner" className="icon-spin" />
          <span>{t("stream.reconnecting")}</span>
        </div>
      )}

      {stream.state === "error" && stream.error && (
        <div className="stream-error">
          <Icon name="alert" />
          <span>{stream.error.message}</span>
          <button type="button" className="product-link-button" onClick={stream.retry}>
            {t("stream.retry")}
          </button>
        </div>
      )}

      {stream.state === "connecting" && !stream.content && !stream.reasoning && (
        <div className="stream-connecting">
          <Icon name="spinner" className="icon-spin" />
          <span>{t("stream.connecting")}</span>
        </div>
      )}
    </div>
  );
}

function ToolCard({ tool }: { tool: StreamController["toolExecutions"][number] }) {
  const [open, setOpen] = useState(false);

  return (
    <div className={`stream-tool-card ${tool.status}`}>
      <button type="button" className="stream-tool-head" onClick={() => setOpen((value) => !value)}>
        <span className="stream-tool-status">
          {tool.status === "running" && <Icon name="spinner" className="icon-spin" />}
          {tool.status === "completed" && <Icon name="check" style={{ color: "var(--green)" }} />}
          {tool.status === "failed" && <Icon name="x" style={{ color: "var(--red)" }} />}
        </span>
        <span className="stream-tool-name">{tool.tool_name}</span>
        {tool.duration_ms != null && tool.duration_ms > 0 && <span className="mono-chip">{tool.duration_ms}ms</span>}
        <Icon name="chevron-right" style={{ transform: open ? "rotate(90deg)" : undefined, transition: "transform 0.15s" }} />
      </button>
      {open && (
        <div className="stream-tool-body">
          {tool.arguments && (
            <div className="stream-tool-section">
              <div className="stream-tool-label">{t("stream.tool_args")}</div>
              <pre className="stream-tool-pre">{formatJson(tool.arguments)}</pre>
            </div>
          )}
          {tool.result && (
            <div className="stream-tool-section">
              <div className="stream-tool-label">{t("stream.tool_result")}</div>
              <pre className="stream-tool-pre">{tool.result.slice(0, 2000)}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function formatJson(str: string): string {
  try {
    return JSON.stringify(JSON.parse(str), null, 2);
  } catch {
    return str;
  }
}
