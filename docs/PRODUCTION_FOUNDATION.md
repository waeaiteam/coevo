# coevo-opc Production-Grade Foundation

## Status: Production-Grade Foundation Ready

This document lists current production-grade capabilities and remaining items
before Private Beta.

## Current Capabilities

- ✅ MissionChat → Mission Draft → WorkOrder → Execute
- ✅ Model Gateway with Mock + OpenAI-compatible
- ✅ Model config persisted to SQLite (survives restart)
- ✅ API key masked in responses, never in logs
- ✅ WorkerHarness with queue, session, steps, events
- ✅ SkillRuntime, ToolPolicy, ToolRegistry
- ✅ MemoryContext with provenance filtering
- ✅ Reflection and SelfUpgradeLoop
- ✅ Red Track blocked by default
- ✅ Yellow Track WaitingApproval
- ✅ GitHubReadonlyTool, FileReadonlyTool
- ✅ Synthetic end-to-end test
- ✅ CI workflow (backend, frontend, synthetic)
- ✅ Desktop app with WorkerHarness visualization

## Still Mock / Not Production

- External Executors: mock adapters only (Hermes, OpenClaw, MCP, 302AI)
- Model providers: only Mock + OpenAI-compatible. Others are planned.
- Credential Vault: API keys stored in SQLite (Alpha). Replace before Private Beta.
- MFA / Lease: Red Track blocking is runtime logic, not production MFA.
- Vector memory: not implemented.
- UI: evolving, not final.

## Pre-Private Beta Checklist

- [ ] Replace api_key_ciphertext with OS keychain / credential vault
- [ ] Real GitHub executor
- [ ] Real Browser executor
- [ ] Real 302AI / OpenClaw / Hermes adapters
- [ ] Production MFA / lease for Red Track
- [ ] Vector memory search
- [ ] Package installers (MSI, DMG, AppImage)
- [ ] Crash recovery
- [ ] Update mechanism

## Manual Model Test

See [MANUAL_MODEL_TEST.md](MANUAL_MODEL_TEST.md).

## Security

Model provides cognition, NOT authorization.
Worker executes under WorkOrder, RiskGate, and ToolPolicy governance.
External Executors are governed execution workers, not free agents.
API keys are masked in all outputs.
Red Track requires identity proof and dual-sign.
