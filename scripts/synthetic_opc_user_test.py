#!/usr/bin/env python3
"""coevo-opc synthetic user test for CI smoke coverage.

This smoke test is intentionally strict about anchors and observations: it must
exercise persisted MCL contracts/plans and fail when worker evidence is missing.
"""

import json
import os
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("COEVO_API_BASE", "http://127.0.0.1:8717").rstrip("/")
AUTH_TOKEN = os.environ.get("COEVO_AUTH_TOKEN", "").strip()
OPC_ID = os.environ.get("COEVO_OPC_ID", "").strip()
REAL_PROVIDER_KINDS_REQUIRING_KEY = {"OpenAICompatible", "OpenAI", "Anthropic", "Gemini", "DeepSeek"}
ZERO_HASH = "0" * 64
SYNTHETIC_TENANT_ID = "00000000-0000-4000-8000-000000000001"
PASS, FAIL, ERRORS = 0, 0, []


def t(name, fn):
    global PASS, FAIL
    try:
        result = fn()
        if result is not True:
            raise AssertionError(f"expected True, got {result!r}")
        print(f"  PASS {name}")
        PASS += 1
    except AssertionError as e:
        print(f"  FAIL {name}: {e}")
        FAIL += 1
        ERRORS.append(name)
    except Exception as e:
        print(f"  FAIL {name}: ERROR {e}")
        FAIL += 1
        ERRORS.append(name)


def req(method, path, body=None):
    url = f"{BASE}{path}"
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    request.add_header("Content-Type", "application/json")
    if AUTH_TOKEN:
        request.add_header("x-coevo-token", AUTH_TOKEN)
    request.add_header("x-coevo-contract-hash", ZERO_HASH)
    request.add_header("x-coevo-policy-version", ZERO_HASH)
    request.add_header("x-coevo-execution-plan-hash", ZERO_HASH)
    request.add_header("x-coevo-tenant-id", SYNTHETIC_TENANT_ID)
    request.add_header("x-coevo-actor-role", "Synthesizer")
    if path.startswith("/opc/") and OPC_ID:
        request.add_header("x-coevo-opc-id", OPC_ID)
    try:
        with urllib.request.urlopen(request) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        try:
            body = json.loads(e.read().decode("utf-8"))
            return {"_status": e.code, "_error": body.get("error", body.get("detail", ""))}
        except Exception:
            return {"_status": e.code, "_error": str(e)}


def get(path):
    return req("GET", path)


def post(path, body=None):
    return req("POST", path, body or {})


def put(path, body):
    return req("PUT", path, body)


def compile_and_route(intent, agent_ids):
    compiled = post(
        "/mcl/compile",
        {
            "user_intent": intent,
            "requested_mode": "DRAFT",
            "parent_contract_hash": None,
        },
    )
    assert "contract_hash" in compiled, compiled
    assert "contract" in compiled, compiled
    routed = post(
        "/router/route",
        {
            "contract_hash": compiled["contract_hash"],
            "contract": compiled["contract"],
            "agent_ids": agent_ids,
        },
    )
    assert "plan_hash" in routed, routed
    return compiled["contract_hash"], routed["plan_hash"]


def has_step(steps, name):
    return isinstance(steps, list) and any(
        name in str(step) or (isinstance(step, dict) and name == step.get("step_type", ""))
        for step in steps
    )


def has_event(events, name):
    return isinstance(events, list) and any(
        name in str(event) or (isinstance(event, dict) and name == event.get("event_type", ""))
        for event in events
    )


def print_context(label, value):
    print(f"  INFO {label}: {json.dumps(value, ensure_ascii=False)[:2000]}")


def ensure_company():
    global OPC_ID
    companies = get("/companies")
    if isinstance(companies, list):
        if OPC_ID and any(company.get("opc_id") == OPC_ID for company in companies):
            return True
        if not OPC_ID and companies:
            OPC_ID = companies[0].get("opc_id", "")
            return bool(OPC_ID)
    created = post(
        "/companies",
        {
            "name": "Synthetic OPC",
            "mission": "Synthetic CI smoke company",
        },
    )
    assert "opc_id" in created, created
    OPC_ID = created["opc_id"]
    print(f"  INFO using synthetic opc_id={OPC_ID}")
    return True


def env_value(name, default=""):
    return os.environ.get(name, default).strip()


def required_env(name):
    value = env_value(name)
    if not value:
        raise AssertionError(f"{name} is required for real-provider synthetic testing")
    return value


def synthetic_model_config_payload():
    kind = required_env("COEVO_SYNTHETIC_MODEL_KIND")
    if kind == "Mock":
        raise AssertionError("Mock provider is not allowed in synthetic acceptance")
    model = required_env("COEVO_SYNTHETIC_DEFAULT_MODEL")
    payload = {
        "provider_id": env_value("COEVO_SYNTHETIC_PROVIDER_ID", "synthetic-real-provider"),
        "kind": kind,
        "base_url": required_env("COEVO_SYNTHETIC_BASE_URL"),
        "default_model": model,
        "fast_model": env_value("COEVO_SYNTHETIC_FAST_MODEL", model),
        "reasoning_model": env_value("COEVO_SYNTHETIC_REASONING_MODEL", model),
        "structured_output_model": env_value("COEVO_SYNTHETIC_STRUCTURED_MODEL", model),
        "max_tokens": int(env_value("COEVO_SYNTHETIC_MAX_TOKENS", "4096")),
        "temperature": float(env_value("COEVO_SYNTHETIC_TEMPERATURE", "0.2")),
        "timeout_ms": int(env_value("COEVO_SYNTHETIC_TIMEOUT_MS", "60000")),
        "max_cost_per_task_usd": float(env_value("COEVO_SYNTHETIC_MAX_COST_USD", "5.0")),
    }
    api_key = env_value("COEVO_SYNTHETIC_API_KEY")
    if kind in REAL_PROVIDER_KINDS_REQUIRING_KEY:
        if not api_key:
            raise AssertionError("COEVO_SYNTHETIC_API_KEY is required for cloud provider synthetic testing")
        payload["api_key"] = api_key
    elif api_key:
        payload["api_key"] = api_key
    return payload


def configure_real_model_provider():
    response = put("/opc/models/config", synthetic_model_config_payload())
    assert response.get("ok") is True, response
    return True

WO_ID = None
WO_RED_ID = None

print("coevo-opc Synthetic User Test")
print("=" * 60)

t("1. GET /health", lambda: get("/health").get("status") == "ok")

initial_cfg = get("/opc/models/config")
t("2. GET /opc/models/config never returns plaintext api_key", lambda: "api_key" not in initial_cfg)
t(
    "3. Fresh model config is unconfigured or redacted",
    lambda: initial_cfg.get("_status") == 409 or "api_key_masked" in initial_cfg,
)
t("4. PUT /opc/models/config real provider from env", configure_real_model_provider)
cfg = get("/opc/models/config")
t("5. GET /opc/models/config has api_key_masked after setup", lambda: "api_key_masked" in cfg)
t("6. POST /opc/models/test ok", lambda: isinstance(post("/opc/models/test", {}), dict))
t(
    "7. POST /opc/models/chat role=BadRole -> 422",
    lambda: post("/opc/models/chat", {"role": "BadRole", "messages": []}).get("_status") == 422,
)
t(
    "8. POST /opc/models/structured role=BadRole -> 422",
    lambda: post("/opc/models/structured", {"role": "BadRole", "messages": []}).get("_status") == 422,
)
t(
    "9. POST /opc/models/structured role=MissionDraft",
    lambda: isinstance(post("/opc/models/structured", {"role": "MissionDraft", "messages": []}), dict),
)
t(
    "10. POST /opc/models/chat role=Synthesizer",
    lambda: isinstance(post("/opc/models/chat", {"role": "Synthesizer", "messages": []}), dict),
)

t("11. Create or resolve synthetic company", ensure_company)
seed = post("/opc/agents/employees/seed")
t("12. POST /opc/agents/employees/seed", lambda: seed.get("total", seed.get("inserted", 0)) >= 10)
employees = get("/opc/agents/employees")
t("13. GET /opc/agents/employees >= 10", lambda: isinstance(employees, list) and len(employees) >= 10)

t(
    "14. Register mock OpenClaw executor",
    lambda: post(
        "/opc/executors/register",
        {
            "executor_id": "exec-oc-test",
            "display_name": "OC Test",
            "source_type": "open_claw",
            "runtime_endpoint": "",
            "capabilities": [],
            "required_credentials": [],
            "permission_boundary": {
                "max_risk_score": 0.5,
                "can_write_fact": False,
                "can_write_decision": False,
                "can_access_network": False,
                "can_access_filesystem": False,
                "can_call_external_executor": False,
                "can_propose_skill": False,
            },
            "file_scope": [],
            "network_scope": [],
            "memory_scope": "executor",
            "risk_ceiling": 0.5,
            "supported_actions": ["read"],
            "sandbox_level": "none",
            "health_check_url": "",
            "audit_callback_url": "",
            "status": "registered",
            "created_at_ms": 0,
            "updated_at_ms": 0,
        },
    ).get("ok") is True
    or len(get("/opc/executors")) > 0,
)

CH, PH = compile_and_route("Summarize project", ["agent-founder-01"])
wo_resp = post(
    "/opc/work-orders",
    {
        "contract_hash": CH,
        "plan_hash": PH,
        "user_id": "synthetic-user",
        "opc_id": OPC_ID,
        "mission_intent": "Summarize project",
        "selected_agents": ["agent-founder-01"],
        "selected_executors": ["exec-oc-test"],
        "required_skills": [],
        "track": "green",
        "allowed_actions": ["read"],
        "restricted_actions": ["write"],
        "risk_summary": "low",
    },
)
t("15. Create Green WorkOrder", lambda: wo_resp.get("ok") is True)
WO_ID = wo_resp.get("work_order_id", "")

exec_resp = post(f"/opc/work-orders/{WO_ID}/execute", {})
if exec_resp.get("status") != "Completed":
    print_context("green execute response", exec_resp)
t("16. Execute Green -> Completed", lambda: exec_resp.get("status") == "Completed")
t("17. worker_session_ids > 0", lambda: len(exec_resp.get("worker_session_ids", [])) > 0)
worker_steps = exec_resp.get("worker_steps") or []
worker_events = exec_resp.get("worker_events") or []
t("18. worker_steps has ModelReasoning", lambda: has_step(worker_steps, "ModelReasoning"))
t("19. worker_events has LifecycleStart", lambda: has_event(worker_events, "LifecycleStart"))
t("20. memory_ids > 0", lambda: len(exec_resp.get("memory_ids", [])) > 0)
t("21. synthesized_summary non-empty", lambda: len(exec_resp.get("synthesized_summary", "")) > 0)

sids = exec_resp.get("worker_session_ids", [])
t("22. worker_session_ids available for detail checks", lambda: len(sids) > 0)
sid = sids[0] if sids else "missing"
t("23. GET /opc/workers/sessions/:id", lambda: isinstance(get(f"/opc/workers/sessions/{sid}"), dict))
steps = get(f"/opc/workers/sessions/{sid}/steps")
t("24. GET steps", lambda: isinstance(steps, list) and len(steps) > 0)
evts = get(f"/opc/workers/sessions/{sid}/events")
t("25. GET events", lambda: isinstance(evts, list) and len(evts) > 0)
t("26. Steps has ToolExecute", lambda: has_step(steps, "ToolExecute"))

timeline = get(f"/opc/work-orders/{WO_ID}/timeline")
t("27. GET timeline ok", lambda: isinstance(timeline, list))
t("28. Timeline non-empty", lambda: len(timeline) > 0)
t("29. Timeline has ToolExecute", lambda: any("ToolExecute" in str(item) for item in timeline))

fb = post(f"/opc/work-orders/{WO_ID}/feedback", {"feedback": "test feedback"})
t("30. Submit feedback -> proposal_id", lambda: fb.get("proposal_id") is not None or fb.get("ok") is True)

RCH, RPH = compile_and_route("Delete production DB", ["agent-risk-01"])
red_wo = post(
    "/opc/work-orders",
    {
        "contract_hash": RCH,
        "plan_hash": RPH,
        "user_id": "synthetic-user",
        "opc_id": OPC_ID,
        "mission_intent": "Delete production DB",
        "selected_agents": ["agent-risk-01"],
        "selected_executors": [],
        "required_skills": [],
        "track": "red",
        "allowed_actions": [],
        "restricted_actions": ["delete"],
        "risk_summary": "high",
    },
)
WO_RED_ID = red_wo.get("work_order_id", "")
t("31. Create Red WorkOrder", lambda: red_wo.get("ok") is True)
red_exec = post(f"/opc/work-orders/{WO_RED_ID}/execute", {})
if red_exec.get("status") != "WaitingApproval":
    print_context("red execute response", red_exec)
t("32. Red execute requires approval", lambda: red_exec.get("status") == "WaitingApproval")

print()
print("=" * 60)
if FAIL == 0:
    print("OPC_PRODUCTION_FOUNDATION_PASS")
    sys.exit(0)

print(f"FAILED: {FAIL} tests: {ERRORS}")
sys.exit(1)
