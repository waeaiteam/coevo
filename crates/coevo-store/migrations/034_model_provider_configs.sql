CREATE TABLE IF NOT EXISTS model_provider_configs (
    provider_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('Mock','OpenAICompatible','OpenAI','Anthropic','Gemini','DeepSeek','Ollama','Local')),
    base_url TEXT NOT NULL DEFAULT '',
    api_key_ciphertext TEXT NOT NULL DEFAULT '',
    api_key_masked TEXT NOT NULL DEFAULT '',
    default_model TEXT NOT NULL DEFAULT '',
    fast_model TEXT NOT NULL DEFAULT '',
    reasoning_model TEXT NOT NULL DEFAULT '',
    structured_output_model TEXT NOT NULL DEFAULT '',
    max_tokens INTEGER NOT NULL DEFAULT 4096,
    temperature REAL NOT NULL DEFAULT 0.7,
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    max_cost_per_task_usd REAL NOT NULL DEFAULT 0.0,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
