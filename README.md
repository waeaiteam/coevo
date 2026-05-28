# coevo-opc

> Governed AI operating system for a one-person company.

**Freedom in Reason. Governance in Action.**

[English](README.md) | [简体中文](README.zh-CN.md)

---

**Status:** Alpha / Internal RC &nbsp;|&nbsp; **License:** Apache-2.0 &nbsp;|&nbsp; **Runtime:** Rust + Tauri + React &nbsp;|&nbsp; **Model:** Mock / OpenAI-compatible

---

## What is coevo-opc?

coevo-opc is **not** a simple multi-agent chat system.
It is an **OPC OS**: an AI operating system for a one-person company.

It combines governed execution with a full OPC runtime:

- **User Profile** — founder identity, goals, preferences, budgets
- **Company Memory** — scoped, provenance-tracked, long-term memory
- **AI Employees** — governed workers with passports, departments, risk ceilings
- **External Executors** — Hermes / OpenClaw / MCP / 302AI as governed execution workers
- **WorkOrders** — mission execution with tracks, approval, and audit
- **Skills** — versioned, testable, rollback-capable capability packages
- **Skill Evolution** — observe → diagnose → propose → verify → approve → publish
- **Model Gateway** — Mock (zero-config) + OpenAI-compatible (real LLM)
- **Agent Governance Mesh** — MCL, RiskGate, Cognitive Customs, Resolution, ADR-A

**Users are not just prompt senders. They are OPC founders.**
**Agents are not disposable sub-agents. They are governed AI employees with passports.**
**Executors are not free agents. Every action is governed by MCL, RiskGate, and ADR-A.**
**Models provide cognition, not authorization.**

---

## Why coevo is different

| Layer | Ordinary Agent App | coevo-opc |
|---|---|---|
| User | Prompt sender | OPC Founder with profile & goals |
| Agent | Prompt role | AI Employee with Passport, department, risk ceiling |
| Tool | Direct function call | Governed Executor (registered, risk-checked) |
| Memory | Chat history | Scoped long-term Memory (provenance, TTL, cognitive layer) |
| Risk | Prompt-based refusal | RiskGate with Green/Yellow/Red tracks |
| Skill | Prompt template | Versioned, testable SkillPackage with verifier |
| Evolution | Manual prompt tweak | Observe → Diagnose → Propose → Verify → Approve |
| Output | Answer text | WorkOrder + Memory + Proposal + Audit |

---

## Current Alpha Capabilities

- ✅ MissionChat — natural language mission entry point
- ✅ Founder Profile — save & load user identity
- ✅ Company Memory — create, search, stale, revoke with scope filtering
- ✅ AI Employees — 10 seed employees with passports and departments
- ✅ External Executors — register, disable, health check, dry-run (mock adapters)
- ✅ WorkOrders — create, execute, cancel, feedback
- ✅ Green / Yellow / Red tracks with differentiated behavior
- ✅ Skill packages — seed, list, activate, rollback
- ✅ Skill Evolution — failure → proposal → verify → approve → reject → rollback
- ✅ Model Gateway — Mock provider (always available) + OpenAI-compatible
- ✅ Desktop console — Tauri + React
- ✅ Swagger / Redoc API docs
- ✅ Synthetic OPC user test

---

## Architecture Overview

```
apps/server          — axum HTTP API (:8717) + OpenAPI/Swagger/Redoc
apps/desktop         — Tauri + React desktop control console

crates/coevo-core       — Protocol types, metadata, OPC data model, skills model
crates/coevo-store      — SQLite + sqlx migrations + repositories
crates/coevo-mcl        — Mission Contract Language compiler + state machine
crates/coevo-router     — PCDT routing + plan revision
crates/coevo-customs    — Cognitive Customs + Provenance + Dependency Graph
crates/coevo-risk       — Risk Gate + Emergency Lease Manager
crates/coevo-resolution — Resolution Engine + ADR-A
crates/coevo-reputation — Reputation v1 Profile
crates/coevo-tracks     — Green / Yellow / Red three-track runtime
crates/coevo-evolution  — Skill evolution loop (analyzer, generator, verifier, scheduler)
crates/coevo-executors  — External Executor adapters (Hermes, OpenClaw, MCP, etc.)
crates/coevo-models     — Model Gateway (Mock + OpenAI-compatible)
crates/coevo-policy     — Pluggable PolicyEngine trait + Mock
crates/coevo-adapters   — Mock A2A / MCP / Identity adapters
crates/coevo-audit      — Structured audit logger
crates/coevo-cli        — CLI tool for local operations
tests/e2e               — Acceptance test suite
```

---

## Runtime Flow

```
User Intent
  → Model-enhanced Mission Draft (fallback to deterministic)
  → MCL Compile
  → PCDT Route
  → AI Employees selected from Registry
  → External Executors selected
  → WorkOrder created
  → Risk track selected (Green / Yellow / Red)
  → Executor dry-run
  → Execute (Green auto, Yellow waiting, Red blocked)
  → Task Memory written
  → Synthesizer summary (model or fallback)
  → Feedback → Skill Evolution Proposal
```

---

## Three Governance Tracks

| | Green | Yellow | Red |
|---|---|---|---|
| Risk | Low | Moderate | High |
| Action | read, analyze, local safe | internal notification, low-impact write | production write, financial, destructive |
| Execution | Auto | WaitingApproval / negative consent | Blocked by default |
| Approval | None | YES (NEGATIVE_CONSENT or EXPLICIT) | YES (identity, dual-sign, lease) |
| Alpha | ✅ Fully supported | ✅ WaitingApproval supported | ✅ Blocked with clear reason |

**Red Track in Alpha:** identity proof, monitoring signature, diagnostic signature, and lease are required but not production-grade MFA yet.

---

## Model Gateway

| Provider | Status | API Key | Use |
|---|---|---|---|
| **Mock** | ✅ Built-in | Not required | Development, CI, synthetic tests |
| **OpenAI-compatible** | ✅ Supported | Required | Real LLM testing |
| OpenAI | Compatible (mapped) | Required | Through OpenAI-compatible |
| Anthropic | Planned | — | — |
| Gemini | Planned | — | — |
| DeepSeek | Compatible (mapped) | Required | Through OpenAI-compatible |
| Ollama | Planned | — | — |

**Mock Provider** returns deterministic MissionDraft, Synthesizer, and SkillGenerator output. No API key needed. Always available.

---

## Quick Start

### Prerequisites
- Rust 1.85+
- Node.js 20+
- SQLite 3
- Python 3 (for synthetic test)

### Server

```bash
cargo check --workspace
cargo run -p coevo-server
# → http://127.0.0.1:8717
# API docs: http://127.0.0.1:8717/docs
```

### Desktop

```bash
cd apps/desktop
npm install
npm run dev          # Web at http://localhost:5173
npm run tauri dev    # Tauri native window
```

### Tests

```bash
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture

cd apps/desktop
npm run build
npm test
npm run test:synthetic-opc    # requires server running
```

---

## First OPC Run

1. Start the server: `cargo run -p coevo-server`
2. Start desktop: `cd apps/desktop && npm run dev`
3. Open **Settings → Model Providers**
4. Select **Mock Provider** (no API key needed)
5. Click **Test Connection** → should pass
6. Open **Founder Profile** → Save your profile
7. Open **AI Employees** → Click **Seed 10 AI Employees**
8. Open **External Executors** → Click **Register** (OpenClaw, risk 0.6)
9. Open **Skills** → Click **Seed Skills**
10. Go to **MissionChat**
11. Enter: `Summarize current coevo-opc progress and propose next roadmap.`
12. Review the Mission Draft → Click **Execute Green**
13. Check **WorkOrders** → see the completed order
14. Check **Company Memory** → see Task Memory written
15. Submit feedback → Check **Skills → Evolution Proposals**

---

## Real Model Test

For LLM testing with an actual API key, see [MANUAL_MODEL_TEST.md](MANUAL_MODEL_TEST.md).

- Choose **OpenAI-compatible** in Settings → Model Providers
- Set base_url, api_key, model
- Click **Test Connection**
- ⚠️ **Do not commit API keys.** Keys are Alpha-level runtime config.

---

## API Overview

### Core
`GET /health` `GET /docs` `GET /redoc`

### MCL / Routing
`POST /mcl/compile` `POST /router/route`

### OPC
`GET/PUT /opc/profile/user` `GET/PUT /opc/profile/company`
`GET/POST /opc/memory` `POST /opc/memory/:id/stale` `POST /opc/memory/:id/revoke`
`GET /opc/agents/employees` `POST /opc/agents/employees/seed`
`GET /opc/executors` `POST /opc/executors/register` `POST /opc/executors/:id/disable`
`GET/POST /opc/work-orders` `POST /opc/work-orders/:id/execute`
`GET /opc/skills` `POST /opc/skills/seed`
`GET /opc/skills/evolution/proposals` `POST /opc/skills/evolution/run`

### Models
`GET/PUT /opc/models/config` `POST /opc/models/test` `POST /opc/models/chat` `POST /opc/models/structured`

---

## Security Model

- Model output is **not authorization** — RiskGate and MCL always have the final say
- Fact writes require **provenance** (Cognitive Customs)
- Red Track is **blocked by default** — credentials required
- External Executor output defaults to **Hypothesis / Suggestion**
- API keys are **Alpha-level config** — not a production vault yet
- Alpha is **local-first** — not designed for public network exposure

---

## Current Limitations

- ⚠️ **Alpha / Internal RC only** — not production ready
- ⚠️ Real Hermes / OpenClaw / 302AI execution not implemented (mock adapters only)
- ⚠️ External executors are governed mock stubs with real API contracts
- ⚠️ Model config is Alpha-level (in-memory, not persisted across server restarts)
- ⚠️ Credential Vault not implemented
- ⚠️ Vector memory not implemented
- ⚠️ Production MFA / lease enforcement not complete
- ⚠️ CI badges may not exist yet
- ⚠️ UI is still evolving

---

## Roadmap

| Milestone | Target |
|---|---|
| v0.2 Alpha | OPC runtime, Model Gateway, MissionChat, Mock executors ✅ |
| v0.3 Private Beta Candidate | Persistent model config, credential vault, real GitHub executor, real MCP tool runtime, vector memory, desktop onboarding |
| v0.4 | Real OpenClaw / Hermes adapter, 302AI capability catalog, plugin marketplace, audit viewer, packaging installers |
| v1.0 | Production Policy Engine, real lease/MFA, sandbox hardening, team/multi-user control plane |

---

## Contributing

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

- Do not commit API keys or secrets
- Do not commit `.env` files
- Add new crates to workspace `Cargo.toml` members
- Add new adapters implementing `ExternalExecutorAdapter` trait
- Add translations via `README.zh-CN.md` pattern

---

## License

Apache-2.0
