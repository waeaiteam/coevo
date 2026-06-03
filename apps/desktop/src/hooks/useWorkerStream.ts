import { useCallback, useEffect, useRef, useState } from 'react';
import { streamWorkerRunEvents } from '../api/client';

export type StreamState = 'idle' | 'connecting' | 'streaming' | 'completed' | 'error';

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
  error: Error | null;
  start: () => void;
  stop: () => void;
  retry: () => void;
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
  const [error, setError] = useState<Error | null>(null);
  const closeRef = useRef<(() => void) | null>(null);
  const contentRef = useRef('');

  const stop = useCallback(() => {
    if (closeRef.current) {
      closeRef.current();
      closeRef.current = null;
    }
    setState('idle');
  }, []);

  const start = useCallback(() => {
    if (state === 'streaming' || state === 'connecting') return;

    setState('connecting');
    setError(null);
    contentRef.current = '';
    setContent('');

    const cleanup = streamWorkerRunEvents(
      runId,
      (event) => {
        if (event.event_type === 'AssistantDelta') {
          const delta = String(event.delta || '');
          contentRef.current += delta;
          setContent(contentRef.current);
          setState('streaming');
          onDelta?.(delta);
        } else if (event.event_type === 'LifecycleEnd') {
          setState('completed');
          onComplete?.(contentRef.current);
          closeRef.current = null;
        }
      },
      () => {
        const err = new Error('SSE connection failed, falling back to polling');
        setError(err);
        setState('error');
        onError?.(err);
      }
    );

    closeRef.current = cleanup;
  }, [runId, state, onDelta, onComplete, onError]);

  const retry = useCallback(() => {
    stop();
    setTimeout(() => start(), 100);
  }, [stop, start]);

  useEffect(() => {
    if (autoStart) {
      start();
    }
    return () => {
      stop();
    };
  }, [autoStart, start, stop]);

  return { state, content, error, start, stop, retry };
}
