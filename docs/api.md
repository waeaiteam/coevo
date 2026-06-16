# coevo API Documentation

Base URL: `http://127.0.0.1:8717`

Interactive docs: `http://127.0.0.1:8717/docs` (Swagger) or `/redoc`

---

## 1. Health Check

```
GET /health
```

**Response 200:**
```json
{"status": "ok", "version": "1.0.0"}
```

---

## 2. Compile Contract

```
POST /mcl/compile
```

Compiles user intent into an MCL contract. Per whitepaper Section 2.1.
The handler persists the returned contract as a contract anchor. Use the returned
`contract_hash` when routing the execution plan.

**Request:**
```json
{
  "user_intent": "Read and analyze system health metrics in development",
  "requested_mode": "DRAFT",
  "parent_contract_hash": null
}
```

`requested_mode`: `DRAFT` (dry-run only) or `ACTIVE` (policy enforcement).

**Response 200:**
```json
{
  "contract": { /* MCLSpec */ },
  "contract_hash": "abc123...",
  "ambiguity_score": 0.25,
  "compile_warnings": []
}
```

**Error 403** (`MCL_INSTITUTION_VIOLATION`): Contract violates institution policy.
**Error 422** (`MCL_COMPILATION_ERROR`): Compilation or ambiguity failure.

---

## 3. Route Execution Plan

```
POST /router/route
```

Computes PCDT execution plan. Per whitepaper Section 7.

**Request:**
```json
{
  "contract_hash": "abc123...",
  "contract": { /* MCLSpec from /mcl/compile */ },
  "agent_ids": ["agent-synthesizer-01", "agent-critic-01"]
}
```

`contract_hash` is required and must refer to a persisted contract returned by
`/mcl/compile`. Route plans are persisted as execution plan anchors under this
contract.

**Response 200:**
```json
{
  "plan": { /* ExecutionPlanSpec */ },
  "plan_hash": "def456..."
}
```

**Error 422** (`ROUTING_NO_PATH`): No compliant routing path found.
**Error 422** (`BUDGET_EXCEEDED`): Token budget exceeded.
**Error 422** (`MCL_COMPILATION_ERROR`): Missing or unknown `contract_hash`.

---

## 4. Propose Blackboard Change

```
POST /customs/propose
```

Propose state change with optimistic concurrency. Per whitepaper Section 8.

**Request:**
```json
{
  "target_key": "my-key",
  "expected_version": 1,
  "proposed_value": {"data": "example"},
  "cognitive_layer": "Hypothesis",
  "provenance_envelope": {
    "source_agent_id": "agent-1",
    "verification_tool_urn": "urn:mcp:tool:unit-test-runner",
    "environmental_scope": {"environment": "development", "tenant_id": "t1"},
    "ttl_seconds": 3600,
    "cryptographic_signature": "sig",
    "verification_report": {"passed": true},
    "created_at": "2025-01-01T00:00:00Z"
  },
  "dependency_entry_ids": []
}
```

**Cognitive Layers:** `Hypothesis`, `Fact`, `Suggestion`, `Decision`

**Response 200:**
```json
{
  "receipt": {
    "commit_index": 1,
    "new_version": 1,
    "key": "my-key",
    "committed_at_ms": 1704067200000
  }
}
```

**Error 403** (`COGNITIVE_BOUND_VIOLATION`): Direct Fact write without MCP provenance.
**Error 409** (`COGNITIVE_WRITE_CONFLICT`): Concurrent write conflict.
**Error 412** (`VERSION_MISMATCH`): Optimistic lock version mismatch.
**Error 428** (`VERSION_REQUIRED`): Missing expected_version.

---

## 5. Evaluate Risk

```
POST /risk/evaluate
```

Intercepts action before physical execution. Per whitepaper Section 9.

**Request:**
```json
{
  "action_urn": "urn:coevo:action:write:deploy",
  "target_environment": "production",
  "parameters": {},
  "emergency_mode": false,
  "blast_radius": 3,
  "irreversibility": 3,
  "environment_sensitivity": 3,
  "reversibility": 3
}
```

**Response 200** (decision: ALLOW/DENY/REQUIRE_HUMAN_APPROVAL/ALLOW_WITH_LEASE/etc.)

---

## 6. Resolve Conflict

```
POST /resolution/process
```

Processes stance matrix, generates ADR-A. Per whitepaper Section 10.

**Response 200:** Resolution decision with ADR-A (must contain `rejected_alternatives` and `responsibility_anchor`).

**Error 422** (`DEADLOCK_DETECTED`): Irreconcilable deadlock; requires human arbitration.

---

## 7. Internal Contributor Test Routes

The legacy `/demo/*` routes are no longer mounted in the product server router
and are not advertised in the public OpenAPI document. Product validation should
use MissionChat, WorkOrders, Timeline, and the synthetic / acceptance test
suites.

---

## 8. WorkOrder Create Boundary

`POST /opc/work-orders` accepts mission facts and selected resources. Track, allowed actions, restricted actions, and risk summary are server-authoritative at creation time. Legacy clients may still send those governance fields, but the server ignores them and persists its own RiskGate classification.

---

## 9. OPC Company Scope

Coevo now exposes two company-scoped API styles:

1. Canonical multi-company routes use a path-scoped company id:
   - `/companies/{opc_id}/...`
2. Legacy `/opc/...` routes remain supported, but they are **header-scoped**:
   - clients **must** send `x-coevo-opc-id: <opc_id>`

The server treats company scope as authoritative and rejects:

- missing `x-coevo-opc-id` on legacy company-scoped `/opc/...` requests
- malformed `opc_id`
- mismatches between the header company id and any body/path company id supplied by the client

This means the chosen isolation rule for old `/opc/...` endpoints is:

- **header-based opc isolation** for legacy routes
- **path-based opc isolation** for canonical `/companies/{opc_id}/...` routes

Employee file-backed storage also follows the canonical company scope:

- `{opc_id}/employees/{agent_id}/passport.json`
- `{opc_id}/employees/{agent_id}/prompt.md`
- `{opc_id}/employees/{agent_id}/prompt_versions/*.md`
- `{opc_id}/employees/{agent_id}/identity.md`
- `{opc_id}/employees/{agent_id}/soul.md`
- `{opc_id}/employees/{agent_id}/agents.md`

---

## Error Format (RFC 9457)

All errors follow the Problem Details format:

```json
{
  "type": "https://coevo.dev/errors/risk-denied",
  "title": "Risk Threshold Not Met",
  "status": 403,
  "detail": "Available 0.35 < Required 0.80",
  "instance": "/risk/evaluate",
  "error_code": "RISK_DENIED"
}
```
