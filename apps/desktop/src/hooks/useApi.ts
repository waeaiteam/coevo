import { useState, useEffect } from "react";
import { getHealth, HealthResponse } from "../api/client";

export function useHealth() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Distinguish "still loading" from "loaded but empty" so callers can show a spinner
  // rather than a misleading empty/offline state on first paint.
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    getHealth()
      .then((h) => { if (alive) setHealth(h); })
      .catch((e) => { if (alive) setError(e instanceof Error ? e.message : String(e)); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, []);

  return { health, error, loading };
}
