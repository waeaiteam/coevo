import { useSyncExternalStore } from "react";
import { getAdvancedMode, subscribeAdvancedMode } from "../settings/appMode";

/**
 * Reactive accessor for the global "advanced mode" flag.
 * See settings/appMode.ts for why this is a standalone store rather than part of
 * the Settings context.
 */
export function useAdvancedMode(): boolean {
  return useSyncExternalStore(subscribeAdvancedMode, getAdvancedMode, getAdvancedMode);
}
