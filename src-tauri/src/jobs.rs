//! Durable job log (RDS-002, docs/04 "Job lifecycle").
//!
//! Every long-running operation gets a `jobs` row plus `job_events` entries so the
//! app can show what ran, how it ended, and detect work cut short by a crash or
//! forced close. Implemented states: `running` → `completed` | `cancelled` |
//! `failed`; on the next startup any job still `running` becomes `interrupted`.

use crate::db;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

pub const KIND_SCAN: &str = "library_scan";
pub const KIND_DEPLOY: &str = "deployment";

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub input: Value,
    pub progress: Option<Value>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
}

pub fn create(connection: &Connection, kind: &str, input: &Value) -> Result<String, String> {
    let id = db::new_id();
    connection
        .execute(
            "INSERT INTO jobs (id, kind, state, input_json) VALUES (?1, ?2, 'running', ?3)",
            params![id, kind, input.to_string()],
        )
        .map_err(|e| e.to_string())?;
    event(connection, &id, "info", "started", input)?;
    Ok(id)
}

pub fn event(connection: &Connection, job_id: &str, severity: &str, code: &str, detail: &Value) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO job_events (job_id, severity, event_code, detail_json) VALUES (?1, ?2, ?3, ?4)",
            params![job_id, severity, code, detail.to_string()],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Record the final state of a job. `state` is `completed`, `cancelled` or `failed`.
pub fn finish(
    connection: &Connection,
    job_id: &str,
    state: &str,
    progress: Option<&Value>,
    error: Option<(&str, &str)>,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE jobs SET state = ?2, progress_json = ?3, finished_at = CURRENT_TIMESTAMP, error_code = ?4, error_message = ?5
             WHERE id = ?1",
            params![job_id, state, progress.map(Value::to_string), error.map(|e| e.0), error.map(|e| e.1)],
        )
        .map_err(|e| e.to_string())?;
    let severity = match state {
        "completed" => "info",
        "cancelled" => "warning",
        _ => "error",
    };
    let detail = match error {
        Some((code, message)) => serde_json::json!({ "code": code, "message": message }),
        None => progress.cloned().unwrap_or(Value::Null),
    };
    event(connection, job_id, severity, state, &detail)
}

/// Mark jobs left `running` by a previous session as `interrupted`, and scan roots
/// left `running` as `interrupted`. Returns how many jobs were affected.
pub fn recover_interrupted(connection: &Connection) -> Result<usize, String> {
    let ids: Vec<String> = {
        let mut statement = connection
            .prepare("SELECT id FROM jobs WHERE state = 'running'")
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    for id in &ids {
        connection
            .execute(
                "UPDATE jobs SET state = 'interrupted', finished_at = CURRENT_TIMESTAMP, error_code = 'interrupted',
                     error_message = 'The application closed before this job finished.' WHERE id = ?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
        event(connection, id, "warning", "interrupted", &Value::Null)?;
    }
    connection
        .execute("UPDATE indexed_roots SET last_scan_status = 'interrupted' WHERE last_scan_status = 'running'", [])
        .map_err(|e| e.to_string())?;
    Ok(ids.len())
}

pub fn recent(connection: &Connection, limit: u32) -> Result<Vec<JobRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, state, input_json, progress_json, started_at, finished_at, error_message
             FROM jobs ORDER BY started_at DESC, rowid DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([limit], |r| {
            let input: String = r.get(3)?;
            let progress: Option<String> = r.get(4)?;
            Ok(JobRecord {
                id: r.get(0)?,
                kind: r.get(1)?,
                state: r.get(2)?,
                input: serde_json::from_str(&input).unwrap_or(Value::Null),
                progress: progress.and_then(|p| serde_json::from_str(&p).ok()),
                started_at: r.get(5)?,
                finished_at: r.get(6)?,
                error_message: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalog() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        c
    }

    fn events(c: &Connection, job: &str) -> Vec<String> {
        let mut s = c.prepare("SELECT event_code FROM job_events WHERE job_id = ?1 ORDER BY id").unwrap();
        s.query_map([job], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn lifecycle_is_recorded() {
        let c = catalog();
        let id = create(&c, KIND_SCAN, &json!({ "folder": "D:/roms" })).unwrap();
        finish(&c, &id, "completed", Some(&json!({ "indexed": 3 })), None).unwrap();
        let jobs = recent(&c, 10).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].state, "completed");
        assert_eq!(jobs[0].input["folder"], "D:/roms");
        assert_eq!(jobs[0].progress.as_ref().unwrap()["indexed"], 3);
        assert!(jobs[0].finished_at.is_some());
        assert_eq!(events(&c, &id), ["started", "completed"]);
    }

    #[test]
    fn failures_keep_the_error() {
        let c = catalog();
        let id = create(&c, KIND_SCAN, &json!({})).unwrap();
        finish(&c, &id, "failed", None, Some(("scan_error", "disk unplugged"))).unwrap();
        let job = &recent(&c, 1).unwrap()[0];
        assert_eq!(job.state, "failed");
        assert_eq!(job.error_message.as_deref(), Some("disk unplugged"));
    }

    #[test]
    fn running_jobs_become_interrupted_on_recovery() {
        let c = catalog();
        let open_job = create(&c, KIND_SCAN, &json!({})).unwrap();
        let done = create(&c, KIND_SCAN, &json!({})).unwrap();
        finish(&c, &done, "completed", None, None).unwrap();
        c.execute(
            "INSERT INTO storage_locations (id, display_name, root_path) VALUES ('s', 'C', 'C:/');
             ",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO indexed_roots (id, storage_location_id, relative_path, last_scan_status) VALUES ('r', 's', 'roms', 'running')",
            [],
        )
        .unwrap();

        assert_eq!(recover_interrupted(&c).unwrap(), 1);
        let states: Vec<(String, String)> = recent(&c, 10).unwrap().into_iter().map(|j| (j.id, j.state)).collect();
        assert!(states.contains(&(open_job.clone(), "interrupted".into())));
        assert!(states.contains(&(done, "completed".into())));
        assert_eq!(events(&c, &open_job).last().unwrap(), "interrupted");
        let root_status: String = c.query_row("SELECT last_scan_status FROM indexed_roots", [], |r| r.get(0)).unwrap();
        assert_eq!(root_status, "interrupted");
        // Running recovery again is a no-op.
        assert_eq!(recover_interrupted(&c).unwrap(), 0);
    }
}
