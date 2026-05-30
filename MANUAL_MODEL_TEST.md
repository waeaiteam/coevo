# coevo-opc Manual Model Test

This guide is for developers and release testers who need to verify coevo with a real OpenAI-compatible model provider.

Mock is a test mode for CI, deterministic synthetic tests, and local developer checks. It is **not** the normal user onboarding path.

## Prerequisites

- coevo local core service running, either through the desktop sidecar or a developer server at `http://127.0.0.1:8717`
- Desktop app running through a packaged Alpha build, `npm run dev`, or `npm run tauri dev`
- An OpenAI-compatible API key
- For real credential persistence, use a Windows Alpha build. Non-Windows credential vault support is unavailable in Alpha.

## Steps

### 1. Open Settings -> Model Providers

In the desktop app, open **Settings -> Model Providers**.

### 2. Configure a Real Provider

- **Provider**: select `OpenAI Compatible`
- **Base URL**: enter your provider URL, for example `https://api.openai.com/v1`
- **API Key**: paste your key
- **Default Model**: for example `gpt-4o`
- **Fast Model**: for example `gpt-4o-mini`

Do not commit the API key. On Windows Alpha builds, new non-empty API keys are stored in the native credential vault and SQLite keeps a `keyring:` reference plus a masked display value. Existing Alpha databases with legacy plaintext keys remain readable until a migration rewrites them. Non-Windows credential vault support is not available in Alpha.

### 3. Save and Test Connection

Click **Save & Test Connection**.

Expected: success with latency, model name, and provider info.

Common failures:

- `MissingApiKey`: API key was not saved or was blank.
- `ProviderUnreachable`: check `base_url`, network access, and provider availability.
- Authentication error: check the key and provider account status.

### 4. Return to MissionChat

Go to MissionChat and send a real mission, for example:

```text
Summarize the current coevo-opc progress and propose the next roadmap.
```

### 5. Observe the Governance Path

- MissionChat should call the configured model for a cognition summary when available.
- MCL and RiskGate still decide the governance path; model output is not authorization.
- Green work may execute when policy allows.
- Yellow work should enter `WaitingApproval`, create an approval request, and require an approved approval receipt before execution.
- Red work is hard-blocked in Alpha.
- WorkOrder, WorkerSession, WorkerRunStep, WorkerEvent, Timeline, and audit records should remain inspectable.
- `Export Audit` on a WorkOrder should return a `coevo.audit_export.v1` audit package.

### 6. Optional Mock Regression Check

Use Mock only when validating deterministic development or CI behavior.

- Mock should produce deterministic MissionDraft / Synthesizer / SkillGenerator output.
- Mock does not require an API key.
- Mock should not be documented or presented as ordinary user onboarding.

## Security Notes

- Never commit `api_key` to source code.
- Never log `api_key` in server logs.
- `GET /opc/models/config` should mask the key.
- Model output and executor output cannot directly write Fact records.
- Fact writes require provenance through the Cognitive Customs path.
- Skills cannot silently elevate permissions.
- Executors are governed workers, not free agents.
- Models provide cognition, not authorization.
