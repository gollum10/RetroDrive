//! RetroDrive Studio native backend: Tauri commands over the catalog services.
//! The frontend is unprivileged; every command validates its own inputs.

mod catalog;
mod classify;
mod db;
mod drives;
mod paths;
mod scanner;

use std::path::{Path, PathBuf};
use tauri::Manager;

/// Catalog location, or the reason it could not be opened/migrated at startup.
struct Catalog {
    path: Result<PathBuf, String>,
}

impl Catalog {
    fn path(&self) -> Result<PathBuf, String> {
        self.path.clone()
    }
}

fn init_catalog(handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("retrodrive.sqlite");
    let mut connection = db::open(&path)?;
    db::migrate(&mut connection, Some(&path))?;
    Ok(path)
}

#[tauri::command]
fn list_games(state: tauri::State<'_, Catalog>) -> Result<Vec<catalog::GameRow>, String> {
    let connection = db::open(&state.path()?)?;
    catalog::list_games(&connection, &catalog::volume_reachable)
}

/// Runs on a worker thread so large scans do not freeze the window.
/// Progress reporting and cancellation are planned in RDS-002.
#[tauri::command]
async fn scan_library(state: tauri::State<'_, Catalog>, folder: String) -> Result<scanner::ScanSummary, String> {
    let db_path = state.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut connection = db::open(&db_path)?;
        scanner::scan(&mut connection, Path::new(&folder), &drives::volume_kind)
    })
    .await
    .map_err(|e| format!("Scan worker failed: {e}"))?
}

#[tauri::command]
fn list_drives() -> Vec<drives::Drive> {
    drives::list()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = init_catalog(app.handle());
            if let Err(error) = &path {
                eprintln!("Catalog unavailable: {error}");
            }
            app.manage(Catalog { path });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_games, scan_library, list_drives])
        .run(tauri::generate_context!())
        .expect("error starting RetroDrive Studio");
}
