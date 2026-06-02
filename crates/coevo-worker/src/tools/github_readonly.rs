use crate::error::WorkerError;
use async_trait::async_trait;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError>;
    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError>;
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError>;
    async fn cancel(&self, _run_id: &str) -> Result<(), WorkerError> {
        Ok(())
    }
}

pub struct GitHubReadonlyTool;

#[async_trait]
impl ToolHandler for GitHubReadonlyTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        let client = reqwest::Client::new();
        let resp = client
            .head("https://github.com")
            .send()
            .await
            .map_err(|e| WorkerError::GitHubReadFailed(e.to_string()))?;
        Ok(
            serde_json::json!({"online":resp.status().is_success(),"status_code":resp.status().as_u16()}),
        )
    }
    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let action = input["action"].as_str().unwrap_or("ReadRepositoryMetadata");
        if action.contains("Write") || action.contains("Delete") {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        Ok(serde_json::json!({"dry_run":true,"action":action}))
    }
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let repo = input["repo_url"]
            .as_str()
            .unwrap_or("")
            .replace("https://github.com/", "");
        let action = input["action"].as_str().unwrap_or("ReadReadme");
        let max_bytes = input["max_bytes"].as_u64().unwrap_or(200_000) as usize;
        if action.contains("Write") || action.contains("Delete") || action.contains("Push") {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        let client = reqwest::Client::builder()
            .user_agent("coevo-worker/1.0")
            .build()
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        match action {
            "ReadReadme" => {
                let urls = vec![
                    format!("https://raw.githubusercontent.com/{}/main/README.md", repo),
                    format!(
                        "https://raw.githubusercontent.com/{}/main/README.zh-CN.md",
                        repo
                    ),
                ];
                let mut content = String::new();
                let mut truncated = false;
                for url in urls {
                    match client.get(&url).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            content = resp.text().await.unwrap_or_default();
                            if content.len() > max_bytes {
                                content.truncate(max_bytes);
                                truncated = true;
                            }
                            break;
                        }
                        _ => continue,
                    }
                }
                if content.is_empty() {
                    return Err(WorkerError::GitHubReadFailed("README not found".into()));
                }
                Ok(
                    serde_json::json!({"repo":repo,"action":"ReadReadme","content":content,"truncated":truncated,"bytes_read":content.len()}),
                )
            }
            "ListRecentCommits" => {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() < 2 {
                    return Err(WorkerError::GitHubReadFailed("invalid repo format".into()));
                }
                let url = format!(
                    "https://api.github.com/repos/{}/{}/commits?per_page=5",
                    parts[0], parts[1]
                );
                let resp = client
                    .get(&url)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .send()
                    .await
                    .map_err(|e| WorkerError::GitHubReadFailed(e.to_string()))?;
                let commits: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| WorkerError::GitHubReadFailed(e.to_string()))?;
                let summaries: Vec<String> = commits
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .take(3)
                    .map(|c| {
                        format!(
                            "{} — {}",
                            c["sha"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .take(7)
                                .collect::<String>(),
                            c["commit"]["message"]
                                .as_str()
                                .unwrap_or("")
                                .lines()
                                .next()
                                .unwrap_or("")
                        )
                    })
                    .collect();
                Ok(
                    serde_json::json!({"repo":repo,"action":"ListRecentCommits","content":summaries,"truncated":false,"bytes_read":summaries.len()}),
                )
            }
            _ => {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() < 2 {
                    return Err(WorkerError::GitHubReadFailed("invalid repo format".into()));
                }
                let url = format!("https://api.github.com/repos/{}/{}", parts[0], parts[1]);
                let resp = client
                    .get(&url)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .send()
                    .await
                    .map_err(|e| WorkerError::GitHubReadFailed(e.to_string()))?;
                let json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| WorkerError::GitHubReadFailed(e.to_string()))?;
                let summary = serde_json::json!({"name":json["full_name"],"description":json["description"],"stars":json["stargazers_count"],"language":json["language"],"topics":json["topics"]});
                Ok(
                    serde_json::json!({"repo":repo,"action":"ReadRepositoryMetadata","content":summary,"truncated":false,"bytes_read":serde_json::to_string(&summary).unwrap_or_default().len()}),
                )
            }
        }
    }
}
