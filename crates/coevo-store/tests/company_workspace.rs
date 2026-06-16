use coevo_store::company_workspace::CompanyWorkspaceManager;
use coevo_store::pool::create_pool;
use sqlx::Row;

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("coevo-{name}-{}", uuid::Uuid::new_v4()))
}

#[tokio::test]
async fn creates_two_companies_with_distinct_dirs_indexes_and_databases() {
    let root = unique_temp_dir("company-workspace");
    let manager = CompanyWorkspaceManager::new(root.clone());

    let alpha = manager
        .create_company("Alpha Labs", Some("Build alpha"), "founder-1")
        .await
        .unwrap();
    let beta = manager
        .create_company("Beta Works", Some("Build beta"), "founder-1")
        .await
        .unwrap();

    assert_ne!(alpha.opc_id, beta.opc_id);
    assert_ne!(alpha.dir, beta.dir);
    assert!(root.join("companies.json").exists());
    assert!(root.join(".governance").join(".tracks-policy").exists());
    assert!(root
        .join(".governance")
        .join(".leases")
        .join("active")
        .exists());
    assert!(root.join(".models").exists());
    assert!(root.join(&alpha.opc_id).join("company.json").exists());
    assert!(root.join(&alpha.opc_id).join("charter.md").exists());
    assert!(root.join(&alpha.opc_id).join("reports").exists());
    assert!(root.join(&alpha.opc_id).join(".meetings").exists());
    assert!(root
        .join(&alpha.opc_id)
        .join(".governance")
        .join(".audit")
        .exists());
    assert!(root
        .join(&alpha.opc_id)
        .join(".governance")
        .join(".tracks")
        .join("green")
        .exists());
    assert!(root.join(&beta.opc_id).join("company.json").exists());
    assert!(root.join(&beta.opc_id).join("charter.md").exists());

    let listed = manager.list_companies().await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|company| company.opc_id == alpha.opc_id));
    assert!(listed.iter().any(|company| company.opc_id == beta.opc_id));

    let alpha_pool = create_pool(&manager.company_db_path(&alpha.opc_id).to_string_lossy())
        .await
        .unwrap();
    let beta_pool = create_pool(&manager.company_db_path(&beta.opc_id).to_string_lossy())
        .await
        .unwrap();

    sqlx::query("CREATE TABLE IF NOT EXISTS marker (value TEXT NOT NULL)")
        .execute(&alpha_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO marker (value) VALUES ('alpha-only')")
        .execute(&alpha_pool)
        .await
        .unwrap();

    sqlx::query("CREATE TABLE IF NOT EXISTS marker (value TEXT NOT NULL)")
        .execute(&beta_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO marker (value) VALUES ('beta-only')")
        .execute(&beta_pool)
        .await
        .unwrap();

    let alpha_value = sqlx::query("SELECT value FROM marker")
        .fetch_one(&alpha_pool)
        .await
        .unwrap()
        .get::<String, _>("value");
    let beta_value = sqlx::query("SELECT value FROM marker")
        .fetch_one(&beta_pool)
        .await
        .unwrap()
        .get::<String, _>("value");
    assert_eq!(alpha_value, "alpha-only");
    assert_eq!(beta_value, "beta-only");

    alpha_pool.close().await;
    beta_pool.close().await;
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn deleting_one_company_removes_its_directory_and_preserves_other_company() {
    let root = unique_temp_dir("company-delete");
    let manager = CompanyWorkspaceManager::new(root.clone());

    let alpha = manager
        .create_company("Alpha Labs", Some("Build alpha"), "founder-1")
        .await
        .unwrap();
    let beta = manager
        .create_company("Beta Works", Some("Build beta"), "founder-1")
        .await
        .unwrap();

    manager.delete_company(&alpha.opc_id).await.unwrap();

    assert!(!root.join(&alpha.opc_id).exists());
    assert!(root.join(&beta.opc_id).exists());

    let listed = manager.list_companies().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].opc_id, beta.opc_id);

    let beta_pool = create_pool(&manager.company_db_path(&beta.opc_id).to_string_lossy())
        .await
        .unwrap();
    let count = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM sqlite_master")
        .fetch_one(&beta_pool)
        .await
        .unwrap()
        .0;
    assert!(count > 0);

    beta_pool.close().await;
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn exposes_canonical_employee_paths_and_skeleton() {
    let root = unique_temp_dir("company-employee-paths");
    let manager = CompanyWorkspaceManager::new(root.clone());

    let company = manager
        .create_company("Acme Ops", Some("Run the shop"), "founder-1")
        .await
        .unwrap();

    let employee_dir = manager.company_employee_dir(&company.opc_id, "agent-ops-01");
    let passport_path = manager.company_employee_passport_path(&company.opc_id, "agent-ops-01");
    let prompt_path = manager.company_employee_prompt_path(&company.opc_id, "agent-ops-01");
    let prompt_versions_dir =
        manager.company_employee_prompt_versions_dir(&company.opc_id, "agent-ops-01");
    let prompt_version_path =
        manager.company_employee_prompt_version_path(&company.opc_id, "agent-ops-01", 3);
    let current_version_path =
        manager.company_employee_prompt_current_version_path(&company.opc_id, "agent-ops-01");
    let company_skills_dir = manager.company_skills_dir(&company.opc_id);
    let company_skill_path = manager.company_skill_markdown_path(&company.opc_id, "skill-ops");
    let employee_skills_dir = manager.company_employee_skills_dir(&company.opc_id, "agent-ops-01");
    let employee_skill_path = manager.company_employee_skill_markdown_path(
        &company.opc_id,
        "agent-ops-01",
        "skill-agent-ops",
    );

    assert_eq!(
        employee_dir,
        root.join(&company.opc_id)
            .join("employees")
            .join("agent-ops-01")
    );
    assert_eq!(passport_path, employee_dir.join("passport.json"));
    assert_eq!(prompt_path, employee_dir.join("prompt.md"));
    assert_eq!(prompt_versions_dir, employee_dir.join("prompt_versions"));
    assert_eq!(
        prompt_version_path,
        employee_dir.join("prompt_versions").join("v3.md")
    );
    assert_eq!(
        current_version_path,
        employee_dir.join("prompt_versions").join("current.txt")
    );
    assert_eq!(
        company_skills_dir,
        root.join(&company.opc_id).join("skills")
    );
    assert_eq!(
        company_skill_path,
        company_skills_dir.join("skill-ops").join("SKILL.md")
    );
    assert_eq!(employee_skills_dir, employee_dir.join("skills"));
    assert_eq!(
        employee_skill_path,
        employee_skills_dir.join("skill-agent-ops").join("SKILL.md")
    );

    manager
        .ensure_company_employee_skeleton(&company.opc_id, "agent-ops-01")
        .unwrap();

    assert!(employee_dir.exists());
    assert!(prompt_versions_dir.exists());
    assert!(employee_skills_dir.exists());

    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn writes_and_reads_employee_file_contents_and_prompt_versions() {
    let root = unique_temp_dir("company-employee-file-io");
    let manager = CompanyWorkspaceManager::new(root.clone());

    let company = manager
        .create_company("Gamma Studio", Some("Support persona file IO"), "founder-1")
        .await
        .unwrap();

    let agent_id = "agent-writer-01";
    manager
        .write_company_employee_files(
            &company.opc_id,
            agent_id,
            &coevo_store::company_workspace::CompanyEmployeeFiles {
                passport_json: serde_json::json!({
                    "agent_id": agent_id,
                    "role": "writer"
                }),
                prompt_md: "base prompt body".to_string(),
                identity_md: "identity body".to_string(),
                soul_md: "soul body".to_string(),
                agents_md: "agents body".to_string(),
                owner_md: "owner body".to_string(),
                tools_md: "tools body".to_string(),
                tool_policy_json: serde_json::json!({
                    "allowed_tools": ["file-readonly"]
                }),
            },
        )
        .unwrap();
    manager
        .write_company_employee_prompt_version(&company.opc_id, agent_id, 1, "published v1", true)
        .unwrap();
    manager
        .write_company_employee_prompt_version(&company.opc_id, agent_id, 2, "draft v2", false)
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(manager.company_employee_prompt_path(&company.opc_id, agent_id))
            .unwrap(),
        "published v1"
    );
    assert_eq!(
        std::fs::read_to_string(manager.company_employee_prompt_version_path(
            &company.opc_id,
            agent_id,
            1
        ))
        .unwrap(),
        "published v1"
    );
    assert_eq!(
        std::fs::read_to_string(manager.company_employee_prompt_version_path(
            &company.opc_id,
            agent_id,
            2
        ))
        .unwrap(),
        "draft v2"
    );
    assert_eq!(
        manager
            .read_company_employee_current_prompt_version(&company.opc_id, agent_id)
            .unwrap(),
        Some(1)
    );

    let files = manager
        .read_company_employee_files(&company.opc_id, agent_id)
        .unwrap();
    assert_eq!(files.passport_json["agent_id"], agent_id);
    assert_eq!(files.prompt_md, "published v1");
    assert_eq!(files.identity_md, "identity body");
    assert_eq!(files.soul_md, "soul body");
    assert_eq!(files.agents_md, "agents body");
    assert_eq!(files.owner_md, "owner body");
    assert_eq!(files.tools_md, "tools body");
    assert_eq!(files.tool_policy_json["allowed_tools"][0], "file-readonly");

    std::fs::remove_dir_all(root).ok();
}
