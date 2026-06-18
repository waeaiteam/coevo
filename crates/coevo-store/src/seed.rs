//! Seed AI Employees for coevo OPC.
//! 10 built-in AI employees with passports, departments, and permission boundaries.

use coevo_core::opc::*;
use coevo_core::reputation::ReputationVector;

/// The company secretary's agent id. Created for every company; reports to the founder.
pub const SECRETARY_AGENT_ID: &str = "agent-secretary-01";

/// System prompt that makes the secretary an intelligent dispatcher rather than a worker.
pub const SECRETARY_SYSTEM_PROMPT: &str = "You are the company Secretary, the founder's chief of staff. \
You do not do the work yourself. Your job is to truly understand what the founder is asking for \
(in plain language, not keywords), decide which department(s) should handle it, and break the request \
into clear sub-tasks for the responsible department heads. Consider the company's existing departments, \
employees, and skills. Be decisive but never expand scope beyond what the founder asked. You only propose \
who should do what — you never decide risk levels or grant authority; the governance layer does that.";

pub fn seed_employees() -> Vec<AgentEmployee> {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    vec![
        // Company secretary: the intelligent dispatcher. Created for every company,
        // reports directly to the founder, understands intent and routes work to the
        // right department heads. See coevo-server dispatch.rs.
        secretary(now),
        employee(
            "agent-founder-01",
            "Founder Assistant",
            Department::FounderOffice,
            "FounderOffice",
            vec!["Hypothesis", "Suggestion"],
            vec!["DRAFT_ONLY"],
            0.3,
            now,
        ),
        employee(
            "agent-pm-01",
            "Product Manager",
            Department::Product,
            "Product",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.3,
            now,
        ),
        employee(
            "agent-research-01",
            "Research Agent",
            Department::Research,
            "Research",
            vec!["Hypothesis", "Suggestion"],
            vec!["DRAFT_ONLY"],
            0.4,
            now,
        ),
        employee(
            "agent-engineer-01",
            "Engineer",
            Department::Engineering,
            "Engineering",
            vec!["Hypothesis", "Suggestion"],
            vec!["DRAFT_ONLY"],
            0.4,
            now,
        ),
        employee(
            "agent-critic-01",
            "Critic",
            Department::Governance,
            "Governance",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.5,
            now,
        ),
        employee(
            "agent-risk-01",
            "Risk & Compliance",
            Department::Governance,
            "Governance",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.6,
            now,
        ),
        employee(
            "agent-sre-01",
            "SRE Diagnostic",
            Department::SRE,
            "SRE",
            vec!["Hypothesis", "Suggestion"],
            vec!["DRAFT_ONLY"],
            0.4,
            now,
        ),
        employee(
            "agent-growth-01",
            "Growth Agent",
            Department::Growth,
            "Growth",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.3,
            now,
        ),
        employee(
            "agent-finance-01",
            "Finance Agent",
            Department::Finance,
            "Finance",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.4,
            now,
        ),
        employee(
            "agent-synth-01",
            "Synthesizer",
            Department::FounderOffice,
            "FounderOffice",
            vec!["Suggestion"],
            vec!["DRAFT_ONLY"],
            0.3,
            now,
        ),
    ]
}

fn employee(
    agent_id: &str,
    name: &str,
    dept: Department,
    role: &str,
    layers: Vec<&str>,
    actions: Vec<&str>,
    risk_ceiling: f64,
    now: u64,
) -> AgentEmployee {
    AgentEmployee {
        agent_id: agent_id.to_string(),
        display_name: name.to_string(),
        department: dept,
        role: role.to_string(),
        passport: AgentPassport {
            passport_id: format!("passport-{}", agent_id),
            issued_by: "coevo-seed".to_string(),
            roles: vec![role.to_string()],
            capabilities: vec!["analysis".to_string(), "planning".to_string()],
            restrictions: vec![
                "no production write".to_string(),
                "no financial transfer".to_string(),
            ],
            expires_at_ms: None,
        },
        model_profile: ModelProviderProfile {
            provider: "mock".to_string(),
            base_url: String::new(),
            api_key_ref: String::new(),
            default_model: "gpt-4o".to_string(),
            fast_model: "gpt-4o-mini".to_string(),
            reasoning_model: "o1".to_string(),
            structured_output_model: "gpt-4o".to_string(),
            timeout_ms: 30000,
            max_tokens: 4096,
            max_cost_per_task_usd: 1.0,
        },
        tool_scopes: vec!["urn:coevo:tool:read".to_string()],
        memory_scope: MemoryScope::Agent,
        permission_boundary: PermissionBoundary {
            max_risk_score: risk_ceiling,
            can_write_fact: false,
            can_write_decision: false,
            can_access_network: false,
            can_access_filesystem: false,
            can_call_external_executor: false,
            can_propose_skill: true,
        },
        allowed_cognitive_layers: layers.into_iter().map(String::from).collect(),
        allowed_action_modes: actions.into_iter().map(String::from).collect(),
        risk_ceiling,
        reputation_vector: ReputationVector::new(agent_id.to_string()),
        supervisor_agent_id: Some("agent-founder-01".to_string()),
        lifecycle_status: LifecycleStatus::Active,
        system_prompt: String::new(),
        created_at_ms: now,
        updated_at_ms: now,
    }
}

/// Build the company secretary employee: the dispatcher that reports to the founder.
fn secretary(now: u64) -> AgentEmployee {
    let mut s = employee(
        SECRETARY_AGENT_ID,
        "Secretary",
        Department::FounderOffice,
        "Secretary",
        vec!["Suggestion"],
        vec!["DRAFT_ONLY"],
        0.3,
        now,
    );
    // The secretary answers to the founder directly, not to the founder-assistant.
    s.supervisor_agent_id = None;
    s.system_prompt = SECRETARY_SYSTEM_PROMPT.to_string();
    s.passport.roles = vec!["Secretary".to_string(), "Dispatcher".to_string()];
    s.passport.capabilities = vec!["planning".to_string(), "dispatch".to_string()];
    s
}
