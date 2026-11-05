//! RetroDrive native backend: Tauri commands over the catalog services.
//! The frontend is unprivileged; every command validates its own inputs.

mod adapters;
mod catalog;
mod classify;
mod db;
mod deployment;
mod drives;
mod installations;
mod jobs;
mod paths;
mod scanner;
mod volumes;

use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Events sent to the UI.
const EVENT_SCAN_PROGRESS: &str = "scan-progress";
const EVENT_SCAN_FINISHED: &str = "scan-finished";

/// Catalog location, or the reason it could not be opened/migrated at startup.
struct Catalog {
    path: Result<PathBuf, String>,
}

impl Catalog {
    fn path(&self) -> Result<PathBuf, String> {
        self.path.clone()
    }
}

/// Jobs running in this session, with their cancellation flags.
#[derive(Default)]
struct ActiveJobs {
    jobs: Mutex<HashMap<String, ActiveJob>>,
}

struct ActiveJob {
    cancel: Arc<AtomicBool>,
    folder: String,
}

#[derive(Serialize, Clone)]
struct ActiveScan {
    job_id: String,
    folder: String,
}

#[derive(Serialize, Clone)]
struct ScanProgressEvent {
    job_id: String,
    progress: scanner::ScanProgress,
}

#[derive(Serialize, Clone)]
struct ScanFinishedEvent {
    job_id: String,
    /// `completed`, `cancelled` or `failed`.
    state: &'static str,
    summary: Option<scanner::ScanSummary>,
    error: Option<String>,
}

/// Forwards scanner progress to the UI and relays cancellation requests.
struct UiObserver {
    app: AppHandle,
    job_id: String,
    cancel: Arc<AtomicBool>,
}

impl scanner::ScanObserver for UiObserver {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn progress(&mut self, progress: &scanner::ScanProgress) {
        let _ = self
            .app
            .emit(EVENT_SCAN_PROGRESS, ScanProgressEvent { job_id: self.job_id.clone(), progress: progress.clone() });
    }
}

fn init_catalog(handle: &AppHandle) -> Result<PathBuf, String> {
    let directory = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("retrodrive.sqlite");
    let mut connection = db::open(&path)?;
    db::migrate(&mut connection, Some(&path))?;
    let interrupted = jobs::recover_interrupted(&connection)?;
    if interrupted > 0 {
        eprintln!("Marked {interrupted} unfinished job(s) from a previous session as interrupted");
    }
    Ok(path)
}

/// One page of the library with filters applied (RDS-004).
#[tauri::command]
fn query_games(catalog: State<'_, Catalog>, query: catalog::GameQuery) -> Result<catalog::GamePage, String> {
    let connection = db::open(&catalog.path()?)?;
    catalog::query_games(&connection, &query, &volumes::presence)
}

/// Library totals and platform list for the dashboard and filter menus.
#[tauri::command]
fn library_stats(catalog: State<'_, Catalog>) -> Result<catalog::LibraryStats, String> {
    let connection = db::open(&catalog.path()?)?;
    catalog::library_stats(&connection, &volumes::presence)
}

/// Every drive the catalog knows about, including ones not plugged in right now.
#[tauri::command]
fn list_storage_locations(catalog: State<'_, Catalog>) -> Result<Vec<catalog::StorageRow>, String> {
    let connection = db::open(&catalog.path()?)?;
    catalog::list_storage_locations(&connection, &volumes::presence)
}

/// Start scanning `folder` in the background and return the job id immediately.
/// Progress arrives as `scan-progress` events and the result as `scan-finished`.
/// Only one scan runs at a time.
#[tauri::command]
fn start_scan(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, folder: String) -> Result<String, String> {
    let db_path = catalog.path()?;
    let mut running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
    if !running.is_empty() {
        return Err("A scan is already running. Wait for it to finish or cancel it first.".into());
    }
    let connection = db::open(&db_path)?;
    let job_id = jobs::create(&connection, jobs::KIND_SCAN, &json!({ "folder": folder }))?;
    drop(connection);

    let cancel = Arc::new(AtomicBool::new(false));
    running.insert(job_id.clone(), ActiveJob { cancel: cancel.clone(), folder: folder.clone() });
    drop(running);

    let id = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || run_scan(app, db_path, id, folder, cancel));
    Ok(job_id)
}

fn run_scan(app: AppHandle, db_path: PathBuf, job_id: String, folder: String, cancel: Arc<AtomicBool>) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let connection = db::open(&db_path)?;
        let mut observer = UiObserver { app: app.clone(), job_id: job_id.clone(), cancel };
        scanner::scan_with(&connection, Path::new(&folder), &volumes::probe, &mut observer, scanner::ScanOptions::default())
    }))
    .unwrap_or_else(|_| Err("The scan stopped because of an internal error.".into()));

    let (state, summary, error) = match outcome {
        Ok(summary) => (if summary.cancelled { "cancelled" } else { "completed" }, Some(summary), None),
        Err(error) => ("failed", None, Some(error)),
    };

    if let Ok(connection) = db::open(&db_path) {
        let progress = summary.as_ref().map(|s| {
            json!({
                "indexed": s.indexed,
                "games": s.games,
                "new_files": s.new_files,
                "marked_missing": s.marked_missing,
                "skipped_unsupported": s.skipped_unsupported,
                "skipped_links": s.skipped_links,
                "issue_count": s.issue_count,
                "error_count": s.error_count,
                "warning_count": s.warning_count,
                "grouped_files": s.grouped_files,
                "complete": s.complete,
                "cancelled": s.cancelled,
                "remounted_from": s.remounted_from,
            })
        });
        if let Some(s) = &summary {
            for issue in &s.issues {
                let _ = jobs::event(&connection, &job_id, if issue.severity == "error" { "error" } else { "warning" }, "scan_issue", &json!({ "path": issue.path, "message": issue.message }));
            }
        }
        if let Err(e) = jobs::finish(&connection, &job_id, state, progress.as_ref(), error.as_deref().map(|e| ("scan_error", e))) {
            eprintln!("Could not record the end of job {job_id}: {e}");
        }
    }

    if let Ok(mut running) = app.state::<ActiveJobs>().jobs.lock() {
        running.remove(&job_id);
    }
    let _ = app.emit(EVENT_SCAN_FINISHED, ScanFinishedEvent { job_id, state, summary, error });
}

/// Ask a running job to stop. Returns false when the job is not running.
#[tauri::command]
fn cancel_job(active: State<'_, ActiveJobs>, job_id: String) -> Result<bool, String> {
    let running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
    Ok(match running.get(&job_id) {
        Some(job) => {
            job.cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    })
}

/// The scan currently running in this session, if any (lets the UI recover after a reload).
#[tauri::command]
fn active_scan(active: State<'_, ActiveJobs>) -> Result<Option<ActiveScan>, String> {
    let running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
    Ok(running.iter().next().map(|(id, job)| ActiveScan { job_id: id.clone(), folder: job.folder.clone() }))
}

#[tauri::command]
fn recent_jobs(catalog: State<'_, Catalog>, limit: Option<u32>) -> Result<Vec<jobs::JobRecord>, String> {
    let connection = db::open(&catalog.path()?)?;
    jobs::recent(&connection, limit.unwrap_or(10).min(100))
}

/// Platforms this build supports, with what each can do.
#[tauri::command]
fn list_adapters() -> Vec<adapters::AdapterInfo> {
    adapters::catalog()
}

/// Inspect a folder for a known platform installation (read-only).
#[tauri::command]
async fn inspect_installation(folder: String) -> Result<installations::Inspection, String> {
    tauri::async_runtime::spawn_blocking(move || installations::inspect(Path::new(&folder)))
        .await
        .map_err(|e| e.to_string())?
}

/// Save a recognized installation; refused if the adapter doesn't detect it.
#[tauri::command]
fn add_installation(catalog: State<'_, Catalog>, folder: String, adapter_id: String) -> Result<String, String> {
    let connection = db::open(&catalog.path()?)?;
    installations::add(&connection, Path::new(&folder), &adapter_id, &volumes::probe)
}

#[tauri::command]
fn list_installations(catalog: State<'_, Catalog>) -> Result<Vec<installations::InstallationRow>, String> {
    let connection = db::open(&catalog.path()?)?;
    installations::list(&connection, &volumes::presence)
}

/// Forget a saved installation. Only the catalog record is removed.
#[tauri::command]
fn remove_installation(catalog: State<'_, Catalog>, id: String) -> Result<bool, String> {
    let connection = db::open(&catalog.path()?)?;
    installations::remove(&connection, &id)
}

/// Deployment plan for an installation (RDS-009): actions, conflicts and capacity.
/// `systems` limits it to those system folders; `None` plans all. Nothing is written.
#[tauri::command]
async fn deployment_plan(catalog: State<'_, Catalog>, installation_id: String, systems: Option<Vec<String>>) -> Result<deployment::DeploymentPlan, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        deployment::plan(&connection, &installation_id, systems.as_deref(), &volumes::presence, &volumes::space)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_drives() -> Vec<drives::Drive> {
    drives::list()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ActiveJobs::default())
        .setup(|app| {
            let path = init_catalog(app.handle());
            if let Err(error) = &path {
                eprintln!("Catalog unavailable: {error}");
            }
            app.manage(Catalog { path });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            query_games,
            library_stats,
            list_adapters,
            inspect_installation,
            add_installation,
            list_installations,
            remove_installation,
            deployment_plan,
            list_storage_locations, start_scan, cancel_job, active_scan, recent_jobs, list_drives])
        .run(tauri::generate_context!())
        .expect("error starting RetroDrive");
}
