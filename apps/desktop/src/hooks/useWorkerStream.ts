import { useCallback, useEffect, useRef, useState } from 'react';
import { streamWorkerRunEvents } from '../api/client';

export type StreamState = 'idle' | 'connecting' | 'streaming' | 'completed' | 'error';

export interface ToolExecution {
  index: number;
  tool_name: string;
  arguments: string;
  status: 'running' | 'completed' | 'failed';
  result?: string;
  duration_ms?: number;
  execution_id?: string;
}

export interface StreamUsage {
  prompt_tokens: number;
  completion_tokens: number;
}

export interface UseWorkerStreamOptions {
  runId: string;
  onDelta?: (text: string) => void;
  onComplete?: (fullText: string) => void;
  onError?: (error: Error) => void;
  autoStart?: boolean;
}

export interface StreamController {
  state: StreamState;
  content: string;
  reasoning: string;
  toolCalls: Array<{ index: number; arguments: string }>;
  toolExecutions: ToolExecution[];
  usage: StreamUsage | null;
  error: Error | null;
  reconnecting: boolean;
  reconnectAttempt: number;
  start: () => void;
  stop: () => void;
  retry: () => void;
}

function parsePayload(event: Record<string, unknown>): Record<string, unknown> {
  const payload = event.payload;
  if (payload && typeof payload === 'object') return payload as Record<string, unknown>;
  const raw = String(event.payload_json || '');
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

export function useWorkerStream({
  runId,
  onDelta,
  onComplete,
  onError,
  autoStart = false,
}: UseWorkerStreamOptions): StreamController {
  const [state, setState] = useState<StreamState>('idle');
  const [content, setContent] = useState('');
  const [reasoning, setReasoning] = useState('');
  const [toolCalls, setToolCalls] = useState<Array<{ index: number; arguments: string }>>([]);
  const [toolExecutions, setToolExecutions] = useState<ToolExecution[]>([]);
  const [usage, setUsage] = useState<StreamUsage | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [reconnecting, setReconnecting] = useState(false);
  const [reconnectAttempt, setReconnectAttempt] = useState(0);
  const closeRef = useRef<(() => void) | null>(null);
  const stateRef = useRef<StreamState>('idle');
  const contentRef = useRef('');
  const reasoningRef = useRef('');
  const toolCallsRef = useRef<Array<{ index: number; arguments: string }>>([]);
  // Hold the latest callbacks in a ref so `start` keeps a stable identity even when the
  // parent passes new inline closures each render. Without this, a parent that doesn't
  // memoize onDelta/onComplete/onError would rebuild `start` every render and the
  // autoStart effect would tear down and re-subscribe the stream — a reconnect storm.
  const callbacksRef = useRef({ onDelta, onComplete, onError });
  useEffect(() => {
    callbacksRef.current = { onDelta, onComplete, onError };
  }, [onDelta, onComplete, onError]);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const stop = useCallback(() => {
    if (closeRef.current) {
      closeRef.current();
      closeRef.current = null;
    }
    setState('idle');
  }, []);

  const start = useCallback(() => {
    if (stateRef.current === 'streaming' || stateRef.current === 'connecting') return;

    setState('connecting');
    setError(null);
    setReconnecting(false);
    setReconnectAttempt(0);
    contentRef.current = '';
    reasoningRef.current = '';
    toolCallsRef.current = [];
    setContent('');
    setReasoning('');
    setToolCalls([]);
    setToolExecutions([]);
    setUsage(null);

    const cleanup = streamWorkerRunEvents(
      runId,
      (event) => {
        const eventType = String(event.event_type || '');
        const payload = parsePayload(event);

        switch (eventType) {
          case 'AssistantDelta':
          case 'ContentDelta': {
            const delta = String(payload.delta ?? event.delta ?? '');
            contentRef.current += delta;
            setContent(contentRef.current);
            setState('streaming');
            callbacksRef.current.onDelta?.(delta);
            break;
          }
          case 'ReasoningDelta': {
            const delta = String(payload.delta ?? event.delta ?? '');
            reasoningRef.current += delta;
            setReasoning(reasoningRef.current);
            setState('streaming');
            break;
          }
          case 'ToolCallDelta': {
            const idx = Number(payload.index ?? 0);
            const argDelta = String(payload.arguments ?? '');
            const existing = toolCallsRef.current.find((tc) => tc.index === idx);
            if (existing) {
              existing.arguments += argDelta;
            } else {
              toolCallsRef.current.push({ index: idx, arguments: argDelta });
            }
            setToolCalls([...toolCallsRef.current]);
            setState('streaming');
            break;
          }
          case 'ToolStart': {
            const exec: ToolExecution = {
              index: toolCallsRef.current.length,
              tool_name: String(payload.tool_name ?? payload.name ?? ''),
              arguments: String(payload.arguments ?? ''),
              status: 'running',
            };
            // Prefer a server-provided execution id for exact ToolEnd matching; fall
            // back to the running index so concurrent same-named tools don't collide.
            const execId = String(payload.execution_id ?? payload.tool_call_id ?? payload.id ?? '');
            if (execId) exec.execution_id = execId;
            setToolExecutions((prev) => [...prev, exec]);
            setState('streaming');
            break;
          }
          case 'ToolEnd': {
            const toolName = String(payload.tool_name ?? payload.name ?? '');
            const execId = String(payload.execution_id ?? payload.tool_call_id ?? payload.id ?? '');
            setToolExecutions((prev) => {
              // Match by unique execution id when present; otherwise update the FIRST
              // still-running tool with the matching name (FIFO), so two concurrent
              // calls to the same tool resolve in order instead of overwriting each other.
              let matched = false;
              return prev.map((te) => {
                if (matched) return te;
                const isMatch = execId
                  ? te.execution_id === execId
                  : te.tool_name === toolName && te.status === 'running';
                if (!isMatch) return te;
                matched = true;
                return {
                  ...te,
                  status: (payload.error ? 'failed' : 'completed') as ToolExecution['status'],
                  result: String(payload.result ?? payload.output ?? ''),
                  duration_ms: Number(payload.duration_ms ?? 0),
                };
              });
            });
            break;
          }
          case 'Usage': {
            setUsage({
              prompt_tokens: Number(payload.prompt_tokens ?? 0),
              completion_tokens: Number(payload.completion_tokens ?? 0),
            });
            break;
          }
          case 'Done':
          case 'LifecycleEnd': {
            setState('completed');
            if (reasoningRef.current) setReasoning(reasoningRef.current);
            callbacksRef.current.onComplete?.(contentRef.current);
            closeRef.current = null;
            break;
          }
          default:
            break;
        }
      },
      () => {
        const err = new Error('SSE connection failed, falling back to polling');
        setError(err);
        setState('error');
        setReconnecting(false);
        callbacksRef.current.onError?.(err);
      },
      {
        onReconnecting: (attempt) => {
          setReconnecting(true);
          setReconnectAttempt(attempt);
        },
        onReconnected: () => {
          setReconnecting(false);
        },
      },
    );

    closeRef.current = cleanup;
  }, [runId]);

  const retry = useCallback(() => {
    stop();
    setTimeout(() => start(), 100);
  }, [stop, start]);

  useEffect(() => {
    if (autoStart) {
      start();
    }
    return () => {
      if (closeRef.current) {
        closeRef.current();
        closeRef.current = null;
      }
      stateRef.current = 'idle';
    };
  }, [autoStart, runId, start]);

  return { state, content, reasoning, toolCalls, toolExecutions, usage, error, reconnecting, reconnectAttempt, start, stop, retry };
}
