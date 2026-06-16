# coevo Security Boundaries

coevo's governing principle is:

> Internal reasoning is free. External behavior is governed.

This document defines the Alpha security boundaries for the AI Employee Operating System / Agent Governance Mesh.

## Boundary Summary

- Models provide cognition, not authorization.
- Executors perform governed work, not autonomous agency.
- Red Track is explicit-approval gated in Alpha.
- Facts require provenance.
- Model output and executor output cannot directly write Facts.
- Skills cannot automatically elevate their own permissions.
- WorkOrder, WorkerSession, WorkerRunStep, WorkerEvent, Timeline, and audit records must be inspectable.

## Model Boundary

Models may draft, summarize, classify, suggest, and reason.

Models must not:

- authorize actions;
- bypass MCL or RiskGate;
- assign themselves or workers additional privileges;
- mark external claims as Facts without provenance;
- write directly into governed Fact storage;
- override user approval requirements.

Any model response that proposes external behavior is treated as an input to governance, not as permission to act.

## Authorization Boundary

Authorization belongs to the governance runtime:

- MCL defines the mission contract and allowed shape of work.
- RiskGate assigns and enforces Green / Yellow / Red track behavior.
- Policy and approval state decide whether execution may continue.
- ADR-A / resolution components arbitrate conflict and escalation paths.

The model may explain why it thinks something should happen. The runtime decides whether it can happen.

## Executor Boundary

Executors are governed workers. They are not free agents.

Executors must:

- be registered before use;
- operate through WorkOrders and WorkerRunSteps;
- respect scope, capability, and risk ceilings;
- return outputs as observations, hypotheses, suggestions, or execution results according to contract;
- remain auditable through WorkerEvent and Timeline records.

Executor output must not directly become a Fact. It must pass through provenance and Cognitive Customs handling.

## Agent Sub-Harness Boundary

The Product / WorkerHarness layer issues the immutable execution authority envelope: WorkOrder id, selected agent, track, approval receipt, contract hash, plan hash, allowed actions, and restricted actions. The Agent Sub-Harness may assemble context, route models, load skills, propose tools, execute allowed read-only tools, write Task memory, reflect, and propose skill upgrades only inside that envelope.

The Agent Sub-Harness must not reload authority from prompt text, conversation messages, or mutable context snapshots. Tool execution must pass both the immutable Product Harness envelope and the live ToolPolicy check at execution time.

## Track Boundary

| Track | Alpha Behavior |
|---|---|
| Green | May execute scoped read/analyze work when MCL, RiskGate, ToolPolicy, and policy allow. |
| Yellow | Creates a persisted approval request and requires an approved receipt before governed execution. Arbitrary non-empty strings are rejected. |
| Red | Creates a persisted explicit approval request and requires an approved receipt before governed execution. Arbitrary non-empty strings are rejected. |

Red Track includes high-risk external behavior such as production writes, destructive operations, financial actions, irreversible changes, or actions requiring stronger identity and lease controls. Alpha does not provide production-grade Red identity, MFA, or lease enforcement; it stops at explicit approval gating before governed execution resumes.

## Fact and Provenance Boundary

Facts are governed memory artifacts and must carry provenance.

The following are not Facts by themselves:

- model answers;
- executor output;
- scraped or imported text;
- user-visible summaries;
- Skill-generated proposals;
- synthetic test data.

Before information becomes a Fact, coevo must be able to explain where it came from and why it is eligible for the target memory scope. Missing provenance means the data must remain a hypothesis, suggestion, observation, or draft.

## Skill Boundary

Skills are versioned capability packages, not authority packages.

Skills must not:

- grant themselves new permissions;
- silently raise risk ceilings;
- bypass MCL, RiskGate, or approval;
- mutate Fact storage without provenance;
- convert model or executor output directly into Facts;
- hide execution from Timeline or audit.

Skill evolution may propose changes, but approval and publication remain governed operations.

## Audit Boundary

The runtime must preserve an inspectable chain for governed work:

- WorkOrder records the mission and governance state.
- WorkerSession records the worker context.
- WorkerRunStep records the ordered work steps.
- WorkerEvent records notable state changes, outputs, approvals, blocks, and errors.
- Timeline / Audit exposes the history for review.
- WorkOrder audit export returns a portable `coevo.audit_export.v1` package with governance fields, worker sessions, runs, steps, events, tool calls, memory evidence, and timeline items.

If work cannot be audited, it should not be treated as governed execution.

## Conversation Boundary

MissionChat conversation threads and messages are durable local context. They may link to generated WorkOrders for user continuity and audit navigation.

Conversation content is not authorization, approval, or policy. Editing or replaying conversation text must not change WorkOrder track, allowed actions, restricted actions, approval receipt validation, or Red Track blocking.

## Credential Boundary

API keys are Alpha-level local configuration. They must not be committed, logged, or displayed in full.

Windows Alpha builds store newly saved non-empty model API keys in the native credential vault and persist only a `keyring:` reference plus masked display value in SQLite. Existing Alpha databases with legacy plaintext values remain readable for compatibility until a migration rewrites them. Non-Windows credential vault support is not available in Alpha; model-provider saves that require a non-empty key will fail with `CREDENTIAL_VAULT_UNAVAILABLE`.

Legacy plaintext support is a read-compatibility path only. New non-empty API-key writes must produce a `keyring:` reference, not plaintext SQLite.

## Workspace Tool Boundary

Green Track file execution is read-only and scoped. The desktop sidecar passes `COEVO_WORKSPACE_DIR` under `COEVO_HOME/workspace`; first install seeds a `welcome.md` file so the user can test governed read-only execution without granting broad filesystem access.

The FileReadonly tool must canonicalize allow/deny paths, reject forbidden secret-like filenames such as `.env`, keys, and tokens, and record tool evidence into Worker audit and Task memory. It must not mutate files.

## Track Classification Boundary

WorkOrder track, allowed actions, restricted actions, and risk summary are server-authoritative at creation time. Current desktop requests send mission facts and selected resources; legacy clients may still send governance fields, but the server ignores them and stores its own classification.

The current Alpha classifier is keyword-based and intentionally over-classifies upward. False positives are safer than under-classifying potentially destructive work, but this is not a hardened production policy engine. Long-term trigger policy should move into versioned governance policy configuration.

## Local-First Boundary

Alpha is local-first:

- desktop launches a local core service sidecar;
- `COEVO_HOME` anchors local runtime state;
- dynamic local ports avoid fixed public service assumptions;
- logs are written locally for debugging and audit.

Alpha is not designed for public network exposure or production multi-user deployment.
