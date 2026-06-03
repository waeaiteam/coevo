import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import type { Span, Trace } from '../tracing/tracer';
import { tracer } from '../tracing/tracer';

interface TraceState {
  spans: Record<string, Span[]>; // traceId -> spans
  traces: Record<string, Trace>;
  activeTraceId: string | null;

  ingestSpan: (span: Span) => void;
  setActiveTrace: (traceId: string | null) => void;
  getTraceSpans: (traceId: string) => Span[];
  clearTraces: () => void;
}

function recomputeTrace(spans: Span[]): Omit<Trace, 'trace_id'> | null {
  if (spans.length === 0) return null;
  const root = spans.find((s) => s.parent_span_id === null) || spans[0];
  const startTime = Math.min(...spans.map((s) => s.start_time));
  const ended = spans.every((s) => s.end_time !== null);
  const endTime = ended ? Math.max(...spans.map((s) => s.end_time || 0)) : null;
  const hasError = spans.some((s) => s.status === 'error');
  const totalTokens = spans.reduce(
    (sum, s) => sum + (s.input_tokens || 0) + (s.output_tokens || 0),
    0
  );
  const totalCost = spans.reduce((sum, s) => sum + (s.cost_usd || 0), 0);

  return {
    root_span_id: root.span_id,
    name: root.name,
    status: hasError ? 'error' : ended ? 'ok' : 'running',
    start_time: startTime,
    end_time: endTime,
    duration_ms: endTime ? endTime - startTime : null,
    span_count: spans.length,
    total_tokens: totalTokens,
    total_cost_usd: totalCost,
  };
}

export const useTraceStore = create<TraceState>()(
  immer((set, get) => ({
    spans: {},
    traces: {},
    activeTraceId: null,

    ingestSpan: (span: Span) =>
      set((state: TraceState) => {
        const list = state.spans[span.trace_id] || [];
        const idx = list.findIndex((s) => s.span_id === span.span_id);
        if (idx >= 0) {
          list[idx] = span;
        } else {
          list.push(span);
        }
        state.spans[span.trace_id] = list;

        const summary = recomputeTrace(list);
        if (summary) {
          state.traces[span.trace_id] = { trace_id: span.trace_id, ...summary };
        }
      }),

    setActiveTrace: (traceId: string | null) =>
      set((state: TraceState) => {
        state.activeTraceId = traceId;
      }),

    getTraceSpans: (traceId: string) => {
      return get().spans[traceId] || [];
    },

    clearTraces: () =>
      set((state: TraceState) => {
        state.spans = {};
        state.traces = {};
        state.activeTraceId = null;
      }),
  }))
);

// Wire the global tracer into the store so every span is captured.
let wired = false;
export function initTraceWiring(): void {
  if (wired) return;
  wired = true;
  tracer.subscribe((span) => {
    useTraceStore.getState().ingestSpan(span);
  });
}
