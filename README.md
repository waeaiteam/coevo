# coevo

coevo is a local-first control plane for running a governed AI company on one machine. The current product is built around a Rust HTTP server, a Tauri 2 desktop console, SQLite-backed state, company-scoped workspaces, and a worker runtime that executes WorkOrders under server-owned policy.

## What runs today

- Company-scoped workspaces with local files for employees, memory, shared files, reports, meetings, work orders, and governance artifacts
- Server-authoritative Green / Yellow / Red track classification and approval enforcement
- MCL compilation and route planning anchors
- Live worker event streaming over SSE, with persisted runs, steps, tool calls, traces, and audit exports
- Model provider support for OpenAI-compatible providers and Anthropic, with DeepSeek through the OpenAI-compatible path
- A Mock provider for development and CI only
- MCP server registrations over `stdio` and streamable HTTP, with cached tool lists and governed use
- External executor registry, health check, and dry-run surfaces are wired; source types include local process, HTTP runtime, Docker, and MCP-backed adapters
- Inline approval flows in the desktop UI for approval-gated work orders
- Canonical company-scoped HTTP routes under `/companies/{opc_id}/...`, with legacy `/opc/...` routes retained for compatibility where needed

## Runtime spine

The main execution path is:

```text
MissionChat or API intent
  -> /mcl/compile
  -> /router/route
  -> create WorkOrder
  -> execute WorkOrder
  -> WorkerHarness
  -> AgentSubHarness governed loop
  -> model / tool / executor calls
  -> worker event + audit persistence
  -> SSE stream back to the desktop UI
```

MissionChat conversations are durable local threads. They can spawn WorkOrders, and the resulting run stays linked to the original conversation, timeline, and audit trail.

## Governance and approvals

coevo treats model output as cognition, not authorization. The server decides the work track, allowed actions, restricted actions, and risk summary when a WorkOrder is created.

- Green runs inside the governed runtime when policy allows it.
- Yellow creates a persisted approval request and pauses until an approval receipt is present. The approval card can be handled inline in MissionChat or from the Timeline flow.
- Red is blocked at runtime entry with an explicit reason.

Approval records are persisted. A resumed run uses the approval receipt, not mutable prompt text, as the authority signal.

## Model and MCP support

Current model support is intentionally narrow:

- OpenAI-compatible providers
- Anthropic
- DeepSeek through the OpenAI-compatible path
- Mock provider for development and CI only

Model API keys are stored in the native credential vault on Windows and macOS. SQLite keeps a masked display value plus a `keyring:` reference. Existing legacy plaintext rows can still be read for compatibility, but new non-empty writes are vault-backed. Non-Windows vault writes for non-empty keys are unavailable.

MCP support is real, but it is a governed integration surface rather than a free-form plugin marketplace. Enabled servers are persisted, can be connected and tested, and expose cached tool lists to the worker runtime.

## Storage and workspace layout

`COEVO_HOME` is the local root for runtime state. If it is unset, the server defaults to `~/.coevo`.

At the top level you will usually see:

- `data/coevo.db` for the global SQLite database
- `logs/` for server and desktop logs
- `runtime/` for launch files such as `server.port` and `server.pid`
- `workspace/` for company-scoped workspaces
- `companies.json` as the company index

Each company lives under `workspace/{opc_id}` and includes:

- `company.json` and `charter.md`
- `employees/`
- `memory/`
- `shared/`
- `reports/`
- `meetings/`
- `.workorders/planned`, `.workorders/running`, `.workorders/waiting`, `.workorders/completed`, `.workorders/failed`
- `.governance/.mcl`, `.governance/.pcdt`, `.governance/.risk`, `.governance/.tracks/{green,yellow,red}`, `.governance/.resolution`, `.governance/.audit`
- `skills/`

Employee state is file-backed. The core files are `passport.json`, `prompt.md`, `prompt_versions/`, `identity.md`, `soul.md`, and `agents.md`, with additional `owner.md`, `tools.md`, and `tool_policy.json` files also present in the current workspace manager.

## Desktop and server layout

The backend can run on its own:

```bash
cargo run -p coevo-server
```

Default server address:

- `http://127.0.0.1:8717`
- OpenAPI docs: `/docs`
- ReDoc: `/redoc`

The desktop shell launches the local server sidecar automatically and talks to it over HTTP. The sidecar uses a dynamic local port, writes logs to `COEVO_HOME/logs`, and seeds runtime files under `COEVO_HOME/runtime`.

`apps/desktop/src-tauri` is intentionally excluded from the root Cargo workspace and is built separately by the desktop wrapper scripts.

## Repository layout

```text
apps/server                Axum HTTP server and API surface
apps/desktop               Tauri + React desktop console
apps/desktop/src-tauri     Desktop shell and sidecar packaging
crates/coevo-core          Shared domain types
crates/coevo-store         SQLite repos, migrations, workspace manager
crates/coevo-policy        Policy helpers and governance primitives
crates/coevo-adapters      MCP client and adapter layer
crates/coevo-audit         Structured audit logging
crates/coevo-mcl           Mission contract language and compilation
crates/coevo-router        Route planning
crates/coevo-customs       Provenance and governed fact flow
crates/coevo-risk          Risk decisions and approval boundaries
crates/coevo-resolution    Resolution and escalation paths
crates/coevo-reputation    Reputation and attribution
crates/coevo-tracks        Track-specific runtime logic
crates/coevo-cli           Small CLI for compile / route flows
crates/coevo-evolution     Improvement and self-upgrade generation
crates/coevo-executors     Local process / HTTP / Docker / MCP-backed executor adapters
crates/coevo-models        Model gateways, routing, and pricing
crates/coevo-worker        Governed worker runtime and SSE event production
tests/e2e                  Acceptance coverage
```

## Setup

```bash
git clone git@github.com:waeaiteam/coevo.git
cd coevo
cd apps/desktop
npm install
```

Desktop commands must go through the npm wrapper scripts. Do not call `vite`, `tsc`, `vitest`, or `tauri` directly.

## Run

Run only the backend:

```bash
cargo run -p coevo-server
```

Run migrations and exit:

```bash
cargo run -p coevo-server -- --migrate
```

Run the desktop web surface against an already running server:

```bash
cd apps/desktop
npm run dev
```

Run the full desktop shell with the local sidecar:

```bash
cd apps/desktop
npm run tauri dev
```

Build the desktop app:

```bash
cd apps/desktop
npm run build
npm run build:tauri
```

## Configuration

Important environment variables:

- `COEVO_HOME`: workspace root
- `COEVO_BIND_ADDR`: full server bind address
- `COEVO_PORT`: server port when `COEVO_BIND_ADDR` is not set
- `COEVO_DATABASE_URL`: SQLite URL or path
- `COEVO_DB_PATH`: raw SQLite path
- `COEVO_BUILD_ARTIFACT_DIR`: optional desktop artifact root
- `RUST_LOG`: Rust log filter

The desktop sidecar also sets `COEVO_HOME`, `COEVO_PORT`, `COEVO_DB_PATH`, `COEVO_WORKSPACE_DIR`, `COEVO_PARENT_HEARTBEAT`, `COEVO_AUTH_TOKEN`, and `COEVO_LOG_DIR` when it launches the server.

## Verify

Backend:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture
```

Desktop:

```bash
cd apps/desktop
npm test
npm run build
```

Optional synthetic integration test:

```bash
cd apps/desktop
npm run test:synthetic-opc
```

## License

Apache-2.0
