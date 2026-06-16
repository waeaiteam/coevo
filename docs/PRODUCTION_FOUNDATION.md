# coevo-opc Alpha Production Foundation

## Status: Alpha Foundation, Not Production V1

This document lists the current Alpha foundation and the remaining items before Private Beta. It should not be read as a Production V1 readiness claim.

## Current Capabilities

- MissionChat -> MCL compile -> PCDT route -> WorkOrder -> governed execution.
- MissionChat conversations persist as local threads/messages and link generated WorkOrders back to their originating conversation.
- Contract anchors are persisted by `/mcl/compile`; plan anchors are persisted by `/router/route`.
- WorkOrder governance fields are server-authoritative at create time; legacy client-supplied governance fields remain only for backward-compatible request shape and are ignored by the server classifier.
- Model Gateway supports OpenAI-compatible provider configuration. Mock remains developer and CI infrastructure only.
- Windows Alpha credential vault: new non-empty API-key writes store the secret in the native credential store and keep only a `keyring:` reference in SQLite.
- API keys are masked in responses and must not be logged.
- WorkerHarness owns the Product Harness authority envelope, while Agent Sub-Harness owns behavior-preserving agent runtime work inside that envelope.
- WorkerHarness records queue, sessions, runs, steps, events, tool calls, memory evidence, and audit export.
- Scoped `FileReadonlyTool` supports Green read/analyze execution under the local workspace.
- SkillRuntime, ToolPolicy, ToolRegistry.
- MemoryContext with provenance filtering.
- Reflection and SelfUpgradeLoop.
- Red Track creates persisted explicit approval requests and resumes only with an approved receipt.
- Yellow Track creates persisted approval requests and requires an approved receipt before execution.
- Synthetic end-to-end tests for development and CI.
- Desktop app with local sidecar launch and WorkerHarness visualization.

## Still Mock / Not Production

- External Executor adapters remain mock/dry-run focused for Hermes, OpenClaw, MCP, and 302AI.
- Model providers beyond OpenAI-compatible are planned.
- Mock Provider is not ordinary user onboarding; it is for deterministic dev/CI only.
- Credential vault is Windows-first. Non-Windows vault support is unavailable in Alpha.
- Existing Alpha databases with legacy plaintext API-key rows remain readable until a migration rewrites them into keyring references.
- Red Track uses explicit approval gating, but production MFA / dual-sign / lease-backed execution remains incomplete.
- Yellow approval has receipt validation, but the full user-facing approval management UI is still evolving.
- Track classification is keyword-based and intentionally over-classifies upward in Alpha.
- Vector memory is not implemented.
- UI and audit viewer surfaces are still evolving.

## Pre-Private Beta Checklist

- [ ] Add legacy plaintext credential migration to keyring refs.
- [ ] Add non-Windows credential vault support.
- [ ] Real GitHub executor.
- [ ] Real Browser executor.
- [ ] Real 302AI / OpenClaw / Hermes adapters.
- [ ] Production MFA / lease for Red Track.
- [ ] Full Yellow approval management UI.
- [ ] Versioned governance policy for track classification.
- [ ] Vector memory search.
- [ ] Package installers (MSI, DMG, AppImage).
- [ ] Crash recovery.
- [ ] Update mechanism.

## Manual Model Test

See [MANUAL_MODEL_TEST.md](../MANUAL_MODEL_TEST.md).

## Security

Model provides cognition, not authorization.
Worker executes under WorkOrder, RiskGate, approval state, and ToolPolicy governance.
External Executors are governed execution workers, not free agents.
API keys are masked in all outputs.
Red Track remains non-production in Alpha until production identity proof, dual-sign, MFA, and lease verification exist; the current behavior is explicit approval gating rather than a hard block.
