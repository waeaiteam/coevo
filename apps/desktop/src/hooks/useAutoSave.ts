import { useCallback, useEffect, useRef, useState } from 'react';

export type SaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

export interface UseAutoSaveOptions<T> {
  data: T;
  onSave: (data: T) => Promise<void>;
  delay?: number;
  enabled?: boolean;
}

export interface AutoSaveController {
  status: SaveStatus;
  lastSaved: number | null;
  error: Error | null;
  saveNow: () => Promise<void>;
}

/**
 * Debounced auto-save with an offline retry queue.
 * Serializes `data` to detect real changes, saves after `delay` ms of quiet,
 * and retries the last failed payload on the next change or manual saveNow().
 */
export function useAutoSave<T>({
  data,
  onSave,
  delay = 1500,
  enabled = true,
}: UseAutoSaveOptions<T>): AutoSaveController {
  const [status, setStatus] = useState<SaveStatus>('idle');
  const [lastSaved, setLastSaved] = useState<number | null>(null);
  const [error, setError] = useState<Error | null>(null);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastSerializedRef = useRef<string>('');
  const pendingRef = useRef<T | null>(null);
  const savingRef = useRef(false);

  const flush = useCallback(async () => {
    if (savingRef.current) return;
    const payload = pendingRef.current;
    if (payload === null) return;

    savingRef.current = true;
    setStatus('saving');
    setError(null);
    try {
      await onSave(payload);
      lastSerializedRef.current = JSON.stringify(payload);
      pendingRef.current = null;
      setStatus('saved');
      setLastSaved(Date.now());
    } catch (e) {
      // Keep payload in the queue for the next attempt.
      setStatus('error');
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      savingRef.current = false;
    }
  }, [onSave]);

  const saveNow = useCallback(async () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    pendingRef.current = data;
    await flush();
  }, [data, flush]);

  useEffect(() => {
    if (!enabled) return;
    const serialized = JSON.stringify(data);
    if (serialized === lastSerializedRef.current) return;

    pendingRef.current = data;
    setStatus('pending');

    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      void flush();
    }, delay);

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [data, delay, enabled, flush]);

  return { status, lastSaved, error, saveNow };
}
