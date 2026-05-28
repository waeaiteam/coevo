# coevo-opc Manual Model Test

This guide is for real model testing with an API key.
CI and automated tests always use the Mock provider (no key required).

## Prerequisites

- coevo server running at `http://127.0.0.1:8717`
- An OpenAI-compatible API key (or real provider)
- Desktop app running (`npm run dev` or `npm run tauri dev`)

## Steps

### 1. Open Settings → Model Providers

Navigate to `/settings/model_provider` in the desktop app.

### 2. Configure Provider

- **Provider**: Select `OpenAI Compatible`
- **Base URL**: Enter your provider URL (e.g., `https://api.openai.com/v1`)
- **API Key**: Paste your key (do NOT commit to GitHub)
- **Default Model**: e.g., `gpt-4o`
- **Fast Model**: e.g., `gpt-4o-mini`

### 3. Test Connection

Click **Test Connection**.
Expected: Success with latency, model name, provider info.

If `MissingApiKey`: API key was not saved.
If `ProviderUnreachable`: Check base_url.

### 4. Return to MissionChat

Go to `/` (Mission Composer).

### 5. Send a Mission

Enter:
```
帮我总结 coevo-opc 当前进展，并给出下一阶段路线图。
```

### 6. Observe

- Mission Draft should show model-enhanced suggestions (if real provider)
- If provider is Mock, fallback to deterministic track inference
- Synthesizer should produce a summary after Green execution
- Skill Evolution should create model-enhanced proposals

## Security Notes

- Never commit `api_key` to source code
- Never log `api_key` in server logs
- GET `/opc/models/config` masks the key as `sk-****abcd`
- All model output is governed by MCL, RiskGate, and SkillVerifier
- Models provide cognition, NOT authorization
