use coevo_core::opc::{
    Department, ExecutorSourceType, ExecutorStatus, LifecycleStatus, MemoryScope, MemoryStatus,
    SandboxLevel,
};
use coevo_core::skills::SkillStatus;

pub(crate) fn department_to_db(value: Department) -> &'static str {
    match value {
        Department::FounderOffice => "FounderOffice",
        Department::Product => "Product",
        Department::Engineering => "Engineering",
        Department::Research => "Research",
        Department::Growth => "Growth",
        Department::Finance => "Finance",
        Department::Legal => "Legal",
        Department::SRE => "SRE",
        Department::Design => "Design",
        Department::Content => "Content",
        Department::Governance => "Governance",
        Department::Custom => "Custom",
    }
}

pub(crate) fn department_from_db(value: &str) -> Department {
    match value {
        "FounderOffice" | "founder_office" => Department::FounderOffice,
        "Product" | "product" => Department::Product,
        "Engineering" | "engineering" => Department::Engineering,
        "Research" | "research" => Department::Research,
        "Growth" | "growth" => Department::Growth,
        "Finance" | "finance" => Department::Finance,
        "Legal" | "legal" => Department::Legal,
        "SRE" | "sre" | "s_r_e" => Department::SRE,
        "Design" | "design" => Department::Design,
        "Content" | "content" => Department::Content,
        "Governance" | "governance" => Department::Governance,
        _ => Department::Custom,
    }
}

pub(crate) fn memory_scope_to_db(value: MemoryScope) -> &'static str {
    match value {
        MemoryScope::User => "User",
        MemoryScope::Company => "Company",
        MemoryScope::Agent => "Agent",
        MemoryScope::Task => "Task",
        MemoryScope::Skill => "Skill",
        MemoryScope::Executor => "Executor",
        MemoryScope::Audit => "Audit",
    }
}

pub(crate) fn memory_scope_from_db(value: &str) -> MemoryScope {
    match value {
        "User" | "user" => MemoryScope::User,
        "Company" | "company" => MemoryScope::Company,
        "Agent" | "agent" => MemoryScope::Agent,
        "Task" | "task" => MemoryScope::Task,
        "Skill" | "skill" => MemoryScope::Skill,
        "Executor" | "executor" => MemoryScope::Executor,
        "Audit" | "audit" => MemoryScope::Audit,
        _ => MemoryScope::Agent,
    }
}

pub(crate) fn memory_status_to_db(value: MemoryStatus) -> &'static str {
    match value {
        MemoryStatus::Active => "Active",
        MemoryStatus::Stale => "Stale",
        MemoryStatus::Revoked => "Revoked",
    }
}

pub(crate) fn memory_status_from_db(value: &str) -> MemoryStatus {
    match value {
        "Active" | "active" => MemoryStatus::Active,
        "Stale" | "stale" => MemoryStatus::Stale,
        "Revoked" | "revoked" => MemoryStatus::Revoked,
        _ => MemoryStatus::Active,
    }
}

pub(crate) fn lifecycle_status_to_db(value: LifecycleStatus) -> &'static str {
    match value {
        LifecycleStatus::Draft => "Draft",
        LifecycleStatus::Active => "Active",
        LifecycleStatus::Suspended => "Suspended",
        LifecycleStatus::Retired => "Retired",
    }
}

pub(crate) fn lifecycle_status_from_db(value: &str) -> LifecycleStatus {
    match value {
        "Draft" | "draft" => LifecycleStatus::Draft,
        "Active" | "active" => LifecycleStatus::Active,
        "Suspended" | "suspended" => LifecycleStatus::Suspended,
        "Retired" | "retired" => LifecycleStatus::Retired,
        _ => LifecycleStatus::Draft,
    }
}

pub(crate) fn executor_source_type_to_db(value: ExecutorSourceType) -> &'static str {
    match value {
        ExecutorSourceType::Hermes => "Hermes",
        ExecutorSourceType::OpenClaw => "OpenClaw",
        ExecutorSourceType::MCP => "MCP",
        ExecutorSourceType::Local302AI => "Local302AI",
        ExecutorSourceType::LocalProcess => "LocalProcess",
        ExecutorSourceType::Browser => "Browser",
        ExecutorSourceType::Docker => "Docker",
        ExecutorSourceType::Custom => "Custom",
    }
}

pub(crate) fn executor_source_type_from_db(value: &str) -> ExecutorSourceType {
    match value {
        "Hermes" | "hermes" => ExecutorSourceType::Hermes,
        "OpenClaw" | "open_claw" => ExecutorSourceType::OpenClaw,
        "MCP" | "mcp" | "m_c_p" => ExecutorSourceType::MCP,
        "Local302AI" | "local302_ai" | "local302_a_i" | "local_302_ai" => {
            ExecutorSourceType::Local302AI
        }
        "LocalProcess" | "local_process" => ExecutorSourceType::LocalProcess,
        "Browser" | "browser" => ExecutorSourceType::Browser,
        "Docker" | "docker" => ExecutorSourceType::Docker,
        _ => ExecutorSourceType::Custom,
    }
}

pub(crate) fn sandbox_level_to_db(value: SandboxLevel) -> &'static str {
    match value {
        SandboxLevel::None => "None",
        SandboxLevel::Process => "Process",
        SandboxLevel::Container => "Container",
        SandboxLevel::VM => "VM",
        SandboxLevel::Remote => "Remote",
    }
}

pub(crate) fn sandbox_level_from_db(value: &str) -> SandboxLevel {
    match value {
        "None" | "none" => SandboxLevel::None,
        "Process" | "process" => SandboxLevel::Process,
        "Container" | "container" => SandboxLevel::Container,
        "VM" | "vm" | "v_m" => SandboxLevel::VM,
        "Remote" | "remote" => SandboxLevel::Remote,
        _ => SandboxLevel::None,
    }
}

pub(crate) fn executor_status_to_db(value: ExecutorStatus) -> &'static str {
    match value {
        ExecutorStatus::Draft => "Draft",
        ExecutorStatus::Registered => "Registered",
        ExecutorStatus::Disabled => "Disabled",
    }
}

pub(crate) fn executor_status_from_db(value: &str) -> ExecutorStatus {
    match value {
        "Draft" | "draft" => ExecutorStatus::Draft,
        "Registered" | "registered" => ExecutorStatus::Registered,
        "Disabled" | "disabled" => ExecutorStatus::Disabled,
        _ => ExecutorStatus::Draft,
    }
}

pub(crate) fn skill_status_to_db(value: SkillStatus) -> &'static str {
    match value {
        SkillStatus::Draft => "Draft",
        SkillStatus::Proposed => "Proposed",
        SkillStatus::Verified => "Verified",
        SkillStatus::Approved => "Approved",
        SkillStatus::Active => "Active",
        SkillStatus::Deprecated => "Deprecated",
        SkillStatus::Revoked => "Revoked",
    }
}

pub(crate) fn skill_status_from_db(value: &str) -> SkillStatus {
    match value {
        "Draft" | "draft" => SkillStatus::Draft,
        "Proposed" | "proposed" => SkillStatus::Proposed,
        "Verified" | "verified" => SkillStatus::Verified,
        "Approved" | "approved" => SkillStatus::Approved,
        "Active" | "active" => SkillStatus::Active,
        "Deprecated" | "deprecated" => SkillStatus::Deprecated,
        "Revoked" | "revoked" => SkillStatus::Revoked,
        _ => SkillStatus::Draft,
    }
}
