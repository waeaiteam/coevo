use crate::migrate::run_migrations;
use crate::pool::create_pool;
use coevo_core::skills::AgentSkillPackage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::{thread, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyIndexEntry {
    pub opc_id: String,
    pub name: String,
    pub dir: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyIdentity {
    pub opc_id: String,
    pub founder_user_id: String,
    pub name: String,
    pub mission: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyEmployeeFiles {
    pub passport_json: Value,
    pub prompt_md: String,
    pub identity_md: String,
    pub soul_md: String,
    pub agents_md: String,
    pub owner_md: String,
    pub tools_md: String,
    pub tool_policy_json: Value,
}

pub struct CompanyWorkspaceManager {
    root: PathBuf,
}

impl CompanyWorkspaceManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn companies_index_path(&self) -> PathBuf {
        self.root.join("companies.json")
    }

    pub fn company_dir(&self, opc_id: &str) -> PathBuf {
        self.root.join(opc_id)
    }

    pub fn company_db_path(&self, opc_id: &str) -> PathBuf {
        self.company_dir(opc_id).join("data.db")
    }

    pub fn company_employees_dir(&self, opc_id: &str) -> PathBuf {
        self.company_dir(opc_id).join("employees")
    }

    pub fn company_employee_dir(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employees_dir(opc_id).join(agent_id)
    }

    pub fn company_employee_passport_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("passport.json")
    }

    pub fn company_employee_prompt_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("prompt.md")
    }

    pub fn company_employee_identity_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("identity.md")
    }

    pub fn company_employee_soul_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id).join("soul.md")
    }

    pub fn company_employee_agents_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("agents.md")
    }

    pub fn company_employee_owner_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id).join("owner.md")
    }

    pub fn company_employee_tools_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id).join("tools.md")
    }

    pub fn company_employee_tool_policy_path(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("tool_policy.json")
    }

    pub fn company_employee_prompt_versions_dir(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id)
            .join("prompt_versions")
    }

    pub fn company_employee_prompt_version_path(
        &self,
        opc_id: &str,
        agent_id: &str,
        version: i32,
    ) -> PathBuf {
        self.company_employee_prompt_versions_dir(opc_id, agent_id)
            .join(format!("v{version}.md"))
    }

    pub fn company_employee_prompt_current_version_path(
        &self,
        opc_id: &str,
        agent_id: &str,
    ) -> PathBuf {
        self.company_employee_prompt_versions_dir(opc_id, agent_id)
            .join("current.txt")
    }

    pub fn company_skills_dir(&self, opc_id: &str) -> PathBuf {
        self.company_dir(opc_id).join("skills")
    }

    pub fn company_skill_dir(&self, opc_id: &str, skill_id: &str) -> PathBuf {
        self.company_skills_dir(opc_id).join(skill_id)
    }

    pub fn company_skill_markdown_path(&self, opc_id: &str, skill_id: &str) -> PathBuf {
        self.company_skill_dir(opc_id, skill_id).join("SKILL.md")
    }

    pub fn company_employee_skills_dir(&self, opc_id: &str, agent_id: &str) -> PathBuf {
        self.company_employee_dir(opc_id, agent_id).join("skills")
    }

    pub fn company_employee_skill_dir(
        &self,
        opc_id: &str,
        agent_id: &str,
        skill_id: &str,
    ) -> PathBuf {
        self.company_employee_skills_dir(opc_id, agent_id)
            .join(skill_id)
    }

    pub fn company_employee_skill_markdown_path(
        &self,
        opc_id: &str,
        agent_id: &str,
        skill_id: &str,
    ) -> PathBuf {
        self.company_employee_skill_dir(opc_id, agent_id, skill_id)
            .join("SKILL.md")
    }

    pub fn render_skill_markdown(skill: &AgentSkillPackage) -> String {
        let mut lines = vec![
            "---".to_string(),
            format!("name: {}", skill.name),
            format!("skill_id: {}", skill.skill_id),
            format!("version: {}", skill.version),
            format!("owner_agent_id: {}", skill.owner_agent_id),
            format!("department: {}", skill.department),
            format!("risk_ceiling: {}", skill.risk_ceiling),
            "---".to_string(),
            String::new(),
        ];
        if !skill.description.trim().is_empty() {
            lines.push(skill.description.trim().to_string());
            lines.push(String::new());
        }
        if !skill.prompt_template.trim().is_empty() {
            lines.push(skill.prompt_template.trim().to_string());
            lines.push(String::new());
        }
        if !skill.procedure_steps.is_empty() {
            lines.push("## Procedure".to_string());
            for step in &skill.procedure_steps {
                lines.push(format!("- {}", step));
            }
            lines.push(String::new());
        }
        if !skill.guardrails.is_empty() {
            lines.push("## Guardrails".to_string());
            for guardrail in &skill.guardrails {
                lines.push(format!("- {}", guardrail));
            }
            lines.push(String::new());
        }
        lines.join("\n").trim_end().to_string() + "\n"
    }

    pub fn write_company_skill_markdown(
        &self,
        opc_id: &str,
        skill: &AgentSkillPackage,
        agent_id: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let skill_path = if let Some(agent_id) = agent_id {
            self.company_employee_skill_markdown_path(opc_id, agent_id, &skill.skill_id)
        } else {
            self.company_skill_markdown_path(opc_id, &skill.skill_id)
        };
        if let Some(parent) = skill_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(skill_path, Self::render_skill_markdown(skill))
    }

    pub fn ensure_company_employee_skeleton(
        &self,
        opc_id: &str,
        agent_id: &str,
    ) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(self.company_employee_dir(opc_id, agent_id))?;
        std::fs::create_dir_all(self.company_employee_prompt_versions_dir(opc_id, agent_id))?;
        std::fs::create_dir_all(self.company_employee_skills_dir(opc_id, agent_id))?;
        Ok(())
    }

    pub fn write_company_employee_files(
        &self,
        opc_id: &str,
        agent_id: &str,
        files: &CompanyEmployeeFiles,
    ) -> Result<(), std::io::Error> {
        self.ensure_company_employee_skeleton(opc_id, agent_id)?;
        std::fs::write(
            self.company_employee_passport_path(opc_id, agent_id),
            serde_json::to_string_pretty(&files.passport_json).map_err(std::io::Error::other)?,
        )?;
        std::fs::write(
            self.company_employee_prompt_path(opc_id, agent_id),
            &files.prompt_md,
        )?;
        std::fs::write(
            self.company_employee_identity_path(opc_id, agent_id),
            &files.identity_md,
        )?;
        std::fs::write(
            self.company_employee_soul_path(opc_id, agent_id),
            &files.soul_md,
        )?;
        std::fs::write(
            self.company_employee_agents_path(opc_id, agent_id),
            &files.agents_md,
        )?;
        std::fs::write(
            self.company_employee_owner_path(opc_id, agent_id),
            &files.owner_md,
        )?;
        std::fs::write(
            self.company_employee_tools_path(opc_id, agent_id),
            &files.tools_md,
        )?;
        std::fs::write(
            self.company_employee_tool_policy_path(opc_id, agent_id),
            serde_json::to_string_pretty(&files.tool_policy_json).map_err(std::io::Error::other)?,
        )?;
        Ok(())
    }

    pub fn read_company_employee_files(
        &self,
        opc_id: &str,
        agent_id: &str,
    ) -> Result<CompanyEmployeeFiles, std::io::Error> {
        Ok(CompanyEmployeeFiles {
            passport_json: read_json_or_default(
                &self.company_employee_passport_path(opc_id, agent_id),
                Value::Null,
            )?,
            prompt_md: read_string_or_default(
                &self.company_employee_prompt_path(opc_id, agent_id),
            )?,
            identity_md: read_string_or_default(
                &self.company_employee_identity_path(opc_id, agent_id),
            )?,
            soul_md: read_string_or_default(&self.company_employee_soul_path(opc_id, agent_id))?,
            agents_md: read_string_or_default(
                &self.company_employee_agents_path(opc_id, agent_id),
            )?,
            owner_md: read_string_or_default(&self.company_employee_owner_path(opc_id, agent_id))?,
            tools_md: read_string_or_default(&self.company_employee_tools_path(opc_id, agent_id))?,
            tool_policy_json: read_json_or_default(
                &self.company_employee_tool_policy_path(opc_id, agent_id),
                Value::Null,
            )?,
        })
    }

    pub fn write_company_employee_prompt_version(
        &self,
        opc_id: &str,
        agent_id: &str,
        version: i32,
        content: &str,
        publish: bool,
    ) -> Result<(), std::io::Error> {
        self.ensure_company_employee_skeleton(opc_id, agent_id)?;
        std::fs::write(
            self.company_employee_prompt_version_path(opc_id, agent_id, version),
            content,
        )?;
        if publish {
            std::fs::write(self.company_employee_prompt_path(opc_id, agent_id), content)?;
            std::fs::write(
                self.company_employee_prompt_current_version_path(opc_id, agent_id),
                version.to_string(),
            )?;
        }
        Ok(())
    }

    pub fn read_company_employee_current_prompt_version(
        &self,
        opc_id: &str,
        agent_id: &str,
    ) -> Result<Option<i32>, std::io::Error> {
        let path = self.company_employee_prompt_current_version_path(opc_id, agent_id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(path)?;
        raw.trim()
            .parse::<i32>()
            .map(Some)
            .map_err(std::io::Error::other)
    }

    pub async fn list_companies(&self) -> Result<Vec<CompanyIndexEntry>, std::io::Error> {
        self.ensure_root_dirs()?;
        self.read_index()
    }

    pub async fn create_company(
        &self,
        name: &str,
        mission: Option<&str>,
        founder_user_id: &str,
    ) -> Result<CompanyIndexEntry, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_root_dirs()?;

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let opc_id = format!("opc-{}", uuid::Uuid::new_v4().simple());
        let company_dir = self.company_dir(&opc_id);
        std::fs::create_dir_all(&company_dir)?;

        for subdir in [
            "sop",
            "shared",
            "memory",
            "skills",
            "goals",
            "reports",
            ".workorders/planned",
            ".workorders/running",
            ".workorders/waiting",
            ".workorders/completed",
            ".workorders/failed",
            ".conversations",
            ".meetings",
            ".governance",
            ".governance/.mcl",
            ".governance/.pcdt",
            ".governance/.risk",
            ".governance/.adr",
            ".governance/.tracks/green",
            ".governance/.tracks/yellow",
            ".governance/.tracks/red",
            ".governance/.resolution",
            ".governance/.audit",
            "employees",
        ] {
            std::fs::create_dir_all(company_dir.join(subdir))?;
        }

        let identity = CompanyIdentity {
            opc_id: opc_id.clone(),
            founder_user_id: founder_user_id.to_string(),
            name: name.to_string(),
            mission: mission.unwrap_or_default().to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        std::fs::write(
            company_dir.join("company.json"),
            serde_json::to_string_pretty(&identity)?,
        )?;
        std::fs::write(
            company_dir.join("charter.md"),
            format!("# {}\n\n{}", name, mission.unwrap_or_default()),
        )?;

        let db_path = self.company_db_path(&opc_id);
        let pool = create_pool(&db_path.to_string_lossy()).await?;
        run_migrations(&pool).await?;
        pool.close().await;

        let mut companies = self.read_index()?;
        let entry = CompanyIndexEntry {
            opc_id,
            name: name.to_string(),
            dir: company_dir.to_string_lossy().to_string(),
            created_at_ms: now,
        };
        companies.push(entry.clone());
        self.write_index(&companies)?;
        Ok(entry)
    }

    pub async fn delete_company(
        &self,
        opc_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_root_dirs()?;
        let company_dir = self.company_dir(opc_id);
        if company_dir.exists() {
            remove_dir_all_with_windows_retry(&company_dir)?;
        }

        let mut companies = self.read_index()?;
        companies.retain(|company| company.opc_id != opc_id);
        self.write_index(&companies)?;
        Ok(())
    }

    fn ensure_root_dirs(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.root)?;
        for subdir in [
            ".governance/.tracks-policy",
            ".governance/.risk-policy",
            ".governance/.adr-templates",
            ".governance/.lease-policy",
            ".governance/.leases/active",
            ".governance/.leases/revoked",
            ".models",
            ".policy",
        ] {
            std::fs::create_dir_all(self.root.join(subdir))?;
        }
        let index = self.companies_index_path();
        if !index.exists() {
            std::fs::write(index, "[]")?;
        }
        Ok(())
    }

    fn read_index(&self) -> Result<Vec<CompanyIndexEntry>, std::io::Error> {
        let path = self.companies_index_path();
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(std::io::Error::other)
    }

    fn write_index(&self, companies: &[CompanyIndexEntry]) -> Result<(), std::io::Error> {
        std::fs::write(
            self.companies_index_path(),
            serde_json::to_string_pretty(companies).map_err(std::io::Error::other)?,
        )
    }
}

fn remove_dir_all_with_windows_retry(
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error: Option<std::io::Error> = None;
    for _ in 0..10 {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if is_transient_windows_dir_lock(&error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(Box::new(error)),
        }
    }

    Err(Box::new(last_error.unwrap_or_else(|| {
        std::io::Error::other("transient directory lock did not clear")
    })))
}

fn is_transient_windows_dir_lock(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        use std::io::ErrorKind;
        matches!(
            error.kind(),
            ErrorKind::PermissionDenied | ErrorKind::WouldBlock
        ) || matches!(error.raw_os_error(), Some(32 | 33 | 5))
    }

    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}

fn read_string_or_default(path: &Path) -> Result<String, std::io::Error> {
    if path.exists() {
        std::fs::read_to_string(path)
    } else {
        Ok(String::new())
    }
}

fn read_json_or_default(path: &Path, default: Value) -> Result<Value, std::io::Error> {
    if !path.exists() {
        return Ok(default);
    }
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(std::io::Error::other)
}
