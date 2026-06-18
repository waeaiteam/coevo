import type { ProviderType } from "./types";

export type ProviderPreset = {
  provider: ProviderType;
  label: string;
  kind: string;
  baseUrl: string;
  defaultModel: string;
  fastModel: string;
  reasoningModel: string;
  structuredModel: string;
  maxTokens: number;
  advancedBaseUrl: boolean;
  apiKeyHelpUrl: string;
};

export type DiscoveredModel = {
  id: string;
  display_name?: string;
  max_context_tokens?: number;
  max_output_tokens?: number;
  supports_json?: boolean;
  supports_reasoning?: boolean;
};

export const PROVIDER_PRESETS: Record<ProviderType, ProviderPreset> = {
  openai: {
    provider: "openai",
    label: "OpenAI",
    kind: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    defaultModel: "gpt-4o",
    fastModel: "gpt-4o-mini",
    reasoningModel: "o3-mini",
    structuredModel: "gpt-4o",
    maxTokens: 16384,
    advancedBaseUrl: false,
    apiKeyHelpUrl: "https://platform.openai.com/api-keys",
  },
  deepseek: {
    provider: "deepseek",
    label: "DeepSeek",
    kind: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    defaultModel: "deepseek-chat",
    fastModel: "deepseek-chat",
    reasoningModel: "deepseek-reasoner",
    structuredModel: "deepseek-chat",
    maxTokens: 8192,
    advancedBaseUrl: false,
    apiKeyHelpUrl: "https://platform.deepseek.com/api_keys",
  },
  "openai-compatible": {
    provider: "openai-compatible",
    label: "OpenAI Compatible",
    kind: "OpenAICompatible",
    baseUrl: "https://api.openai.com/v1",
    defaultModel: "gpt-4o",
    fastModel: "gpt-4o-mini",
    reasoningModel: "o3-mini",
    structuredModel: "gpt-4o",
    maxTokens: 16384,
    advancedBaseUrl: true,
    apiKeyHelpUrl: "https://platform.openai.com/api-keys",
  },
  anthropic: {
    provider: "anthropic",
    label: "Anthropic",
    kind: "Anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    defaultModel: "claude-3-5-sonnet-latest",
    fastModel: "claude-3-5-haiku-latest",
    reasoningModel: "claude-3-5-sonnet-latest",
    structuredModel: "claude-3-5-sonnet-latest",
    maxTokens: 8192,
    advancedBaseUrl: true,
    apiKeyHelpUrl: "https://console.anthropic.com/settings/keys",
  },
  gemini: {
    provider: "gemini",
    label: "Gemini",
    kind: "Gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    defaultModel: "gemini-1.5-pro",
    fastModel: "gemini-1.5-flash",
    reasoningModel: "gemini-1.5-pro",
    structuredModel: "gemini-1.5-pro",
    maxTokens: 8192,
    advancedBaseUrl: true,
    apiKeyHelpUrl: "https://aistudio.google.com/app/apikey",
  },
  ollama: {
    provider: "ollama",
    label: "Ollama",
    kind: "Ollama",
    baseUrl: "http://127.0.0.1:11434/v1",
    defaultModel: "llama3.1",
    fastModel: "llama3.1",
    reasoningModel: "llama3.1",
    structuredModel: "llama3.1",
    maxTokens: 4096,
    advancedBaseUrl: true,
    apiKeyHelpUrl: "https://ollama.com/download",
  },
  local: {
    provider: "local",
    label: "Local",
    kind: "Local",
    baseUrl: "http://127.0.0.1:8080/v1",
    defaultModel: "local-model",
    fastModel: "local-model",
    reasoningModel: "local-model",
    structuredModel: "local-model",
    maxTokens: 4096,
    advancedBaseUrl: true,
    apiKeyHelpUrl: "http://127.0.0.1:8080",
  },
};

export function providerOptions() {
  return Object.values(PROVIDER_PRESETS).map((p) => ({ value: p.provider, label: p.label }));
}

export function presetFor(provider: ProviderType | string): ProviderPreset {
  return PROVIDER_PRESETS[provider as ProviderType] || PROVIDER_PRESETS.openai;
}

export function isKnownProvider(provider: ProviderType | string): provider is ProviderType {
  return Boolean(PROVIDER_PRESETS[provider as ProviderType]);
}

export function chooseModelRoles(models: DiscoveredModel[], preset: ProviderPreset) {
  const ids = models.map((m) => m.id).filter(Boolean);
  const by = (fn: (id: string) => boolean) => ids.find((id) => fn(id.toLowerCase()));
  const fallback = ids[0] || preset.defaultModel;
  const defaultModel = by((id) => id.includes("gpt-4o") || id.includes("gpt-4.1") || id.includes("deepseek-chat")) || fallback;
  const fastModel = by((id) => id.includes("mini") || id.includes("flash") || id.includes("haiku") || id.includes("small")) || defaultModel;
  const reasoningModel = ids.find((id) => models.find((m) => m.id === id)?.supports_reasoning)
    || by((id) => id.startsWith("o") || id.includes("reason") || id.includes("thinking"))
    || defaultModel;
  const structuredModel = ids.find((id) => models.find((m) => m.id === id)?.supports_json) || defaultModel;
  const chosenMeta = models.find((m) => m.id === defaultModel);
  return {
    default_model: defaultModel,
    fast_model: fastModel,
    reasoning_model: reasoningModel,
    structured_output_model: structuredModel,
    max_tokens: chosenMeta?.max_output_tokens || chosenMeta?.max_context_tokens || preset.maxTokens,
  };
}
