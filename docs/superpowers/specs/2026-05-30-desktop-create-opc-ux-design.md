# Desktop Create OPC UX Design

## Goal

Make the installed desktop app feel like an agent product instead of an internal admin console. First launch starts with creating a local OPC, model setup uses provider presets and discovered model choices, and the main sidebar exposes only the primary workflow.

## Current Problems

- First launch says "Configure Model Provider", so the product begins with infrastructure instead of the user's OPC.
- Model provider selection still forces users to type base URLs, model IDs, max tokens, timeouts, and cost caps.
- The desktop API client sends `x-coevo-tenant-id: desktop-tenant`, which fails server metadata validation because the server requires UUIDv4.
- MissionChat hard-codes `default-founder` and `default-opc`.
- The sidebar exposes low-level implementation modules as primary navigation.
- Settings mixes ordinary user choices with governance internals and developer diagnostics.

## Product Design

### First Run

The first screen is "Create your OPC". The user enters an OPC name, an owner name, and a language. The app creates and stores a local identity:

- `coevo-tenant-id`: UUIDv4 used for metadata headers.
- `coevo-opc-id`: UUIDv4 used in WorkOrders.
- `coevo-user-id`: stable founder id, defaulting to `default-founder` for compatibility.
- `coevo-opc-name`, `coevo-user-name`, and `coevo-language`.

After this, the onboarding moves to "Connect a model". The default provider is OpenAI. The user should only need to choose a provider and paste an API key.

### Model Provider

Provider selection applies a preset:

- OpenAI: `https://api.openai.com/v1`
- DeepSeek: `https://api.deepseek.com/v1`
- OpenAI Compatible: custom base URL shown in Advanced
- Anthropic, Gemini, Ollama, Local: kept available but not treated as the default path

After the key is tested, the app discovers available models and populates model dropdowns for default, fast, reasoning, and structured output roles. If discovery fails but connectivity succeeds, the app falls back to curated provider defaults and explains that model discovery was unavailable.

Base URL, manual model IDs, max tokens, temperature, timeout, and cost cap live in Advanced settings.

### Main Navigation

The sidebar primary entries are:

- New Chat
- OPC
- WorkOrders
- Audit
- Settings

Existing internal pages remain routable for now but are not primary navigation. Settings/Developer can expose advanced entry points later.

### Error Handling

Mission creation errors should surface the server's detail string. The UI should not collapse metadata validation, model gateway failure, and governance denial into a generic `HTTP 403`.

## Architecture

- Add `src/settings/identity.ts` for local OPC/user/tenant identity and UUIDv4 generation.
- Add `src/settings/modelPresets.ts` for provider presets, curated defaults, and model role selection.
- Extend the model gateway with a discovery method that normalizes OpenAI-compatible `/models` responses.
- Add `POST /opc/models/discover` for candidate config discovery without persisting API keys.
- Rework FirstRun as a two-step Create OPC and Connect Model flow.
- Rework ModelProviderPanel around presets, discovery, role dropdowns, and an Advanced disclosure.
- Slim Sidebar links without deleting routes.

## Tests

- API headers use a persisted UUIDv4 tenant id.
- FirstRun starts at "Create your OPC" and persists OPC identity.
- Model settings hide base URL and model text fields until Advanced.
- Save/Test calls discovery and uses discovered model IDs in role selectors.
- Sidebar only exposes the primary IA entries.
- MissionChat uses persisted user and OPC ids in WorkOrder creation.
- Backend discovery normalizes OpenAI-compatible model IDs and does not persist config.
