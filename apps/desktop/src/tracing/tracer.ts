// Span tracing system — OpenTelemetry-inspired, local-first.
// Captures Mission → SubTask → AgentAction hierarchy with timing, tokens, and status.

export type SpanStatus = 'running' | 'ok' | 'error';
export type SpanKind =
  | 'mission'
  | 'subtask'
  | 'agent_action'
  | 'tool_call'
  | 'model_call'
  | 'governance'
  | 'custom';

export interface SpanAttributes {
  [key: string]: string | number | boolean | null;
}

export interface Span {
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  name: string;
  kind: SpanKind;
  status: SpanStatus;
  start_time: number;
  end_time: number | null;
  duration_ms: number | null;
  attributes: SpanAttributes;
  input?: string;
  output?: string;
  error?: string;
  input_tokens?: number;
  output_tokens?: number;
  cost_usd?: number;
}

export interface Trace {
  trace_id: string;
  root_span_id: string;
  name: string;
  status: SpanStatus;
  start_time: number;
  end_time: number | null;
  duration_ms: number | null;
  span_count: number;
  total_tokens: number;
  total_cost_usd: number;
}

function genId(bytes: number): string {
  const arr = new Uint8Array(bytes);
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(arr);
  } else {
    for (let i = 0; i < bytes; i++) arr[i] = Math.floor(Math.random() * 256);
  }
  return Array.from(arr, (b) => b.toString(16).padStart(2, '0')).join('');
}

export type TraceListener = (span: Span) => void;

export class Tracer {
  private listeners = new Set<TraceListener>();

  subscribe(listener: TraceListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(span: Span): void {
    this.listeners.forEach((l) => {
      try {
        l(span);
      } catch {
        /* listener errors must not break tracing */
      }
    });
  }

  startSpan(
    name: string,
    kind: SpanKind,
    options?: {
      traceId?: string;
      parentSpanId?: string | null;
      attributes?: SpanAttributes;
      input?: string;
    }
  ): SpanHandle {
    const traceId = options?.traceId || genId(16);
    const span: Span = {
      trace_id: traceId,
      span_id: genId(8),
      parent_span_id: options?.parentSpanId ?? null,
      name,
      kind,
      status: 'running',
      start_time: Date.now(),
      end_time: null,
      duration_ms: null,
      attributes: options?.attributes || {},
      input: options?.input,
    };
    this.emit({ ...span });
    return new SpanHandle(span, this.emit.bind(this));
  }
}

export class SpanHandle {
  constructor(
    private span: Span,
    private emit: (span: Span) => void
  ) {}

  get traceId(): string {
    return this.span.trace_id;
  }

  get spanId(): string {
    return this.span.span_id;
  }

  setAttribute(key: string, value: string | number | boolean | null): this {
    this.span.attributes[key] = value;
    return this;
  }

  setTokens(input: number, output: number, costUsd = 0): this {
    this.span.input_tokens = input;
    this.span.output_tokens = output;
    this.span.cost_usd = costUsd;
    return this;
  }

  setOutput(output: string): this {
    this.span.output = output;
    return this;
  }

  child(name: string, kind: SpanKind, attributes?: SpanAttributes): SpanHandle {
    return new Tracer().startSpan(name, kind, {
      traceId: this.span.trace_id,
      parentSpanId: this.span.span_id,
      attributes,
    });
  }

  end(status: SpanStatus = 'ok', error?: string): void {
    this.span.end_time = Date.now();
    this.span.duration_ms = this.span.end_time - this.span.start_time;
    this.span.status = status;
    if (error) this.span.error = error;
    this.emit({ ...this.span });
  }
}

export const tracer = new Tracer();
