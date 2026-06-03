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
    assert!(root.join(".governance").join(".leases").join("active").exists());
    assert!(root.join(".models").exists());
    assert!(root.join(&alpha.opc_id).join("company.json").exists());
    assert!(root.join(&alpha.opc_id).join("charter.md").exists());
    assert!(root.join(&alpha.opc_id).join("reports").exists());
    assert!(root.join(&alpha.opc_id).join(".meetings").exists());
    assert!(root.join(&alpha.opc_id).join(".governance").join(".audit").exists());
    assert!(root.join(&alpha.opc_id).join(".governance").join(".tracks").join("green").exists());
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
