# coevo-opc

> AI Employee Operating System and Agent Governance Mesh for a one-person company.

**Freedom in Reason. Governance in Action.**

[English](README.md) | [简体中文](README.zh-CN.md)

---

**Status:** Alpha / Internal RC &nbsp;|&nbsp; **License:** Apache-2.0 &nbsp;|&nbsp; **Runtime:** Rust + Tauri + React &nbsp;|&nbsp; **Model:** OpenAI-compatible

---

## What is coevo-opc?

coevo-opc is **not** a chatbot, a prompt wrapper, or a multi-agent demo.
It is an **AI Employee Operating System** for an OPC (One-Person Company): a local-first desktop runtime where AI employees can reason freely, but every external action is governed.

The product shape is an **Agent Governance Mesh**:

- **MissionChat** - the desktop entry point for real user missions
- **WorkOrders** - governed execution records with tracks, approvals, events, and audit
- **AI Employees** - workers with passports, departments, capabilities, and risk ceilings
- **Company Memory** - scoped long-term memory with provenance and lifecycle controls
- **Model Gateway** - real OpenAI-compatible model configuration through the desktop
- **External Executors** - governed execution workers, not free agents
- **Skills** - versioned, testable, rollback-capable capability packages
- **Timeline / Audit** - inspectable WorkerSession, WorkerRunStep, WorkerEvent, and WorkOrder history
- **Governance Mesh** - MCL, RiskGate, Cognitive Customs, Resolution, and ADR-A

**Users are not prompt senders. They are OPC founders.**
**AI workers are not disposable sub-agents. They are governed AI employees.**
**Executors are not autonomous agents. Every action is checked by MCL, RiskGate, and ADR-A.**
**Models provide cognition, not authorization.**

---

## Why coevo is different

| Layer | Ordinary Agent App | coevo-opc |
|---|---|---|
| User | Prompt sender | OPC Founder with profile, goals, and budget context |
| Agent | Prompt role | AI Employee with passport, department, and risk ceiling |
| Tool | Direct function call | Governed Executor with registration, scope, and risk checks |
| Memory | Chat history | Scoped long-term Memory with provenance, TTL, and audit |
| Risk | Prompt-based refusal | RiskGate with Green / Yellow / Red tracks |
| Skill | Prompt template | Versioned SkillPackage with tests and verifier |
| Evolution | Manual prompt tweak | Observe -> Diagnose -> Propose -> Verify -> Approve |
| Output | Answer text | WorkOrder + Timeline + Memory + Proposal + Audit |

---

## Desktop User Path

The ordinary user path is desktop-first:

1. Install or unzip the desktop Alpha build.
2. Double-click the coevo desktop app.
3. The app starts the local core service as a sidecar automatically.
4. coevo prepares `COEVO_HOME`, uses a dynamic local port, and writes runtime logs under the local log directory.
5. FirstRun creates a local OPC identity: OPC name, owner name, and language.
6. The app opens **Settings -> Model Provider** for a real provider/API key connection.
7. After **Test / Discover Models** succeeds, the app fills the model role selectors and MissionChat opens.

Alpha is local-first. The desktop app and local core service are intended for a user's own machine, not public network exposure.

---

## First Mission Path

1. Create your OPC during FirstRun.
2. In **Settings -> Model Provider**, select OpenAI, DeepSeek, or another OpenAI-compatible provider.
3. Paste the API key. Provider presets fill the base URL; custom transport settings live under **Advanced**.
4. Click **Test / Discover Models**. Discovered model IDs populate the default, fast, reasoning, and structured-output selectors.
5. Go to **New Chat / MissionChat** and describe the mission you want an AI employee to handle.
6. Review the model cognition summary, generated WorkOrder, and governance track.
7. Execute allowed Green work, approve Yellow work when appropriate, and expect Red work to be blocked in Alpha.
8. Open **WorkOrders** to inspect the created order.
9. Open **Audit** or the WorkOrder timeline to inspect worker sessions, steps, events, and governance decisions.

The expected product loop is:

```
Desktop Launch
  -> Create OPC
  -> Connect real model provider
  -> MissionChat
  -> MCL compile
  -> PCDT route
  -> AI Employee selection
  -> WorkOrder
  -> RiskGate track decision
  -> WorkerSession / WorkerRunStep / WorkerEvent
  -> Timeline / Audit
```

Primary navigation is intentionally small: **New Chat**, **OPC**, **WorkOrders**, **Audit**, and **Settings**. The original specialist surfaces remain available under **OPC -> Advanced Console** and **Settings -> Advanced**.

---

## Current Alpha Capabilities

- MissionChat as the natural language mission entry point
- Desktop launch with local core sidecar, `COEVO_HOME`, dynamic port, and logs
- Founder Profile and Company Profile
- Company Memory create, search, stale, revoke, and scope filtering
- AI Employees with passports, departments, capabilities, and risk ceilings
- External Executor registry, disable, health check, and dry-run contracts
- WorkOrders with create, execute, cancel, feedback, timeline, and audit export
- Green / Yellow / Red tracks with differentiated governance behavior
- Server-authoritative track classification at WorkOrder creation
- Red Track Alpha hard block with explicit reason
- Scoped `FileReadonlyTool` execution for Green read/analyze work under the local workspace
- Skill packages with seed/list/activate/rollback support
- Skill Evolution proposal flow for governed improvement
- OpenAI-compatible model provider configuration
- Swagger / Redoc API docs for developers
- Synthetic OPC tests for development and CI

---

## Governance Tracks

| | Green | Yellow | Red |
|---|---|---|---|
| Risk | Low | Moderate | High |
| Action | read, analyze, local safe work | low-impact write or internal notification | production write, financial, destructive, irreversible |
| Execution | Auto when policy allows | Approval request and approved receipt required before execution | Blocked by default in Alpha |
| Authorization | MCL + RiskGate | MCL / RiskGate boundary + Alpha approval receipt validation | Requires stronger identity, dual control, and lease model |
| Alpha behavior | Supported for scoped read/analyze work | WaitingApproval creates an approval request; arbitrary strings are rejected | Hard blocked with clear reason |

Red Track in Alpha is intentionally conservative: high-risk external behavior is blocked rather than partially automated.

---

## Security Model

The short version: **internal reasoning is free; external behavior is governed**.

- Model output is never authorization. MCL, RiskGate, policy, and user approval decide what can happen.
- External Executors are not free agents. They operate through registered contracts and governed WorkOrders.
- Fact writes require provenance. Hypotheses, suggestions, model output, and executor output cannot become Facts without the Cognitive Customs path.
- Skills cannot silently increase their own authority or bypass risk ceilings.
- WorkOrders, WorkerSessions, WorkerRunSteps, WorkerEvents, Timeline entries, and audit records must remain inspectable.
- On Windows Alpha builds, newly saved model API keys are stored through the native credential vault and the SQLite row stores only a `keyring:` reference. Existing Alpha databases with legacy plaintext keys remain readable for compatibility until a migration is added. Non-Windows credential storage is not available in Alpha.

See [docs/SECURITY_BOUNDARIES.md](docs/SECURITY_BOUNDARIES.md) for the full boundary model.

---

## Model Gateway

Normal desktop onboarding requires a real OpenAI-compatible provider.

| Provider | Status | API Key | User Path |
|---|---|---|---|
| **OpenAI-compatible** | Supported | Required | FirstRun and real missions |
| OpenAI | Compatible through OpenAI-compatible settings | Required | Real missions |
| DeepSeek and similar compatible APIs | Compatible when API shape matches | Required | Real missions |
| Anthropic | Planned | - | Not a FirstRun target |
| Gemini | Planned | - | Not a FirstRun target |
| Ollama | Planned | - | Not a FirstRun target |

Model configuration is no longer treated as pure process memory. In Windows Alpha builds, new API-key writes go to the native credential vault and SQLite stores a `keyring:` reference plus a masked display value. Existing Alpha databases with legacy plaintext keys are still readable for compatibility; opportunistic re-encryption is pending. Non-Windows credential vault support is not yet available.

---

## Developer Setup

These commands are for contributors and internal testing. They are not the ordinary desktop Quick Start.

### Prerequisites

- Rust 1.85+
- Node.js 20+
- SQLite 3
- Python 3 for synthetic tests

### Server

```bash
cargo check --workspace
cargo run -p coevo-server
# http://127.0.0.1:8717
# API docs: http://127.0.0.1:8717/docs
```

### Desktop Development

```bash
cd apps/desktop
npm install
npm run dev
npm run tauri dev
```

### Tests

```bash
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture

cd apps/desktop
npm run build
npm test
npm run test:synthetic-opc
```

---

## Developer / CI Synthetic Tests

Mock provider, seed data, and demo-like fixtures are development infrastructure only.
They exist so CI, deterministic tests, and local contributor workflows can run without paid model credentials.

- Mock Provider returns deterministic MissionDraft, Synthesizer, and SkillGenerator output.
- Seed AI Employees and seed Skills are test/dev bootstrap tools.
- Synthetic OPC tests use Mock by design.
- Mock is not the ordinary user onboarding path and should not be presented as the product Quick Start.

For manual real-model testing, see [MANUAL_MODEL_TEST.md](MANUAL_MODEL_TEST.md).

---

## Architecture Overview

```
apps/server          - axum HTTP API + OpenAPI/Swagger/Redoc
apps/desktop         - Tauri + React desktop console and sidecar launch

crates/coevo-core       - protocol types, metadata, OPC data model, skills model
crates/coevo-store      - SQLite + sqlx migrations + repositories
crates/coevo-mcl        - Mission Contract Language compiler + state machine
crates/coevo-router     - PCDT routing + plan revision
crates/coevo-customs    - Cognitive Customs + Provenance + Dependency Graph
crates/coevo-risk       - RiskGate + Emergency Lease Manager
crates/coevo-resolution - Resolution Engine + ADR-A
crates/coevo-reputation - Reputation v1 Profile
crates/coevo-tracks     - Green / Yellow / Red runtime
crates/coevo-evolution  - Skill evolution loop
crates/coevo-executors  - External Executor adapters
crates/coevo-models     - Model Gateway
crates/coevo-policy     - PolicyEngine trait
crates/coevo-adapters   - A2A / MCP / Identity adapters for tests and integration
crates/coevo-audit      - structured audit logger
crates/coevo-cli        - local developer operations
tests/e2e               - acceptance test suite
```

---

## API Overview

### Core

`GET /health` `GET /docs` `GET /redoc`

### MCL / Routing

`POST /mcl/compile` `POST /router/route`

`/mcl/compile` persists a contract anchor in SQLite and returns `contract_hash`. `/router/route` requires that `contract_hash` and persists the resulting execution plan anchor.

### OPC

`GET/PUT /opc/profile/user` `GET/PUT /opc/profile/company`
`GET/POST /opc/memory` `POST /opc/memory/:id/stale` `POST /opc/memory/:id/revoke`
`GET /opc/agents/employees` `POST /opc/agents/employees/seed`
`GET /opc/executors` `POST /opc/executors/register` `POST /opc/executors/:id/disable`
`GET/POST /opc/work-orders` `POST /opc/work-orders/:id/execute`
`GET /opc/work-orders/:id/timeline` `GET /opc/work-orders/:id/audit-export`
`GET /opc/skills` `POST /opc/skills/seed`
`GET /opc/skills/evolution/proposals` `POST /opc/skills/evolution/run`

WorkOrder governance fields are server-authoritative at create time. Current desktop requests send mission facts and selected resources; legacy clients may still send `track`, `allowed_actions`, `restricted_actions`, or `risk_summary`, but the server ignores them and stores its own classification.

### Models

`GET/PUT /opc/models/config` `POST /opc/models/test` `POST /opc/models/chat` `POST /opc/models/structured`

---

## Current Alpha Limitations

- Alpha / Internal RC only; not production ready.
- Credential vault is Windows-first in Alpha: new model API-key writes use native keyring references, while legacy plaintext rows remain readable until a migration rewrites them.
- Real executor MVP is the next stage; current external executor behavior is governed dry-run / mock-adapter focused.
- Red Track Alpha behavior is hard block, not production-grade lease/MFA execution.
- Yellow approval has persisted approval requests and receipt validation, but the full user-facing approval management UI is still evolving.
- Server-side mission track classification is keyword-based and intentionally over-classifies upward in Alpha.
- Vector memory is not implemented.
- Production MFA, lease enforcement, sandbox hardening, and public-network deployment are not complete.
- UI and audit viewer surfaces are still evolving.

---

## Roadmap

| Milestone | Target |
|---|---|
| v0.2 Alpha | OPC runtime, desktop MissionChat, Model Gateway, governance tracks |
| v0.3 Private Beta Candidate | Credential migration / cross-platform vault, real executor MVP, MCP tool runtime, vector memory, stronger audit viewer |
| v0.4 | OpenClaw / Hermes adapters, 302AI capability catalog, plugin marketplace, packaged installers |
| v1.0 | Production Policy Engine, real lease/MFA, sandbox hardening, team/multi-user control plane |

---

## Contributing

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

- Do not commit API keys or secrets.
- Do not commit `.env` files.
- Keep Mock, seed, and demo fixtures in developer / CI documentation only.
- Add new crates to workspace `Cargo.toml` members.
- Add new adapters implementing `ExternalExecutorAdapter` trait.

---

## License

Apache-2.0
