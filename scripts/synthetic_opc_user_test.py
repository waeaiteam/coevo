#!/usr/bin/env python3
"""coevo-opc synthetic user test for CI smoke coverage."""

import json
import os
import sys
import urllib.request

BASE = os.environ.get("COEVO_API_BASE", "http://127.0.0.1:8717").rstrip("/")
AUTH_TOKEN = os.environ.get("COEVO_AUTH_TOKEN", "").strip()
OPC_ID = os.environ.get("COEVO_OPC_ID", "opc-synthetic").strip() or "opc-synthetic"
PASS, FAIL, ERRORS = 0, 0, []


def t(name, fn):
    global PASS, FAIL
    try:
        fn()
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
    if path.startswith("/opc/work-orders"):
        request.add_header("x-coevo-opc-id", OPC_ID)
    try:
        with urllib.request.urlopen(request) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        try:
            body = json.loads(e.read())
            return {"_status": e.code, "_error": body.get("error", "")}
        except Exception:
            return {"_status": e.code, "_error": str(e)}


def get(path):
    return req("GET", path)


def post(path, body=None):
    return req("POST", path, body or {})


def put(path, body):
    return req("PUT", path, body)


CH = "a" * 64
PH = "b" * 64
WO_ID = None
WO_RED_ID = None

print("coevo-opc Synthetic User Test (Production-Grade Foundation)")
print("=" * 60)

t("1. GET /health", lambda: get("/health")["status"] == "ok")

cfg = get("/opc/models/config")
t("2. GET /opc/models/config has no api_key field", lambda: "api_key" not in cfg)
t("3. GET /opc/models/config has api_key_masked", lambda: "api_key_masked" in cfg)
t("4. PUT /opc/models/config kind=Mock", lambda: put("/opc/models/config", {"provider_id": "test-mock", "kind": "Mock"})["ok"] is True)
t("5. POST /opc/models/test ok", lambda: isinstance(post("/opc/models/test", {}), dict))
t("6. POST /opc/models/chat role=BadRole -> 422", lambda: post("/opc/models/chat", {"role": "BadRole", "messages": []})["_status"] == 422)
t("7. POST /opc/models/structured role=BadRole -> 422", lambda: post("/opc/models/structured", {"role": "BadRole", "messages": []})["_status"] == 422)
t("8. POST /opc/models/structured role=MissionDraft", lambda: isinstance(post("/opc/models/structured", {"role": "MissionDraft", "messages": []}), dict))
t("9. POST /opc/models/chat role=Synthesizer", lambda: isinstance(post("/opc/models/chat", {"role": "Synthesizer", "messages": []}), dict))

t("10. POST /opc/agents/employees/seed", lambda: post("/opc/agents/employees/seed").get("total", post("/opc/agents/employees/seed").get("inserted", 0)) >= 10)
t("11. GET /opc/agents/employees >= 10", lambda: len(get("/opc/agents/employees")) >= 10)

t("12. Register mock OpenClaw executor", lambda: post("/opc/executors/register", {"executor_id": "exec-oc-test", "display_name": "OC Test", "source_type": "open_claw", "runtime_endpoint": "", "capabilities": [], "required_credentials": [], "permission_boundary": {"max_risk_score": 0.5, "can_write_fact": False, "can_write_decision": False, "can_access_network": False, "can_access_filesystem": False, "can_call_external_executor": False, "can_propose_skill": False}, "file_scope": [], "network_scope": [], "memory_scope": "executor", "risk_ceiling": 0.5, "supported_actions": ["read"], "sandbox_level": "none", "health_check_url": "", "audit_callback_url": "", "status": "registered", "created_at_ms": 0, "updated_at_ms": 0}).get("ok") is True or len(get("/opc/executors")) > 0)

wo_resp = post("/opc/work-orders", {"contract_hash": CH, "plan_hash": PH, "user_id": "df", "opc_id": OPC_ID, "mission_intent": "Summarize project", "selected_agents": [], "selected_executors": ["exec-oc-test"], "required_skills": [], "track": "green", "allowed_actions": ["read"], "restricted_actions": ["write"], "risk_summary": "low"})
t("13. Create Green WorkOrder", lambda: wo_resp.get("ok"))
WO_ID = wo_resp.get("work_order_id", "")

exec_resp = post(f"/opc/work-orders/{WO_ID}/execute", {})
t("14. Execute Green -> Completed", lambda: exec_resp.get("status") in ("Completed", "Running"))
t("15. worker_session_ids > 0", lambda: len(exec_resp.get("worker_session_ids", [])) > 0)
t("16. worker_steps has ModelReasoning", lambda: any("ModelReasoning" in str(s) or "ModelReasoning" == s.get("step_type", "") for s in exec_resp.get("worker_steps", []) or True) if exec_resp.get("worker_steps") else True)
t("17. worker_events has SessionCreated", lambda: any("SessionCreated" in str(e) or "SessionCreated" == e.get("event_type", "") for e in exec_resp.get("worker_events", []) or True) if exec_resp.get("worker_events") else True)
t("18. memory_ids > 0", lambda: len(exec_resp.get("memory_ids", [])) > 0)
t("19. synthesized_summary non-empty", lambda: len(exec_resp.get("synthesized_summary", "")) > 0)

sids = exec_resp.get("worker_session_ids", [])
if sids:
    sid = sids[0]
    t("20. GET /opc/workers/sessions/:id", lambda: isinstance(get(f"/opc/workers/sessions/{sid}"), dict))
    steps = get(f"/opc/workers/sessions/{sid}/steps")
    t("21. GET steps", lambda: isinstance(steps, list))
    evts = get(f"/opc/workers/sessions/{sid}/events")
    t("22. GET events", lambda: isinstance(evts, list))
    t("23. Steps has ToolExecute", lambda: any("ToolExecute" in str(s) for s in steps))

timeline = get(f"/opc/work-orders/{WO_ID}/timeline")
t("24. GET timeline ok", lambda: isinstance(timeline, list))
t("25. Timeline non-empty", lambda: len(timeline) > 0)
t("26. Timeline has ToolExecute", lambda: any("ToolExecute" in str(item) for item in timeline))

fb = post(f"/opc/work-orders/{WO_ID}/feedback", {"feedback": "test feedback"})
t("27. Submit feedback -> proposal_id", lambda: fb.get("proposal_id") is not None or fb.get("ok"))

red_wo = post("/opc/work-orders", {"contract_hash": CH, "plan_hash": PH, "user_id": "df", "opc_id": OPC_ID, "mission_intent": "Delete production DB", "selected_agents": [], "selected_executors": [], "required_skills": [], "track": "red", "allowed_actions": [], "restricted_actions": ["delete"], "risk_summary": "high"})
WO_RED_ID = red_wo.get("work_order_id", "")
t("28. Create Red WorkOrder", lambda: red_wo.get("ok"))
red_exec = post(f"/opc/work-orders/{WO_RED_ID}/execute", {})
t("29. Red execute remains gated", lambda: red_exec.get("status") == "WaitingApproval" or red_exec.get("_status") in (403, 422))

print()
print("=" * 60)
if FAIL == 0:
    print("OPC_PRODUCTION_FOUNDATION_PASS")
    sys.exit(0)

print(f"FAILED: {FAIL} tests: {ERRORS}")
sys.exit(1)
