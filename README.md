# coevo — Agent Governance Mesh

**内部推理自由，外部行为受治**

coevo is a control-plane specification and runtime for multi-agent systems, sitting atop the A2A and MCP protocols. It provides a deterministic governance mesh that manages the cognition, permissions, state, and responsibility of probabilistic LLM agents.

## Architecture

```
apps/server     ── axum HTTP API (:8717) + OpenAPI/Swagger/Redoc
apps/desktop    ── Tauri + React desktop control console
crates/coevo-cli ── CLI tool for local operations

crates/coevo-tracks    ── Green/Yellow/Red three-track runtime
crates/coevo-mcl       ── Mission Contract Language compiler + state machine
crates/coevo-router    ── PCDT routing + plan revision
crates/coevo-customs   ── Cognitive Customs + Blackboard + Dependency Graph
crates/coevo-risk      ── Risk Gate + Emergency Lease Manager
crates/coevo-resolution── Resolution Engine + ADR-A
crates/coevo-reputation── Reputation v1 Profile
crates/coevo-audit     ── Structured audit logger
crates/coevo-policy    ── Pluggable PolicyEngine trait + Mock
crates/coevo-adapters  ── Mock A2A/MCP/Identity adapters
crates/coevo-store     ── SQLite + sqlx + 12 migrations
crates/coevo-core      ── Protocol types + Common Metadata Header
```

## Quick Start

### Prerequisites
- Rust 1.85+
- Node.js 20+
- SQLite 3

### Run the server

```bash
make dev
# Server starts at http://127.0.0.1:8717
# API docs at http://127.0.0.1:8717/docs
```

### Run a demo

```bash
# Green Track — fast, low-risk (read/analyze)
curl -X POST http://127.0.0.1:8717/demo/green \
  -H "Content-Type: application/json" \
  -H "x-coevo-tenant-id: demo" \
  -H "x-coevo-actor-role: CLI" \
  -H "x-coevo-contract-hash: 0000000000000000000000000000000000000000000000000000000000000000" \
  -H "x-coevo-policy-version: 0000000000000000000000000000000000000000000000000000000000000000" \
  -H "x-coevo-execution-plan-hash: 0000000000000000000000000000000000000000000000000000000000000000" \
  -d '{}'
```

### Docker

```bash
docker compose up -d
```

### Desktop app

```bash
cd apps/desktop
npm install
npm run dev
# Opens at http://localhost:5173
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/openapi.json` | OpenAPI spec |
| GET | `/docs` | Swagger UI |
| GET | `/redoc` | Redoc |
| POST | `/mcl/compile` | Compile user intent → MCL contract |
| POST | `/router/route` | Compute execution plan |
| POST | `/customs/propose` | Propose blackboard state change |
| POST | `/risk/evaluate` | Evaluate action risk |
| POST | `/resolution/process` | Process conflict resolution |
| POST | `/demo/green` | Run Green Track demo |
| POST | `/demo/yellow` | Run Yellow Track demo |
| POST | `/demo/red` | Run Red Track demo |

## Common Metadata Header

Every API request must carry these headers:

| Header | Description |
|--------|-------------|
| `x-coevo-idempotency-key` | UUIDv4 idempotency key |
| `traceparent` | W3C Trace Context |
| `x-coevo-contract-hash` | SHA256 of active MCL contract |
| `x-coevo-policy-version` | SHA256 of institution policy |
| `x-coevo-tenant-id` | UUIDv4 tenant identifier |
| `x-coevo-execution-plan-hash` | SHA256 of execution plan |
| `x-coevo-actor-role` | Proposer/Critic/Synthesizer |
| `x-coevo-causality-parent-id` | UUIDv4 parent event |
| `x-coevo-request-ttl-ms` | Request TTL in ms |
| `x-coevo-timestamp` | Unix timestamp ms |
| `x-coevo-replay-mode` | Dry-run flag (true/false) |
| `x-coevo-caller-identity-proof` | Ed25519 signature (Red Track required) |
| `tracestate` | Optional system context (PII-safe) |

## Three Tracks

| Track | BR | IR | Approval | Lease | ADR-A |
|-------|----|----|----------|-------|-------|
| Green | 0 | 0 | None | No | Lightweight trace |
| Yellow | ≤1 | 1 | NEGATIVE_CONSENT / EXPLICIT_APPROVAL | No | Simplified |
| Red | ≥2 | 3 | MFA + dual-sign | 15-min | Full |

## Testing

```bash
# Unit tests
cargo test --workspace

# E2E acceptance tests (requires server on :8717)
cargo test --test acceptance
```

## License

Apache-2.0
