export const MODEL_PROVIDER_CONFIGURED_KEY = "coevo-model-provider-configured";

export function isModelProviderConfigured(): boolean {
  try {
    return localStorage.getItem(MODEL_PROVIDER_CONFIGURED_KEY) === "true";
  } catch {
    return false;
  }
}

export function clearModelProviderConfigured(): void {
  try {
    localStorage.removeItem(MODEL_PROVIDER_CONFIGURED_KEY);
  } catch {
    /* localStorage may be unavailable in restricted contexts */
  }
}

export function markModelProviderConfigured(): void {
  try {
    localStorage.setItem(MODEL_PROVIDER_CONFIGURED_KEY, "true");
  } catch {
    /* localStorage may be unavailable in restricted contexts */
  }
}
