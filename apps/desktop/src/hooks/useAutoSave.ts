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
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastSerializedRef = useRef<string>('');
  const pendingRef = useRef<T | null>(null);
  const savingRef = useRef(false);

  // Serialize defensively: a circular structure must not crash auto-save. On failure we
  // treat the value as "always changed" (unique sentinel) so a real save is still attempted.
  const serialize = useCallback((value: T): string => {
    try {
      return JSON.stringify(value);
    } catch {
      return `__unserializable__:${Date.now()}`;
    }
  }, []);

  const flush = useCallback(async () => {
    if (savingRef.current) return;
    const payload = pendingRef.current;
    if (payload === null) return;

    savingRef.current = true;
    setStatus('saving');
    setError(null);
    try {
      await onSave(payload);
      lastSerializedRef.current = serialize(payload);
      pendingRef.current = null;
      setStatus('saved');
      setLastSaved(Date.now());
      if (retryRef.current) {
        clearTimeout(retryRef.current);
        retryRef.current = null;
      }
    } catch (e) {
      // Keep payload in the queue and schedule a background retry so a transient
      // failure recovers without requiring another edit or manual save.
      setStatus('error');
      setError(e instanceof Error ? e : new Error(String(e)));
      if (retryRef.current) clearTimeout(retryRef.current);
      retryRef.current = setTimeout(() => {
        retryRef.current = null;
        void flush();
      }, Math.max(2000, delay * 2));
    } finally {
      savingRef.current = false;
    }
  }, [onSave, serialize, delay]);

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
    const serialized = serialize(data);
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
  }, [data, delay, enabled, flush, serialize]);

  // Clean up any pending retry on unmount.
  useEffect(() => {
    return () => {
      if (retryRef.current) clearTimeout(retryRef.current);
    };
  }, []);

  return { status, lastSaved, error, saveNow };
}
