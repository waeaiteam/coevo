use crate::migrate::run_migrations;
use crate::pool::create_pool;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
            std::fs::remove_dir_all(&company_dir)?;
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
