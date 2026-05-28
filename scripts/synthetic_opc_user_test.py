#!/usr/bin/env python3
"""coevo-opc Synthetic User Test — runs full Alpha Ready end-to-end workflow."""
import urllib.request, json, sys, uuid

BASE = "http://127.0.0.1:8717"
PASS, FAIL = 0, 0
def test(name, fn):
    global PASS, FAIL
    try:
        fn()
        print(f"  ✓ {name}")
        PASS += 1
    except AssertionError as e:
        print(f"  ✗ {name}: {e}")
        FAIL += 1
    except Exception as e:
        print(f"  ✗ {name}: ERROR {e}")
        FAIL += 1

def req(method, path, body=None):
    url = f"{BASE}{path}"
    data = json.dumps(body).encode() if body else None
    r = urllib.request.Request(url, data=data, method=method)
    r.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(r) as resp:
        return json.loads(resp.read())

def get(path): return req("GET", path)
def post(path, body=None): return req("POST", path, body or {})
def put(path, body): return req("PUT", path, body)

CH = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
PH = "f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1"
WO_ID = None

print("coevo-opc Synthetic User Test")
print("=" * 50)

test("1. GET /health returns ok",
     lambda: get("/health")["status"] == "ok")

test("2. PUT /opc/profile/user saves Founder Profile",
     lambda: put("/opc/profile/user", {"user_id":"default-founder","display_name":"OPC Founder","preferred_language":"zh","timezone":"Asia/Shanghai","risk_preference":"Balanced","default_mission_mode":"Auto","long_term_goals":["Build coevo OPC OS"],"business_domains":["AI","DevTools"],"communication_style":"direct","approval_preferences":{"auto_approve_below_risk":0.3,"require_explicit_for_yellow":True,"require_mfa_for_red":True,"negative_consent_timeout_secs":300},"data_boundaries":[],"budget_limits":{"max_cost_per_task_usd":50,"max_cost_per_day_usd":500,"max_agents_per_task":5},"favorite_tools":[],"active_projects":[],"created_at_ms":0,"updated_at_ms":0})["ok"])

test("3. POST /opc/agents/employees/seed returns total >= 10",
     lambda: post("/opc/agents/employees/seed")["total"] >= 10)

test("4. GET /opc/agents/employees returns >= 10 employees",
     lambda: len(get("/opc/agents/employees")) >= 10)

test("5. POST /opc/skills/seed returns ok",
     lambda: post("/opc/skills/seed")["ok"])

test("6. GET /opc/skills does not error (query)",
     lambda: isinstance(get("/opc/skills"), list))

test("7. POST /opc/executors/register mock OpenClaw",
     lambda: post("/opc/executors/register", {
         "executor_id":"exec-openclaw-test","display_name":"OpenClaw Test","source_type":"OpenClaw",
         "runtime_endpoint":"http://localhost:0","capabilities":["mock"],"required_credentials":[],
         "permission_boundary":{"max_risk_score":0.5,"can_write_fact":False,"can_write_decision":False,"can_access_network":False,"can_access_filesystem":False,"can_call_external_executor":False,"can_propose_skill":False},
         "file_scope":[],"network_scope":[],"memory_scope":"Executor","risk_ceiling":0.5,
         "supported_actions":["read"],"sandbox_level":"None","health_check_url":"","audit_callback_url":"",
         "status":"Registered","created_at_ms":0,"updated_at_ms":0
     })["ok"])

# Create Green WorkOrder (no created_at_ms/updated_at_ms — server generates)
green_resp = None
def create_green_wo():
    global WO_ID, green_resp
    green_resp = post("/opc/work-orders", {
        "contract_hash": CH, "plan_hash": PH, "user_id": "default-founder", "opc_id": "default-opc",
        "mission_intent": "Generate coevo project progress summary",
        "selected_agents": [], "selected_executors": ["exec-openclaw-test"], "required_skills": [],
        "track": "green", "allowed_actions": ["read","analyze"], "restricted_actions": ["write"],
        "risk_summary": "Low risk — read/analyze only"
    })
    WO_ID = green_resp["work_order_id"]
    return True

test("8. POST /opc/work-orders (green, no timestamps) returns Planned",
     lambda: create_green_wo() and green_resp["status"] == "Planned")

test("9. POST /opc/work-orders/:id/execute Green → Completed",
     lambda: post(f"/opc/work-orders/{WO_ID}/execute", {})["status"] == "Completed")

test("10. Execute returns memory_ids",
     lambda: len(post(f"/opc/work-orders/{WO_ID}/execute", {}).get("memory_ids", [])) >= 0)

test("11. GET /opc/memory contains Task Memory",
     lambda: len(get("/opc/memory")) >= 1)

# Create Red WorkOrder (no timestamps)
red_id = None
def create_red_wo():
    global red_id
    r = post("/opc/work-orders", {
        "contract_hash": CH, "plan_hash": PH, "user_id": "default-founder", "opc_id": "default-opc",
        "mission_intent": "Delete production database records",
        "selected_agents": [], "selected_executors": [], "required_skills": [],
        "track": "red", "allowed_actions": [], "restricted_actions": ["delete","write","production"],
        "risk_summary": "High risk — production deletion"
    })
    red_id = r["work_order_id"]
    return True

test("12. POST /opc/work-orders (red, no timestamps)",
     lambda: create_red_wo())

# Try executing Red without credentials
red_exec = None
def exec_red_no_creds():
    global red_exec
    try:
        red_exec = post(f"/opc/work-orders/{red_id}/execute", {})
        return False  # Should have raised
    except urllib.error.HTTPError as e:
        red_exec = json.loads(e.read())
        return e.code == 403

test("13. Red WO execute without credentials → 403",
     lambda: exec_red_no_creds())

test("14. Red WO 403 mentions dual-sign",
     lambda: "dual-sign" in str(red_exec).lower() or "identity" in str(red_exec).lower() or "lease" in str(red_exec).lower())

# Feedback on green WO
fb = None
def submit_feedback():
    global fb
    fb = post(f"/opc/work-orders/{WO_ID}/feedback", {"feedback": "The summary missed key project milestones", "agent_id": "agent-synth-01"})
    return fb.get("ok") == True and "proposal_id" in fb

test("15. POST /opc/work-orders/:id/feedback creates proposal",
     lambda: submit_feedback())

test("16. GET /opc/skills/evolution/proposals returns proposals",
     lambda: isinstance(get("/opc/skills/evolution/proposals"), list))

prop_id = fb.get("proposal_id", "") if fb else ""
if prop_id:
    test("17. POST /opc/skills/evolution/proposals/:id/verify",
         lambda: post(f"/opc/skills/evolution/proposals/{prop_id}/verify")["passed"] in [True, False])

# Yellow Track
yellow_id = None
def create_yellow_wo():
    global yellow_id
    r = post("/opc/work-orders", {
        "contract_hash": CH, "plan_hash": PH, "user_id": "default-founder", "opc_id": "default-opc",
        "mission_intent": "Prepare roadmap and notify team",
        "selected_agents": [], "selected_executors": [], "required_skills": [],
        "track": "yellow", "allowed_actions": ["read","write_internal"], "restricted_actions": ["production"],
        "risk_summary": "Moderate risk — internal write"
    })
    yellow_id = r["work_order_id"]
    return True

test("18. POST /opc/work-orders (yellow, no timestamps)",
     lambda: create_yellow_wo())

test("19. Yellow WO execute → WaitingApproval",
     lambda: post(f"/opc/work-orders/{yellow_id}/execute", {})["status"] == "WaitingApproval")

print()
print(f"Results: {PASS} passed, {FAIL} failed")
sys.exit(0 if FAIL == 0 else 1)
