use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::{
    migrate::run_migrations,
    pool::create_pool,
    repos_opc::{agent_employee_repo, work_order_repo::WorkOrderRepo},
};
use serde::Deserialize;
use sqlx::Row;

use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Deserialize)]
pub struct CreateMeetingRequest {
    pub topic: String,
    pub participants: Vec<String>,
    pub close_mode: String,
}

#[derive(Deserialize)]
pub struct CreateKpiRequest {
    pub work_order_id: String,
    pub scores: serde_json::Value,
    pub reviewer: String,
    pub comment: Option<String>,
}

#[derive(Deserialize)]
pub struct GenerateReportRequest {
    pub period: String,
}

#[derive(Deserialize)]
pub struct UpdateCostQuotaRequest {
    pub department: String,
    pub token_quota: i64,
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    let company_dir = state.company_workspace.company_dir(opc_id);
    if !company_dir.exists() {
        return Err(err!(StatusCode::NOT_FOUND, "company not found"));
    }
    let pool = create_pool(
        &state
            .company_workspace
            .company_db_path(opc_id)
            .to_string_lossy(),
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    run_migrations(&pool)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(pool)
}

fn meeting_dir(state: &AppState, opc_id: &str, meeting_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join(".meetings")
        .join(meeting_id)
}

fn report_path(state: &AppState, opc_id: &str, report_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("reports")
        .join(format!("{report_id}.md"))
}

fn relative_company_path(state: &AppState, opc_id: &str, path: &std::path::Path) -> String {
    path.strip_prefix(state.company_workspace.company_dir(opc_id))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn seeded_meeting_transcript(participants: &[String]) -> Vec<serde_json::Value> {
    let mut transcript = Vec::new();
    for agent_id in participants {
        let stance = if agent_id == "agent-critic-01" || agent_id == "agent-risk-01" {
            "oppose"
        } else {
            "support"
        };
        let text = if stance == "oppose" {
            "Raise a concrete dissenting risk or quality concern before adopting the plan."
        } else {
            "Support the proposal and summarize the execution benefit."
        };
        transcript.push(serde_json::json!({
            "agent_id": agent_id,
            "stance": stance,
            "text": text
        }));
    }
    transcript
}

async fn ensure_company_employee_exists(
    pool: &sqlx::SqlitePool,
    agent_id: &str,
) -> Result<(), String> {
    match agent_employee_repo::AgentEmployeeRepo::exists(pool, agent_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("Employee not found: {agent_id}")),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn create_meeting(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<CreateMeetingRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.topic.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "topic is required");
    }
    if req.participants.is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "participants are required");
    }
    if req.close_mode != "vote" && req.close_mode != "chair" {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "close_mode must be vote or chair");
    }
    if !req.participants.iter().any(|id| id == "agent-critic-01" || id == "agent-risk-01") {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "meetings must include agent-critic-01 or agent-risk-01 as a dissenting participant"
        );
    }

    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    for participant in &req.participants {
        if let Err(e) = ensure_company_employee_exists(&pool, participant).await {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, e);
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let meeting_id = format!("meeting-{}", uuid::Uuid::new_v4().simple());
    let archive_dir = meeting_dir(&s, &opc_id, &meeting_id);
    if let Err(e) = std::fs::create_dir_all(&archive_dir) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let transcript = seeded_meeting_transcript(&req.participants);
    let resolution_md = format!(
        "# Opinion Letter\n\nTopic: {}\n\nMode: {}\n\nResolution: Proceed with the discussion outcome and keep the dissent on record.\n",
        req.topic, req.close_mode
    );
    let agenda = serde_json::json!([req.topic.clone()]);
    let participants_json = serde_json::to_string(&req.participants).unwrap_or_else(|_| "[]".to_string());
    let transcript_json = serde_json::to_string(&transcript).unwrap_or_else(|_| "[]".to_string());
    let agenda_json = agenda.to_string();
    let archive_relpath = relative_company_path(&s, &opc_id, &archive_dir);
    if let Err(e) = std::fs::write(
        archive_dir.join("agenda.json"),
        serde_json::json!({
            "topic": req.topic,
            "participants": req.participants,
            "close_mode": req.close_mode,
        })
        .to_string(),
    ) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    if let Err(e) = std::fs::write(archive_dir.join("resolution.md"), &resolution_md) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let insert = sqlx::query(
        "INSERT INTO meetings (
            meeting_id, topic, status, close_mode, agenda_json, participants_json, transcript_json,
            resolution_md, responsibility_anchor, archive_relpath, created_at_ms, updated_at_ms
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&meeting_id)
    .bind(&req.topic)
    .bind("running")
    .bind(&req.close_mode)
    .bind(&agenda_json)
    .bind(&participants_json)
    .bind(&transcript_json)
    .bind(&resolution_md)
    .bind("department-supervisor")
    .bind(&archive_relpath)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await;
    pool.close().await;

    match insert {
        Ok(_) => ok!(serde_json::json!({
            "meeting_id": meeting_id,
            "status": "running"
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_meetings(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT meeting_id, topic, status, close_mode, created_at_ms
         FROM meetings
         ORDER BY created_at_ms DESC",
    )
    .fetch_all(&pool)
    .await;
    pool.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    serde_json::json!({
                        "meeting_id": row.get::<String, _>("meeting_id"),
                        "topic": row.get::<String, _>("topic"),
                        "status": row.get::<String, _>("status"),
                        "close_mode": row.get::<String, _>("close_mode"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_meeting(
    State(s): State<AppState>,
    Path((opc_id, meeting_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let row = sqlx::query("SELECT * FROM meetings WHERE meeting_id = ?")
        .bind(&meeting_id)
        .fetch_optional(&pool)
        .await;
    pool.close().await;
    match row {
        Ok(Some(row)) => {
            let agenda = serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("agenda_json"))
                .unwrap_or_else(|_| serde_json::json!([]));
            let transcript = serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("transcript_json"))
                .unwrap_or_else(|_| serde_json::json!([]));
            ok!(serde_json::json!({
                "meeting_id": row.get::<String, _>("meeting_id"),
                "topic": row.get::<String, _>("topic"),
                "status": row.get::<String, _>("status"),
                "agenda": agenda,
                "transcript": transcript,
                "resolution_md": row.get::<String, _>("resolution_md"),
                "responsibility_anchor": row.get::<String, _>("responsibility_anchor"),
                "created_at_ms": row.get::<i64, _>("created_at_ms"),
            }))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Meeting not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_employee_kpi(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT kpi_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
         FROM kpi_records
         WHERE agent_id = ?
         ORDER BY created_at_ms DESC",
    )
    .bind(&agent_id)
    .fetch_all(&pool)
    .await;
    pool.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    let scores = serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("scores_json"))
                        .unwrap_or_else(|_| serde_json::json!({}));
                    serde_json::json!({
                        "kpi_id": row.get::<String, _>("kpi_id"),
                        "work_order_id": row.get::<String, _>("work_order_id"),
                        "reviewer": row.get::<String, _>("reviewer_agent_id"),
                        "scores": scores,
                        "comment": row.get::<String, _>("comment"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_employee_kpi(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
    Json(req): Json<CreateKpiRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    if let Err(e) = ensure_company_employee_exists(&company_db, &agent_id).await {
        company_db.close().await;
        return err!(StatusCode::NOT_FOUND, e);
    }
    if let Err(e) = ensure_company_employee_exists(&company_db, &req.reviewer).await {
        company_db.close().await;
        return err!(StatusCode::NOT_FOUND, e);
    }
    match WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
        Ok(Some(work_order)) if work_order.opc_id == opc_id => {}
        Ok(Some(_)) => {
            company_db.close().await;
            return err!(StatusCode::FORBIDDEN, "work order belongs to a different company");
        }
        Ok(None) => {
            company_db.close().await;
            return err!(StatusCode::NOT_FOUND, "WorkOrder not found");
        }
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }

    let kpi_id = format!("kpi-{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().timestamp_millis();
    let insert = sqlx::query(
        "INSERT INTO kpi_records (
            kpi_id, agent_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
        ) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&kpi_id)
    .bind(&agent_id)
    .bind(&req.work_order_id)
    .bind(&req.reviewer)
    .bind(req.scores.to_string())
    .bind(req.comment.clone().unwrap_or_default())
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match insert {
        Ok(_) => ok!(serde_json::json!({
            "kpi_id": kpi_id,
            "work_order_id": req.work_order_id,
            "reviewer": req.reviewer,
            "scores": req.scores,
            "comment": req.comment.unwrap_or_default(),
            "created_at_ms": now,
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_reports(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT report_id, period, report_md_path, created_at_ms
         FROM generated_reports
         ORDER BY created_at_ms DESC",
    )
    .fetch_all(&company_db)
    .await;
    company_db.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    serde_json::json!({
                        "report_id": row.get::<String, _>("report_id"),
                        "period": row.get::<String, _>("period"),
                        "path": row.get::<String, _>("report_md_path"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_report(
    State(s): State<AppState>,
    Path((opc_id, report_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let row = sqlx::query(
        "SELECT report_id, period, report_md_path, kpi_summary_json, token_usage_json, alerts_json
         FROM generated_reports
         WHERE report_id = ?",
    )
    .bind(&report_id)
    .fetch_optional(&company_db)
    .await;
    company_db.close().await;
    match row {
        Ok(Some(row)) => {
            let absolute_path = s.company_workspace.company_dir(&opc_id).join(row.get::<String, _>("report_md_path"));
            let report_md = std::fs::read_to_string(absolute_path).unwrap_or_default();
            ok!(serde_json::json!({
                "report_md": report_md,
                "period": row.get::<String, _>("period"),
                "kpi_summary": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("kpi_summary_json")).unwrap_or_else(|_| serde_json::json!([])),
                "token_usage": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("token_usage_json")).unwrap_or_else(|_| serde_json::json!({})),
                "alerts": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("alerts_json")).unwrap_or_else(|_| serde_json::json!([])),
            }))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Report not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn generate_report(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<GenerateReportRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.period != "daily" && req.period != "monthly" {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "period must be daily or monthly");
    }
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let kpi_rows = match sqlx::query(
        "SELECT agent_id, scores_json, created_at_ms
         FROM kpi_records
         ORDER BY created_at_ms DESC
         LIMIT 20",
    )
    .fetch_all(&company_db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };

    let run_rows = match sqlx::query(
        "SELECT wr.agent_id, ae.department, COALESCE(SUM(wr.total_tokens),0) AS tokens, COALESCE(SUM(wr.total_cost_usd),0) AS cost
         FROM worker_runs wr
         JOIN work_orders wo ON wo.work_order_id = wr.work_order_id
         LEFT JOIN agent_employees ae ON ae.agent_id = wr.agent_id
         WHERE wo.opc_id = ?
         GROUP BY wr.agent_id, ae.department",
    )
    .bind(&opc_id)
    .fetch_all(&s.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };

    let kpi_summary: Vec<serde_json::Value> = kpi_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "agent_id": row.get::<String, _>("agent_id"),
                "scores": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("scores_json")).unwrap_or_else(|_| serde_json::json!({})),
                "created_at_ms": row.get::<i64, _>("created_at_ms"),
            })
        })
        .collect();
    let token_usage = serde_json::json!({
        "by_agent": run_rows.iter().map(|row| serde_json::json!({
            "agent_id": row.get::<String, _>("agent_id"),
            "department": row.try_get::<Option<String>, _>("department").ok().flatten().unwrap_or_else(|| "Unknown".to_string()),
            "tokens": row.get::<i64, _>("tokens"),
            "cost_usd": row.get::<f64, _>("cost"),
        })).collect::<Vec<_>>()
    });
    let alerts: Vec<serde_json::Value> = run_rows
        .iter()
        .filter(|row| row.get::<f64, _>("cost") > 0.0)
        .map(|row| {
            serde_json::json!({
                "level": "info",
                "message": format!(
                    "{} consumed ${:.4}",
                    row.get::<String, _>("agent_id"),
                    row.get::<f64, _>("cost")
                )
            })
        })
        .collect();

    let report_id = format!("report-{}", uuid::Uuid::new_v4().simple());
    let markdown = format!(
        "# {} Report\n\n## KPI\n{}\n\n## Token Usage\n{}\n",
        req.period,
        kpi_summary
            .iter()
            .map(|item| format!("- {}: {}", item["agent_id"].as_str().unwrap_or_default(), item["scores"]))
            .collect::<Vec<_>>()
            .join("\n"),
        token_usage["by_agent"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|item| format!(
                "- {} / {}: {} tokens / ${:.4}",
                item["department"].as_str().unwrap_or("Unknown"),
                item["agent_id"].as_str().unwrap_or_default(),
                item["tokens"].as_i64().unwrap_or_default(),
                item["cost_usd"].as_f64().unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let absolute_report_path = report_path(&s, &opc_id, &report_id);
    if let Err(e) = std::fs::write(&absolute_report_path, &markdown) {
        company_db.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let relative_path = relative_company_path(&s, &opc_id, &absolute_report_path);
    let insert = sqlx::query(
        "INSERT INTO generated_reports (
            report_id, period, report_md_path, kpi_summary_json, token_usage_json, alerts_json, created_at_ms
        ) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&report_id)
    .bind(&req.period)
    .bind(&relative_path)
    .bind(serde_json::to_string(&kpi_summary).unwrap_or_else(|_| "[]".to_string()))
    .bind(token_usage.to_string())
    .bind(serde_json::to_string(&alerts).unwrap_or_else(|_| "[]".to_string()))
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match insert {
        Ok(_) => ok!(serde_json::json!({ "report_id": report_id })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_cost_summary(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let quota_rows = sqlx::query("SELECT department, token_quota FROM department_cost_quotas")
        .fetch_all(&company_db)
        .await;
    let quota_map = match quota_rows {
        Ok(rows) => rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("department"),
                    row.get::<i64, _>("token_quota"),
                )
            })
            .collect::<std::collections::HashMap<_, _>>(),
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    company_db.close().await;

    let rows = match sqlx::query(
        "SELECT COALESCE(ae.department, 'Unknown') AS department,
                COALESCE(SUM(wr.total_tokens),0) AS tokens,
                COALESCE(SUM(wr.total_cost_usd),0) AS cost_usd
         FROM worker_runs wr
         JOIN work_orders wo ON wo.work_order_id = wr.work_order_id
         LEFT JOIN agent_employees ae ON ae.agent_id = wr.agent_id
         WHERE wo.opc_id = ?
         GROUP BY COALESCE(ae.department, 'Unknown')
         ORDER BY department ASC",
    )
    .bind(&opc_id)
    .fetch_all(&s.pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let by_department: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let department = row.get::<String, _>("department");
            serde_json::json!({
                "dept": department,
                "tokens": row.get::<i64, _>("tokens"),
                "cost_usd": row.get::<f64, _>("cost_usd"),
                "quota": quota_map.get(&department).copied(),
            })
        })
        .collect();
    let total_tokens = by_department
        .iter()
        .map(|row| row["tokens"].as_i64().unwrap_or_default())
        .sum::<i64>();
    let total_cost = by_department
        .iter()
        .map(|row| row["cost_usd"].as_f64().unwrap_or_default())
        .sum::<f64>();
    ok!(serde_json::json!({
        "by_department": by_department,
        "total": {
            "tokens": total_tokens,
            "cost_usd": total_cost
        }
    }))
}

pub async fn put_cost_quota(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<UpdateCostQuotaRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.department.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "department is required");
    }
    if req.token_quota < 0 {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "token_quota must be >= 0");
    }
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        "INSERT INTO department_cost_quotas (department, token_quota, updated_at_ms)
         VALUES (?,?,?)
         ON CONFLICT(department)
         DO UPDATE SET token_quota = excluded.token_quota, updated_at_ms = excluded.updated_at_ms",
    )
    .bind(&req.department)
    .bind(req.token_quota)
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match result {
        Ok(_) => ok!(serde_json::json!({"ok": true, "department": req.department, "token_quota": req.token_quota})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::build_router, state::AppState};
    use axum::{body::Body, http::Request};
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::{
        migrate::run_migrations,
        pool::create_test_pool,
        repos::worker_run_repo::WorkerRunRepo,
        repos_opc::work_order_repo::WorkOrderRepo,
    };
    use tower::ServiceExt;

    async fn create_company(app: &axum::Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Org Co","mission":"Stage 5"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        body["opc_id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn company_organization_routes_support_meetings_kpi_reports_and_costs() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-company-org-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let meeting = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/meetings"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "topic": "Ship Stage 5",
                            "participants": ["agent-founder-01", "agent-critic-01", "agent-risk-01"],
                            "close_mode": "vote"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meeting.status(), StatusCode::OK);
        let meeting_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(meeting.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let meeting_id = meeting_body["meeting_id"].as_str().unwrap();
        assert!(root.join(&opc_id).join(".meetings").join(meeting_id).join("resolution.md").exists());

        let meeting_detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/meetings/{meeting_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meeting_detail.status(), StatusCode::OK);

        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &pool,
            &WorkOrder {
                work_order_id: "wo-stage5".to_string(),
                conversation_id: None,
                contract_hash: "a".repeat(64),
                plan_hash: "b".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Stage 5 reporting".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Completed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "stage5".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        WorkerRunRepo::create(
            &pool,
            "run-stage5-a",
            "wo-stage5",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-stage5",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64,
            Some(now as i64 + 10),
        )
        .await
        .unwrap();
        WorkerRunRepo::record_summary(&pool, "run-stage5-a", 10, 20, 30, 0.25, 10)
            .await
            .unwrap();

        let kpi = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/agent-founder-01/kpi"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-stage5",
                            "scores": {"completion": 95, "speed": 88, "clarity": 90},
                            "reviewer": "agent-risk-01",
                            "comment": "solid"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi.status(), StatusCode::OK);

        let kpi_list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/employees/agent-founder-01/kpi"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi_list.status(), StatusCode::OK);

        let quota = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/companies/{opc_id}/cost/quota"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "department": "FounderOffice",
                            "token_quota": 1000
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(quota.status(), StatusCode::OK);

        let report = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/reports/generate"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"period": "daily"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::OK);
        let report_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let report_id = report_body["report_id"].as_str().unwrap();
        assert!(root.join(&opc_id).join("reports").join(format!("{report_id}.md")).exists());

        let cost = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cost.status(), StatusCode::OK);
        let cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(cost_body["total"]["tokens"], 30);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn cost_and_kpi_are_company_scoped() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!("coevo-company-org-scope-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let company_a = create_company(&app).await;
        let company_b = create_company(&app).await;

        for opc_id in [&company_a, &company_b] {
            let seed = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/companies/{opc_id}/employees/seed"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(seed.status(), StatusCode::OK);
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        for (work_order_id, opc_id, run_id, tokens, cost) in [
            ("wo-a", company_a.clone(), "run-a", 40, 0.4),
            ("wo-b", company_b.clone(), "run-b", 70, 0.7),
        ] {
            WorkOrderRepo::create(
                &pool,
                &WorkOrder {
                    work_order_id: work_order_id.to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id,
                    mission_intent: "company scoped stats".to_string(),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "scope".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
            )
            .await
            .unwrap();
            WorkerRunRepo::create(
                &pool,
                run_id,
                work_order_id,
                "agent-founder-01",
                "worker-agent-founder-01",
                &format!("session-{run_id}"),
                "Completed",
                "{}",
                "[]",
                "[]",
                None,
                now as i64,
                Some(now as i64 + 5),
            )
            .await
            .unwrap();
            WorkerRunRepo::record_summary(&pool, run_id, 0, 0, tokens, cost, 5)
                .await
                .unwrap();
        }

        let kpi = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{company_a}/employees/agent-founder-01/kpi"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-a",
                            "scores": {"completion": 91},
                            "reviewer": "agent-risk-01"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi.status(), StatusCode::OK);

        let company_a_cost = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_a}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_a_cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_a_cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_a_cost_body["total"]["tokens"], 40);

        let company_b_cost = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_b}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_b_cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_b_cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_b_cost_body["total"]["tokens"], 70);

        let company_b_kpi = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_b}/employees/agent-founder-01/kpi"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_b_kpi_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_b_kpi.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_b_kpi_body.as_array().unwrap().len(), 0);

        std::fs::remove_dir_all(root).ok();
    }
}
