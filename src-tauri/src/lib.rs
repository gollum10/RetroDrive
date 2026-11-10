//! RetroDrive native backend: Tauri commands over the catalog services.
//! The frontend is unprivileged; every command validates its own inputs.

mod adapters;
mod batocera_settings;
mod bios_check;
mod bios_step;
mod catalog;
mod catalog_reset;
mod catalog_tools;
mod classify;
mod collections;
mod controller_setup;
mod controllers;
mod customizer;
mod designs;
mod copier;
mod custom_collections;
mod db;
mod deployment;
mod drives;
mod find_systems;
mod game_groups;
mod gamelists;
mod health;
mod duplicates;
mod install_helper;
mod installations;
mod lightguns;
mod jobs;
mod lakka_playlists;
mod media;
mod paths;
mod reference;
mod recalbox_settings;
mod restore;
mod retropie_settings;
mod rocknix_settings;
mod themes;
pub mod theme_studio;
/// Optional AI helpers for Theme Studio (ADR-118).
pub mod ai;
pub mod ai_design;
mod scanner;
mod sinden_step;
mod backup_zip;
mod system_backup;
mod system_restore;
mod vertical;
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
const EVENT_DEPLOY_PROGRESS: &str = "deploy-progress";
const EVENT_DEPLOY_FINISHED: &str = "deploy-finished";

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
    /// True while a reference catalog is being imported (it rewrites catalog tables,
    /// so scans and copies wait for it, and it waits for them).
    importing: AtomicBool,
    /// True while a replace is undone or RetroBat settings change (both move files on a drive).
    undoing: AtomicBool,
}

const BUSY_IMPORTING: &str = "The reference catalog is being imported. Start this when it has finished.";
const BUSY_UNDOING: &str = "A change on the drive is in progress (undo or emulator settings). Start this when it has finished.";

struct ActiveJob {
    cancel: Arc<AtomicBool>,
    folder: String,
    kind: &'static str,
}

#[derive(Serialize, Clone)]
struct ActiveScan {
    job_id: String,
    folder: String,
    /// `library_scan` or `deployment`.
    kind: &'static str,
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
    // Developers and tests can point RetroDrive at a separate catalog folder so a
    // test run never touches the real catalog (docs/12). Normal users never set this.
    let directory = match std::env::var_os("RETRODRIVE_DATA_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => handle.path().app_data_dir().map_err(|e| e.to_string())?,
    };
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("retrodrive.sqlite");
    let mut connection = db::open(&path)?;
    db::migrate(&mut connection, Some(&path))?;
    let interrupted = jobs::recover_interrupted(&connection)?;
    if interrupted > 0 {
        eprintln!("Marked {interrupted} unfinished job(s) from a previous session as interrupted");
    }
    copier::recover(&connection)?;
    // Only removes RetroDrive's own temporary files, on confirmed drives (docs/18).
    match copier::cleanup_leftovers(&connection, &volumes::presence) {
        Ok(0) => {}
        Ok(n) => eprintln!("Removed {n} unfinished temporary file(s) from an interrupted copy"),
        Err(e) => eprintln!("Could not check for unfinished copies: {e}"),
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

/// Import the user's reference catalog (.xlsx/.csv) or a genre supplement, then match.
#[tauri::command]
async fn import_reference(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, path: String) -> Result<reference::ImportSummary, String> {
    let db_path = catalog.path()?;
    {
        let running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
        if !running.is_empty() {
            return Err("A scan or copy is running. Import the catalog when it has finished.".into());
        }
        if active.undoing.load(Ordering::SeqCst) {
            return Err(BUSY_UNDOING.into());
        }
        if active.importing.swap(true, Ordering::SeqCst) {
            return Err(BUSY_IMPORTING.into());
        }
    }
    let result = tauri::async_runtime::spawn_blocking(move || reference::import(&db::open(&db_path)?, Path::new(&path))).await.map_err(|e| e.to_string());
    app.state::<ActiveJobs>().importing.store(false, Ordering::SeqCst);
    result?
}

#[tauri::command]
fn reference_status(catalog: State<'_, Catalog>) -> Result<Option<reference::CatalogStatus>, String> {
    reference::status(&db::open(&catalog.path()?)?)
}

/// Use an image the user picked as a game's box art. The image isn't copied.
#[tauri::command]
fn set_game_art(catalog: State<'_, Catalog>, file_id: String, path: String) -> Result<(), String> {
    media::set_user_art(&db::open(&catalog.path()?)?, &file_id, Path::new(&path), &volumes::probe)
}

/// Forget the user's own art for a game (the image file is untouched).
#[tauri::command]
fn remove_game_art(catalog: State<'_, Catalog>, file_id: String) -> Result<bool, String> {
    media::remove_user_art(&db::open(&catalog.path()?)?, &file_id)
}

/// Serve `rdmedia://<game file id>`: the best image linked to that game, and nothing else.
fn serve_media(db_path: Result<PathBuf, String>, uri_path: &str) -> tauri::http::Response<Vec<u8>> {
    let not_found = || tauri::http::Response::builder().status(404).body(Vec::new()).unwrap_or_default();
    let id = uri_path.trim_matches('/');
    // Theme Studio clips (ADR-117): `tv-<sha256>` from the clip store, checked when read.
    if let Some(sha) = id.strip_prefix("tv-") {
        return match db_path.map(|p| theme_studio::video_store::read(&p, sha)) {
            Ok(Ok(bytes)) => tauri::http::Response::builder()
                .header(tauri::http::header::CONTENT_TYPE, "video/mp4")
                .header(tauri::http::header::CACHE_CONTROL, "private, max-age=86400")
                .body(bytes)
                .unwrap_or_else(|_| not_found()),
            _ => not_found(),
        };
    }
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return not_found();
    }
    let found = db_path.and_then(|p| db::open(&p)).and_then(|c| media::resolve(&c, id, &volumes::presence));
    match found {
        Ok(Some((path, mime))) => match std::fs::read(&path) {
            Ok(bytes) => tauri::http::Response::builder()
                .header(tauri::http::header::CONTENT_TYPE, mime)
                .header(tauri::http::header::CACHE_CONTROL, "private, max-age=86400")
                .body(bytes)
                .unwrap_or_else(|_| not_found()),
            Err(_) => not_found(),
        },
        _ => not_found(),
    }
}

/// Remove the reference catalog; the library itself is unchanged.
#[tauri::command]
fn remove_reference(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>) -> Result<bool, String> {
    if !active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?.is_empty() || active.importing.load(Ordering::SeqCst) {
        return Err("A scan, copy or import is running. Remove the catalog when it has finished.".into());
    }
    reference::remove(&db::open(&catalog.path()?)?)
}

/// Leave files out of the library, or bring them back (catalog only; files untouched).
#[tauri::command]
fn set_left_out(catalog: State<'_, Catalog>, file_ids: Vec<String>, left_out: bool) -> Result<i64, String> {
    duplicates::set_left_out(&db::open(&catalog.path()?)?, &file_ids, left_out)
}

/// What "Clean up duplicates" would do. Reads only.
#[tauri::command]
async fn duplicate_cleanup_preview(catalog: State<'_, Catalog>) -> Result<duplicates::CleanupPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || duplicates::preview(&db::open(&db_path)?, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Keep one copy of each duplicate group; leave the rest out (catalog only).
#[tauri::command]
async fn clean_up_duplicates(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>) -> Result<i64, String> {
    if !active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?.is_empty() {
        return Err("A scan or copy is running. Clean up duplicates when it has finished.".into());
    }
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || duplicates::clean_up(&db::open(&db_path)?, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Catalog tools (ADR-097): refuse while a scan, copy or import is running.
fn catalog_idle(active: &ActiveJobs) -> Result<(), String> {
    if !active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?.is_empty() {
        return Err("A scan or copy is running. Try again when it has finished.".into());
    }
    if active.importing.load(Ordering::SeqCst) {
        return Err(BUSY_IMPORTING.into());
    }
    Ok(())
}

/// Catalog tools: the scanned library folders (reads only).
#[tauri::command]
async fn library_folders(catalog: State<'_, Catalog>) -> Result<Vec<catalog_tools::LibraryFolder>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog_tools::folders(&db::open(&db_path)?, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Catalog tools: what removing a library folder (`Some`) or the missing entries
/// (`None`) would forget (reads only).
#[tauri::command]
async fn catalog_removal_preview(catalog: State<'_, Catalog>, folder: Option<String>) -> Result<catalog_tools::Removal, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = db::open(&db_path)?;
        match folder {
            Some(id) => catalog_tools::folder_preview(&mut c, &id),
            None => catalog_tools::missing_preview(&mut c),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Catalog tools: remove a library folder's entries (`Some`) or the missing entries
/// (`None`) from the catalog, as previewed. Files on drives are never touched.
#[tauri::command]
async fn remove_catalog_entries(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, folder: Option<String>, expected: catalog_tools::Removal) -> Result<catalog_tools::Removal, String> {
    catalog_idle(&active)?;
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = db::open(&db_path)?;
        match folder {
            Some(id) => catalog_tools::remove_folder(&mut c, &id, &expected),
            None => catalog_tools::remove_missing(&mut c, &expected),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Catalog reset (ADR-098): what a reset would clear and keep (reads only).
#[tauri::command]
async fn catalog_reset_preview(catalog: State<'_, Catalog>) -> Result<catalog_reset::ResetPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog_reset::preview(&db::open(&db_path)?)).await.map_err(|e| e.to_string())?
}

/// Catalog reset: the saved catalogs next to the catalog (reads only).
#[tauri::command]
async fn saved_catalogs(catalog: State<'_, Catalog>) -> Result<Vec<catalog_reset::SavedCatalog>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog_reset::saved(&db_path)).await.map_err(|e| e.to_string())
}

fn catalog_stamp(db_path: &Path) -> Result<String, String> {
    Ok(system_backup::now(&db::open(db_path)?, "%Y-%m-%d %H.%M.%S"))
}

/// Catalog reset: save a full copy of the catalog, then clear the library.
#[tauri::command]
async fn reset_catalog(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, expected: catalog_reset::Counts) -> Result<catalog_reset::Done, String> {
    catalog_idle(&active)?;
    if active.undoing.load(Ordering::SeqCst) {
        return Err(BUSY_UNDOING.into());
    }
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog_reset::reset(&db_path, &expected, &catalog_stamp(&db_path)?)).await.map_err(|e| e.to_string())?
}

/// Catalog reset: save the current catalog, then put a saved catalog's library back.
#[tauri::command]
async fn put_back_catalog(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, path: String) -> Result<catalog_reset::Done, String> {
    catalog_idle(&active)?;
    if active.undoing.load(Ordering::SeqCst) {
        return Err(BUSY_UNDOING.into());
    }
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog_reset::put_back(&db_path, Path::new(&path), &catalog_stamp(&db_path)?)).await.map_err(|e| e.to_string())?
}

/// Catalog reset: show the catalog's folder (with the saved copies) in File Explorer.
#[tauri::command]
fn open_catalog_folder(catalog: State<'_, Catalog>) -> Result<(), String> {
    let db_path = catalog.path()?;
    std::process::Command::new("explorer.exe").arg(format!("/select,{}", db_path.display())).spawn().map(|_| ()).map_err(|e| format!("Couldn't open File Explorer: {e}"))
}

/// Catalog tools: save the game list as a new CSV file in Documents.
#[tauri::command]
async fn export_library(app: AppHandle, catalog: State<'_, Catalog>) -> Result<catalog_tools::Exported, String> {
    let folder = app.path().document_dir().map_err(|e| format!("Couldn't find your Documents folder: {e}"))?;
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let date = system_backup::now(&c, "%Y-%m-%d");
        catalog_tools::export(&c, &folder, &date, &volumes::presence)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Catalog tools: show an exported game list in File Explorer.
#[tauri::command]
fn show_exported_library(app: AppHandle, path: String) -> Result<(), String> {
    let folder = app.path().document_dir().map_err(|e| format!("Couldn't find your Documents folder: {e}"))?;
    let file = PathBuf::from(&path);
    let named = file.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("RetroDrive library") && n.ends_with(".csv"));
    if file.parent() != Some(folder.as_path()) || !named || !file.is_file() {
        return Err("That file isn't an exported game list.".into());
    }
    std::process::Command::new("explorer.exe").arg(format!("/select,{}", file.display())).spawn().map(|_| ()).map_err(|e| format!("Couldn't open File Explorer: {e}"))
}

#[tauri::command]
fn list_collections(catalog: State<'_, Catalog>) -> Result<Vec<collections::Collection>, String> {
    collections::list(&db::open(&catalog.path()?)?, &volumes::presence)
}

/// Move a collection up or down in the user's order (ADR-080).
#[tauri::command]
fn move_collection(catalog: State<'_, Catalog>, id: String, up: bool) -> Result<(), String> {
    collections::move_one(&db::open(&catalog.path()?)?, &id, up)
}

/// Star or unstar a game (Favorites, ADR-073).
#[tauri::command]
fn set_favorite(catalog: State<'_, Catalog>, file_id: String, favorite: bool) -> Result<(), String> {
    collections::set_favorite(&db::open(&catalog.path()?)?, &file_id, favorite)
}

/// Save library filters as a smart collection (ADR-073).
#[tauri::command]
fn create_smart_collection(catalog: State<'_, Catalog>, name: String, query: catalog::GameQuery) -> Result<String, String> {
    collections::create_smart(&db::open(&catalog.path()?)?, &name, &query)
}

/// Genres in the user's reference catalog, for the library's genre filter.
#[tauri::command]
fn library_genres(catalog: State<'_, Catalog>) -> Result<Vec<String>, String> {
    catalog::genres(&db::open(&catalog.path()?)?)
}

#[tauri::command]
fn create_collection(catalog: State<'_, Catalog>, name: String) -> Result<String, String> {
    collections::create(&db::open(&catalog.path()?)?, &name)
}

#[tauri::command]
fn rename_collection(catalog: State<'_, Catalog>, id: String, name: String) -> Result<(), String> {
    collections::rename(&db::open(&catalog.path()?)?, &id, &name)
}

/// Delete a collection (the list only; games and files are untouched).
#[tauri::command]
fn delete_collection(catalog: State<'_, Catalog>, id: String) -> Result<bool, String> {
    collections::delete(&db::open(&catalog.path()?)?, &id)
}

/// Add games to a collection: the given file ids, or every game matching `query` (up to 10,000).
#[tauri::command]
async fn add_to_collection(catalog: State<'_, Catalog>, id: String, file_ids: Option<Vec<String>>, query: Option<catalog::GameQuery>) -> Result<i64, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let files = match (file_ids, query) {
            (Some(files), _) => files,
            (None, Some(query)) => catalog::matching_file_ids(&connection, &query, &volumes::presence, 10_000)?,
            (None, None) => Vec::new(),
        };
        collections::add(&connection, &id, &files)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn remove_from_collection(catalog: State<'_, Catalog>, id: String, file_ids: Vec<String>) -> Result<i64, String> {
    collections::remove(&db::open(&catalog.path()?)?, &id, &file_ids)
}

/// Replaced files and their backups for one installation.
#[tauri::command]
async fn list_backups(catalog: State<'_, Catalog>, installation_id: String) -> Result<restore::BackupList, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || restore::list(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Undo one replace: swap the original back (docs/18 §10). Never deletes anything.
#[tauri::command]
async fn undo_replace(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, item_id: i64) -> Result<String, String> {
    {
        let running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
        if !running.is_empty() || active.importing.load(Ordering::SeqCst) {
            return Err("A scan, copy or import is running. Undo the replace when it has finished.".into());
        }
        if active.undoing.swap(true, Ordering::SeqCst) {
            return Err("Another undo is running.".into());
        }
    }
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || restore::undo(&db::open(&db_path)?, item_id, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Show the installation's backup folder in File Explorer, so the user can manage it.
#[tauri::command]
fn open_backup_folder(catalog: State<'_, Catalog>, installation_id: String) -> Result<(), String> {
    let folder = restore::backup_folder(&db::open(&catalog.path()?)?, &installation_id, &volumes::presence)?;
    std::process::Command::new("explorer.exe").arg(&folder).spawn().map(|_| ()).map_err(|e| format!("Couldn't open File Explorer: {e}"))
}

/// RetroBat's current emulator settings and the choices its install offers (reads only).
#[tauri::command]
async fn emulator_settings(catalog: State<'_, Catalog>, installation_id: String) -> Result<customizer::SettingsOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::options(&db::open(&db_path)?, &installation_id, &volumes::presence, &customizer::retrobat_running)).await.map_err(|e| e.to_string())?
}

/// The exact before -> after values of a settings change (reads only).
#[tauri::command]
async fn preview_emulator_settings(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<customizer::Change>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::preview(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Claim the "drive change" slot, or explain why not.
fn claim_drive_change(active: &ActiveJobs) -> Result<(), String> {
    let running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
    if !running.is_empty() || active.importing.load(Ordering::SeqCst) {
        return Err("A scan, copy or import is running. Try again when it has finished.".into());
    }
    if active.undoing.swap(true, Ordering::SeqCst) {
        return Err(BUSY_UNDOING.into());
    }
    Ok(())
}

/// Write a reviewed settings change to RetroBat (docs/18 §12).
#[tauri::command]
async fn apply_emulator_settings(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<customizer::Change>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || customizer::apply(&db::open(&db_path)?, &installation_id, &changes, &file_sha, &volumes::presence, &customizer::retrobat_running)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

#[tauri::command]
async fn emulator_settings_history(catalog: State<'_, Catalog>, installation_id: String) -> Result<Vec<customizer::HistoryRow>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::history(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Put RetroBat's previous settings back.
#[tauri::command]
async fn undo_emulator_settings(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, change_id: i64) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let installation: String = connection.query_row("SELECT installation_id FROM settings_changes WHERE id = ?1", [change_id], |r| r.get(0)).map_err(|e| e.to_string())?;
        customizer::undo(&connection, change_id, &volumes::presence, running_for(&connection, &installation))
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// RetroBat's themes and what can be changed in each (reads only).
#[tauri::command]
async fn list_themes(catalog: State<'_, Catalog>, installation_id: String) -> Result<themes::ThemeList, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || themes::list(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// A picture or font as a data URL for the theme preview.
#[tauri::command]
async fn theme_preview_asset(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || themes::preview_asset(&path)).await.map_err(|e| e.to_string())?
}

/// Save a remix of an installed theme as a new theme folder (the original is never changed).
#[tauri::command]
async fn create_theme_remix(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, remix: themes::Remix) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || themes::create(&db::open(&db_path)?, &installation_id, &remix, &volumes::presence, &|p: &Path| volumes::space(p).map(|s| s.free_bytes)))
        .await
        .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Theme Studio (ADR-115): put a saved theme on RetroBat as a new theme folder (never replacing one).
#[tauri::command]
async fn put_theme_on_retrobat(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, folder: String) -> Result<theme_studio::export::save::SavedTheme, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let install = customizer::install(&db::open(&db_path)?, &installation_id, &volumes::presence)?;
        let themes = install.root.join("emulationstation").join(".emulationstation").join("themes");
        theme_studio::export::save::put_on_retrobat(&themes, std::path::Path::new(&folder))
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// What making a theme RetroBat's theme would change (reads only).
#[tauri::command]
async fn preview_theme_switch(catalog: State<'_, Catalog>, installation_id: String, theme: String) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::preview_theme(&db::open(&db_path)?, &installation_id, &theme, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Make a theme RetroBat's theme (reviewed, backed up, undoable like other settings).
#[tauri::command]
async fn apply_theme_switch(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, theme: String, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || customizer::apply_theme(&db::open(&db_path)?, &installation_id, &theme, &file_sha, &volumes::presence, &customizer::retrobat_running)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Boot video, music and frontend options from retrobat.ini (reads only).
#[tauri::command]
async fn look_options(catalog: State<'_, Catalog>, installation_id: String) -> Result<customizer::LookOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::look_options(&db::open(&db_path)?, &installation_id, &volumes::presence, &customizer::retrobat_running)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn preview_look(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<customizer::IniChange>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::preview_look(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Write a reviewed retrobat.ini change (backed up, undoable).
#[tauri::command]
async fn apply_look(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<customizer::IniChange>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || customizer::apply_look(&db::open(&db_path)?, &installation_id, &changes, &file_sha, &volumes::presence, &customizer::retrobat_running)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Copy the user's own boot video (.mp4) or music (.ogg/.mp3) into RetroBat as a new file.
#[tauri::command]
async fn add_retrobat_media(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, kind: String, path: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        match kind.as_str() {
            "video" => customizer::add_boot_video(&connection, &installation_id, &path, &volumes::presence),
            "music" => customizer::add_music(&connection, &installation_id, &path, &volumes::presence),
            _ => Err("Unknown kind of file.".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

#[tauri::command]
fn list_designs(catalog: State<'_, Catalog>) -> Result<Vec<designs::Design>, String> {
    designs::list(&db::open(&catalog.path()?)?)
}

/// Start a design (a draft), empty or from an installation's current setup.
#[tauri::command]
async fn create_design(catalog: State<'_, Catalog>, name: String, from_installation: Option<String>) -> Result<String, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let seed = match from_installation {
            Some(id) => designs::seed_from(&connection, &id, &volumes::presence)?,
            None => designs::DesignSpec::default(),
        };
        designs::create(&connection, &name, seed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn save_design_draft(catalog: State<'_, Catalog>, id: String, name: String, spec: designs::DesignSpec) -> Result<(), String> {
    designs::save_draft(&db::open(&catalog.path()?)?, &id, &name, &spec)
}

#[tauri::command]
fn finish_design(catalog: State<'_, Catalog>, id: String) -> Result<(), String> {
    designs::finish(&db::open(&catalog.path()?)?, &id)
}

#[tauri::command]
fn abandon_design(catalog: State<'_, Catalog>, id: String) -> Result<bool, String> {
    designs::abandon(&db::open(&catalog.path()?)?, &id)
}

#[tauri::command]
fn delete_design(catalog: State<'_, Catalog>, id: String) -> Result<bool, String> {
    designs::delete(&db::open(&catalog.path()?)?, &id)
}

#[tauri::command]
async fn music_settings(catalog: State<'_, Catalog>, installation_id: String) -> Result<Vec<designs::MusicSetting>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || designs::music_settings(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Everything applying a design would do (reads only).
#[tauri::command]
async fn preview_design(catalog: State<'_, Catalog>, id: String, installation_id: String) -> Result<designs::DesignPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || designs::preview(&db::open(&db_path)?, &id, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Apply a finished design to an installation as one undoable step (docs/18 §15).
#[tauri::command]
async fn apply_design(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, id: String, installation_id: String, es_sha: String, ini_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        designs::apply(&db::open(&db_path)?, &id, &installation_id, &es_sha, &ini_sha, &volumes::presence, &customizer::retrobat_running, &|p: &Path| volumes::space(p).map(|s| s.free_bytes))
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

#[tauri::command]
fn design_applications(catalog: State<'_, Catalog>, installation_id: String) -> Result<Vec<designs::Application>, String> {
    designs::applications(&db::open(&catalog.path()?)?, &installation_id)
}

#[tauri::command]
async fn undo_design_application(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, id: i64) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let installation: String = connection.query_row("SELECT installation_id FROM design_applications WHERE id = ?1", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
        designs::undo(&connection, id, &volumes::presence, running_for(&connection, &installation))
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Install helper (RDS-027): what an image the user downloaded is (reads only).
#[tauri::command]
async fn install_image_info(path: String, system: Option<String>) -> Result<install_helper::ImageInfo, String> {
    tauri::async_runtime::spawn_blocking(move || install_helper::inspect(&path, system.as_deref())).await.map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
struct HashProgress {
    done: u64,
    total: u64,
}

/// Compare the image with the checksum from the download page (reads only).
#[tauri::command]
async fn install_checksum(app: AppHandle, path: String, expected: String) -> Result<install_helper::ChecksumResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let last = std::sync::atomic::AtomicU64::new(0);
        install_helper::checksum(&path, &expected, &|done, total| {
            // About one update per 64 MB.
            if done - last.load(Ordering::Relaxed) >= 64 << 20 || done == total {
                last.store(done, Ordering::Relaxed);
                let _ = app.emit("install-hash", HashProgress { done, total });
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The image writers installed on this PC.
#[tauri::command]
fn install_writers() -> Vec<install_helper::Writer> {
    install_helper::writers()
}

/// Open Raspberry Pi Imager or balenaEtcher (from its install folder) for the image.
#[tauri::command]
fn open_install_writer(id: String, image: String) -> Result<String, String> {
    install_helper::open_writer(&id, &image)
}

/// AI helpers (ADR-118): connection status (no network).
#[tauri::command]
fn ai_status(catalog: State<'_, Catalog>) -> Result<ai::AiStatus, String> {
    ai::status(&catalog.path()?, &ai::CredentialManager)
}

/// AI helpers: open the Claude Console page where the user signs in and creates a key.
#[tauri::command]
fn ai_open_keys_page() -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(ai::CLAUDE_KEYS_PAGE).spawn().map(|_| ()).map_err(|e| format!("Couldn't open the browser: {e}"))
}

/// AI helpers: check a Claude key (one request, no tokens) and keep it in Credential Manager.
#[tauri::command]
async fn ai_connect_claude(key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let key = key.trim().to_string();
        ai::check_claude_key(&key)?;
        ai::KeyStore::set(&ai::CredentialManager, &key)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI helpers: remove the Claude key from Credential Manager.
#[tauri::command]
fn ai_disconnect_claude() -> Result<(), String> {
    ai::KeyStore::delete(&ai::CredentialManager)
}

/// AI helpers: choose which Claude model design help uses.
#[tauri::command]
fn ai_set_claude_model(catalog: State<'_, Catalog>, model: String) -> Result<(), String> {
    if !ai::CLAUDE_MODELS.iter().any(|(id, _)| *id == model) {
        return Err("Unknown model.".into());
    }
    let path = catalog.path()?;
    let mut settings = ai::load_settings(&path);
    settings.claude_model = Some(model);
    ai::save_settings(&path, &settings)
}

#[derive(serde::Serialize)]
struct PrivateFound { url: String, models: Vec<String> }

/// AI helpers: look for a private AI program at its usual address on this PC.
#[tauri::command]
async fn ai_find_private() -> Result<Option<PrivateFound>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(ai::USUAL_PRIVATE.iter().find_map(|url| ai::private_models(url).ok().map(|models| PrivateFound { url: url.to_string(), models })))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI helpers: the models a private AI server offers (local or private-network addresses only).
#[tauri::command]
async fn ai_private_models(url: String) -> Result<PrivateFound, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = ai::private_url(&url)?;
        let models = ai::private_models(&url)?;
        Ok(PrivateFound { url, models })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI helpers: remember a private AI server and model (checked against what it offers).
#[tauri::command]
async fn ai_connect_private(catalog: State<'_, Catalog>, url: String, model: String) -> Result<(), String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let url = ai::private_url(&url)?;
        if !ai::private_models(&url)?.contains(&model) {
            return Err("That model isn't offered by the server anymore.".into());
        }
        let mut settings = ai::load_settings(&path);
        settings.private_url = Some(url);
        settings.private_model = Some(model);
        ai::save_settings(&path, &settings)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI helpers: forget the private AI server.
#[tauri::command]
fn ai_disconnect_private(catalog: State<'_, Catalog>) -> Result<(), String> {
    let path = catalog.path()?;
    let mut settings = ai::load_settings(&path);
    settings.private_url = None;
    settings.private_model = None;
    ai::save_settings(&path, &settings)
}

/// Other AI services (ADR-121): open the page where the user creates a key (fixed addresses).
#[tauri::command]
fn ai_open_service_keys(id: String) -> Result<(), String> {
    let s = ai::service(&id).ok_or("Unknown AI service.")?;
    std::process::Command::new("explorer.exe").arg(s.keys_page).spawn().map(|_| ()).map_err(|e| format!("Couldn't open the browser: {e}"))
}

/// Other AI services: check a key and list the models it can use (custom: the address too).
#[tauri::command]
async fn ai_check_service(catalog: State<'_, Catalog>, id: String, key: String, url: Option<String>) -> Result<Vec<String>, String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut settings = ai::load_settings(&path);
        if id == ai::CUSTOM {
            settings.custom_url = Some(ai::custom_url(url.as_deref().unwrap_or(""))?);
        }
        let (base, name, check) = ai::service_address(&id, &settings)?;
        ai::service_models(&base, check, &name, key.trim())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Other AI services: keep the key in Credential Manager and remember the model chosen.
#[tauri::command]
async fn ai_connect_service(catalog: State<'_, Catalog>, id: String, key: String, model: String, url: Option<String>) -> Result<(), String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let key = key.trim().to_string();
        let mut settings = ai::load_settings(&path);
        if id == ai::CUSTOM {
            settings.custom_url = Some(ai::custom_url(url.as_deref().unwrap_or(""))?);
        }
        let (base, name, check) = ai::service_address(&id, &settings)?;
        if !ai::service_models(&base, check, &name, &key)?.contains(&model) {
            return Err(format!("{name} doesn't offer that model to this key."));
        }
        ai::KeyStore::set(&ai::ServiceKey(id.clone()), &key)?;
        settings.service_models.insert(id, model);
        ai::save_settings(&path, &settings)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Other AI services: the models a connected service offers (uses the stored key).
#[tauri::command]
async fn ai_service_models(catalog: State<'_, Catalog>, id: String) -> Result<Vec<String>, String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let settings = ai::load_settings(&path);
        let (base, name, check) = ai::service_address(&id, &settings)?;
        let key = ai::KeyStore::get(&ai::ServiceKey(id))?.ok_or("Connect the service first.")?;
        ai::service_models(&base, check, &name, &key)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Other AI services: choose another model for a connected service.
#[tauri::command]
fn ai_set_service_model(catalog: State<'_, Catalog>, id: String, model: String) -> Result<(), String> {
    let path = catalog.path()?;
    let mut settings = ai::load_settings(&path);
    if !settings.service_models.contains_key(&id) || model.is_empty() || model.len() > 200 {
        return Err("Connect the service first.".into());
    }
    settings.service_models.insert(id, model);
    ai::save_settings(&path, &settings)
}

/// Other AI services: remove the key from Credential Manager and forget the service.
#[tauri::command]
fn ai_disconnect_service(catalog: State<'_, Catalog>, id: String) -> Result<(), String> {
    if id != ai::CUSTOM && ai::service(&id).is_none() {
        return Err("Unknown AI service.".into());
    }
    ai::KeyStore::delete(&ai::ServiceKey(id.clone()))?;
    let path = catalog.path()?;
    let mut settings = ai::load_settings(&path);
    settings.service_models.remove(&id);
    if id == ai::CUSTOM {
        settings.custom_url = None;
    }
    ai::save_settings(&path, &settings)
}

/// Ask AI for design suggestions (ADR-120): one screen's layers and the request typed.
#[tauri::command]
async fn ai_suggest_design(catalog: State<'_, Catalog>, provider: String, screen: serde_json::Value, request: String) -> Result<ai_design::DesignAnswer, String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || ai_design::ask(&path, &ai::CredentialManager, &|id| ai::KeyStore::get(&ai::ServiceKey(id.to_string())), &ai_design::DesignAsk { provider, screen, request }))
        .await
        .map_err(|e| e.to_string())?
}

/// The exact text an AI design request sends (for "Show exactly"), besides the fixed instructions.
#[tauri::command]
fn ai_design_preview(screen: serde_json::Value, request: String) -> (String, String) {
    (ai_design::SYSTEM_PROMPT.to_string(), ai_design::user_message(&screen, &request))
}

/// Open an official download page (fixed addresses only).
#[tauri::command]
fn open_download_page(id: String) -> Result<(), String> {
    install_helper::open_download_page(&id)
}

/// Controllers workspace: game controllers plugged into this PC (reads only).
#[tauri::command]
async fn connected_controllers(catalog: State<'_, Catalog>) -> Result<Vec<controllers::Connected>, String> {
    let path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<controllers::Connected>, String> {
        let remembered = controllers::remembered(&db::open(&path)?)?;
        Ok(controllers::connected(&remembered))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Controllers step 2: each installation's state for this controller (reads only).
#[tauri::command]
async fn controller_setup_preview(catalog: State<'_, Catalog>, vid: String, pid: String, xinput: bool) -> Result<Vec<controller_setup::SetupTarget>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || controller_setup::preview(&db::open(&db_path)?, &vid, &pid, xinput, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Controllers step 2: add the setup files on the chosen installations (journaled, undoable).
#[tauri::command]
async fn apply_controller_setup(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, vid: String, pid: String, xinput: bool, installation_ids: Vec<String>) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || controller_setup::apply(&db::open(&db_path)?, &installation_ids, &vid, &pid, xinput, &volumes::presence))
        .await
        .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Controllers workspace: remember the style the user confirmed (`None` forgets it).
#[tauri::command]
fn remember_controller_type(catalog: State<'_, Catalog>, vid: String, pid: String, type_id: Option<String>, xinput: Option<bool>) -> Result<(), String> {
    controllers::remember(&db::open(&catalog.path()?)?, &vid, &pid, type_id.as_deref(), xinput.unwrap_or(false))
}

/// Light guns: the guns plugged into this PC, and the user's own gun (reads only).
#[derive(serde::Serialize)]
struct LightGunState {
    connected: Vec<lightguns::ConnectedGun>,
    /// The gun the user chose without one plugged in.
    chosen: Option<lightguns::Gun>,
    guns: Vec<lightguns::Gun>,
    gun_games: usize,
    /// Platforms of the user's installations (for the guide).
    systems: Vec<String>,
    /// Steps for RetroPie, ES-DE and Recalbox, by gun id then system (ADR-075).
    other_steps: std::collections::BTreeMap<String, std::collections::BTreeMap<&'static str, Vec<String>>>,
}

#[tauri::command]
async fn light_guns(catalog: State<'_, Catalog>) -> Result<LightGunState, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<LightGunState, String> {
        let connection = db::open(&db_path)?;
        let remembered = lightguns::remembered(&connection)?;
        let mut systems: Vec<String> = installations::located(&connection, &volumes::presence, None)?.into_iter().map(|l| l.adapter_id).collect();
        systems.sort();
        systems.dedup();
        Ok(LightGunState {
            connected: lightguns::connected(&remembered),
            chosen: remembered.get("manual").and_then(|id| lightguns::gun(id)).cloned(),
            guns: lightguns::guns().to_vec(),
            gun_games: lightguns::gun_game_count(),
            other_steps: lightguns::guns().iter().map(|g| (g.id.to_string(), lightguns::other_steps(g.id))).collect(),
            systems,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Light guns D: whether each RetroBat installation has the Sinden software (reads only).
#[tauri::command]
async fn sinden_status(catalog: State<'_, Catalog>) -> Result<Vec<sinden_step::SindenStatus>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || sinden_step::status(&db::open(&db_path)?, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Light guns D: what copying the user's Sinden folder into RetroBat would do (reads only).
#[tauri::command]
async fn preview_sinden_step(catalog: State<'_, Catalog>, installation_id: String, source: String) -> Result<sinden_step::SindenPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || sinden_step::preview(&db::open(&db_path)?, &installation_id, Path::new(&source), &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Light guns D: copy the Sinden software into RetroBat (new files only; nothing replaced).
#[tauri::command]
async fn apply_sinden_step(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, source: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        sinden_step::apply(&db::open(&db_path)?, &installation_id, Path::new(&source), &volumes::presence, &|phase, done, total| {
            let _ = app.emit("art-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Backups (FR-09, ADR-090): the folder backups go into, the user's choice or
/// `Documents\RetroDrive Backups`.
fn backup_destination(app: &AppHandle, folder: Option<String>) -> Result<PathBuf, String> {
    match folder.filter(|f| !f.trim().is_empty()) {
        Some(f) if Path::new(&f).is_absolute() => Ok(PathBuf::from(f)),
        Some(_) => Err("Choose a full folder path for backups.".into()),
        None => Ok(app.path().document_dir().map_err(|e| format!("Couldn't find your Documents folder: {e}"))?.join("RetroDrive Backups")),
    }
}

fn running_frontend(connection: &rusqlite::Connection, installation_id: &str) -> customizer::RunningFn<'static> {
    running_for(connection, installation_id)
}

/// Maintenance checks (FR-10, ADR-093): one system's health (reads only).
#[tauri::command]
async fn check_health(app: AppHandle, catalog: State<'_, Catalog>, installation_id: String, folder: Option<String>) -> Result<health::Health, String> {
    let backups = backup_destination(&app, folder)?;
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || health::check(&db::open(&db_path)?, &installation_id, &backups, &volumes::presence, &volumes::space)).await.map_err(|e| e.to_string())?
}

/// Settings → About and legal (LEGAL-009): open the notices shipped next to the app
/// (`license`: RetroDrive's Apache-2.0 LICENSE.txt, `software`: THIRD_PARTY_NOTICES.txt,
/// `data`: THIRD_PARTY_CONTROLLERS.txt) in the
/// Notepad.
#[tauri::command]
fn open_third_party_notices(app: AppHandle, which: String) -> Result<(), String> {
    let name = match which.as_str() {
        "license" => "LICENSE.txt",
        "software" => "THIRD_PARTY_NOTICES.txt",
        "data" => "THIRD_PARTY_CONTROLLERS.txt",
        _ => return Err("Unknown notices file.".into()),
    };
    let path = app.path().resource_dir().map_err(|e| format!("Couldn't find RetroDrive's files: {e}"))?.join(name);
    if !path.is_file() {
        return Err(format!("{name} wasn't found next to RetroDrive."));
    }
    // Notepad is on every Windows PC, so no "choose an app" prompt.
    std::process::Command::new("notepad.exe").arg(&path).spawn().map(|_| ()).map_err(|e| format!("Couldn't open {name}: {e}"))
}

/// Settings: the folder backups go into, as a path to show.
#[tauri::command]
fn backup_destination_path(app: AppHandle, folder: Option<String>) -> Result<String, String> {
    backup_destination(&app, folder).map(|p| p.display().to_string())
}

/// Backups: what a backup of this system would copy (reads only).
#[tauri::command]
async fn system_backup_preview(app: AppHandle, catalog: State<'_, Catalog>, installation_id: String, folder: Option<String>) -> Result<system_backup::BackupPreview, String> {
    let destination = backup_destination(&app, folder)?;
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let running = running_frontend(&c, &installation_id);
        system_backup::preview(&c, &installation_id, &destination, &volumes::presence, running)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Backups: copy the system's saves, settings and game lists into a new folder on this PC.
#[tauri::command]
async fn run_system_backup(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, optional: Vec<String>, folder: Option<String>) -> Result<system_backup::BackupEntry, String> {
    let destination = backup_destination(&app, folder)?;
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let running = running_frontend(&c, &installation_id);
        system_backup::run(&c, &installation_id, &optional, &destination, &volumes::presence, running, &|phase, done, total| {
            let _ = app.emit("backup-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Backups: the backups in the folder, newest first (reads only).
#[tauri::command]
async fn list_system_backups(app: AppHandle, folder: Option<String>) -> Result<Vec<system_backup::BackupEntry>, String> {
    let destination = backup_destination(&app, folder)?;
    tauri::async_runtime::spawn_blocking(move || system_backup::list(&destination)).await.map_err(|e| e.to_string())
}

/// Backups: check a backup's files against its list (reads only).
#[tauri::command]
async fn check_system_backup(app: AppHandle, folder: Option<String>, backup: String) -> Result<system_backup::CheckResult, String> {
    let destination = backup_destination(&app, folder)?;
    let path = PathBuf::from(&backup);
    if path.parent() != Some(destination.as_path()) {
        return Err("That isn't a backup in the backup folder.".into());
    }
    tauri::async_runtime::spawn_blocking(move || system_backup::check(&path)).await.map_err(|e| e.to_string())?
}

/// Backups: show the backup folder (or one backup) in File Explorer.
#[tauri::command]
fn open_system_backups(app: AppHandle, folder: Option<String>, backup: Option<String>) -> Result<(), String> {
    let destination = backup_destination(&app, folder)?;
    let target = match backup.map(PathBuf::from) {
        Some(b) if b.parent() == Some(destination.as_path()) && b.is_dir() => b,
        Some(_) => return Err("That isn't a backup in the backup folder.".into()),
        None if destination.is_dir() => destination,
        None => return Err("There are no backups yet.".into()),
    };
    std::process::Command::new("explorer.exe").arg(&target).spawn().map(|_| ()).map_err(|e| format!("Couldn't open File Explorer: {e}"))
}

/// Backups (ADR-096): save a finished backup as one .zip file in the backup folder.
#[tauri::command]
async fn save_backup_zip(app: AppHandle, folder: Option<String>, backup: String) -> Result<backup_zip::ZipSaved, String> {
    let destination = backup_destination(&app, folder.clone())?;
    let backup = backup_in(&app, folder, &backup)?;
    tauri::async_runtime::spawn_blocking(move || {
        backup_zip::save(&backup, &destination, &|phase, done, total| {
            let _ = app.emit("backup-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Backups (ADR-096): add a backup from a .zip the user chose, as a new folder in the
/// backup folder (nothing else is written).
#[tauri::command]
async fn add_backup_zip(app: AppHandle, folder: Option<String>, zip: String) -> Result<system_backup::BackupEntry, String> {
    let destination = backup_destination(&app, folder)?;
    let path = PathBuf::from(&zip);
    if !path.is_absolute() || !path.is_file() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        return Err("Choose a .zip file.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        backup_zip::add(&path, &destination, &|phase, done, total| {
            let _ = app.emit("backup-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restore (FR-09 step 2, ADR-091): a backup directly inside the backup folder.
fn backup_in(app: &AppHandle, folder: Option<String>, backup: &str) -> Result<PathBuf, String> {
    let destination = backup_destination(app, folder)?;
    let path = PathBuf::from(backup);
    if path.parent() != Some(destination.as_path()) || !path.is_dir() {
        return Err("That isn't a backup in the backup folder.".into());
    }
    Ok(path)
}

/// Restore: what putting these groups back would do (reads only).
#[tauri::command]
async fn restore_preview(app: AppHandle, catalog: State<'_, Catalog>, installation_id: String, backup: String, groups: Vec<String>, folder: Option<String>) -> Result<system_restore::RestorePreview, String> {
    let backup = backup_in(&app, folder, &backup)?;
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let running = running_frontend(&c, &installation_id);
        system_restore::preview(&c, &backup, &installation_id, &groups, &volumes::presence, running)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restore: put the reviewed files back (journaled; undoable; nothing deleted).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn apply_restore(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, backup: String, groups: Vec<String>, use_backup: bool, fingerprint: String, folder: Option<String>) -> Result<system_restore::RestoreSummary, String> {
    let backup = backup_in(&app, folder, &backup)?;
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let running = running_frontend(&c, &installation_id);
        system_restore::apply(&c, &backup, &installation_id, &groups, use_backup, &fingerprint, &volumes::presence, running)
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Restore: this system's restores (reads only).
#[tauri::command]
async fn list_restores(catalog: State<'_, Catalog>, installation_id: String) -> Result<Vec<system_restore::RestoreRecord>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || system_restore::list(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Restore: undo one (moves restored files aside, puts the originals back).
#[tauri::command]
async fn undo_restore(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, restore_id: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let c = db::open(&db_path)?;
        let running = running_frontend(&c, &installation_id);
        system_restore::undo(&c, &installation_id, &restore_id, &volumes::presence, running)
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Light guns: remember the user's gun for `vid:pid`, or `manual` (`None` forgets it).
#[tauri::command]
fn remember_light_gun(catalog: State<'_, Catalog>, usb_id: String, gun_id: Option<String>) -> Result<(), String> {
    lightguns::remember(&db::open(&catalog.path()?)?, &usb_id, gun_id.as_deref())
}

/// Plan and Sync: the confirmed controllers' state on this installation (reads only).
#[tauri::command]
async fn controllers_for_installation(catalog: State<'_, Catalog>, installation_id: String) -> Result<controller_setup::InstallationControllers, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || controller_setup::for_installation(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Controllers workspace: look a controller up by name or `vid:pid`.
#[tauri::command]
fn find_controllers(query: String) -> Vec<controllers::ControllerInfo> {
    controllers::search(&query)
}

/// Controllers workspace: controller types, data sources and their licences.
#[tauri::command]
fn controller_reference() -> controllers::Reference {
    controllers::reference()
}

/// BIOS step (RetroPie's bios share, ES-DE's RetroArch system folder): what it would do (reads only).
#[tauri::command]
async fn preview_bios_step(catalog: State<'_, Catalog>, installation_id: String) -> Result<bios_step::BiosPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || bios_step::preview(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// BIOS step: copy the missing files (new files only; nothing replaced or deleted).
#[tauri::command]
async fn apply_bios_step(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        bios_step::apply(&db::open(&db_path)?, &installation_id, &volumes::presence, &|phase, done, total| {
            let _ = app.emit("art-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// RetroPie: installed emulators per system and the defaults (reads only).
#[tauri::command]
async fn retropie_options(catalog: State<'_, Catalog>, installation_id: String) -> Result<retropie_settings::RetroPieOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || retropie_settings::options(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What changing RetroPie's default emulators would do (reads only).
#[tauri::command]
async fn preview_retropie_emulators(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<retropie_settings::EmulatorChange>) -> Result<retropie_settings::EmulatorPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || retropie_settings::preview(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Save reviewed default emulators on RetroPie (journaled, backed up, undoable).
#[tauri::command]
async fn apply_retropie_emulators(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<retropie_settings::EmulatorChange>, shas: std::collections::BTreeMap<String, String>) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || retropie_settings::apply(&db::open(&db_path)?, &installation_id, &changes, &shas, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Game groups (light gun, vertical) on RetroBat/Batocera: which are on (reads only).
#[tauri::command]
async fn game_groups_state(catalog: State<'_, Catalog>, installation_id: String) -> Result<game_groups::GroupsState, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || game_groups::state(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What turning game groups on or off would change (reads only).
#[tauri::command]
async fn preview_game_groups(catalog: State<'_, Catalog>, installation_id: String, wanted: std::collections::BTreeMap<String, bool>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || game_groups::preview(&db::open(&db_path)?, &installation_id, &wanted, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Turn game groups on or off (journaled, backed up, undoable).
#[tauri::command]
async fn apply_game_groups(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, wanted: std::collections::BTreeMap<String, bool>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let running = running_for(&connection, &installation_id);
        game_groups::apply(&connection, &installation_id, &wanted, &file_sha, &volumes::presence, running)
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// The games in a Recalbox system folder, for settings of one game (reads only, ADR-085).
#[tauri::command]
async fn recalbox_games(catalog: State<'_, Catalog>, installation_id: String, system: String) -> Result<Vec<String>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::games(&db::open(&db_path)?, &installation_id, &system, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// The Recalbox games of a system with their own settings file (reads only).
#[tauri::command]
async fn recalbox_games_with_settings(catalog: State<'_, Catalog>, installation_id: String, system: String) -> Result<Vec<String>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::games_with_settings(&db::open(&db_path)?, &installation_id, &system, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// One Recalbox game's own settings (reads only).
#[tauri::command]
async fn recalbox_game_options(catalog: State<'_, Catalog>, installation_id: String, system: String, game: String) -> Result<recalbox_settings::GameOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::game_options(&db::open(&db_path)?, &installation_id, &system, &game, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What changes to one Recalbox game's settings would do (reads only).
#[tauri::command]
async fn preview_recalbox_game(catalog: State<'_, Catalog>, installation_id: String, system: String, game: String, changes: Vec<recalbox_settings::ConfChange>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::preview_game(&db::open(&db_path)?, &installation_id, &system, &game, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Save reviewed settings for one Recalbox game (its override file; journaled, undoable).
#[tauri::command]
async fn apply_recalbox_game(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, system: String, game: String, changes: Vec<recalbox_settings::ConfChange>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || recalbox_settings::apply_game(&db::open(&db_path)?, &installation_id, &system, &game, &changes, &file_sha, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// What adding copied games to Lakka's playlists (and box art) would do (reads only, ADR-086).
#[tauri::command]
async fn preview_lakka_playlists(catalog: State<'_, Catalog>, installation_id: String) -> Result<lakka_playlists::LakkaPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || lakka_playlists::preview(&db::open(&db_path)?, &installation_id, None, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Add copied games to Lakka's playlists and copy their box art (as reviewed).
#[tauri::command]
async fn apply_lakka_playlists(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, shas: std::collections::BTreeMap<String, String>) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || lakka_playlists::apply(&db::open(&db_path)?, &installation_id, None, Some(&shas), &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Which BIOS files the library's systems need, in the library and on a system (reads only, ADR-088).
#[tauri::command]
async fn check_bios(catalog: State<'_, Catalog>, installation_id: Option<String>) -> Result<bios_check::BiosCheck, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || bios_check::check(&db::open(&db_path)?, installation_id.as_deref(), &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Recalbox settings and themes offered, with current values (reads only).
#[tauri::command]
async fn recalbox_options(catalog: State<'_, Catalog>, installation_id: String) -> Result<recalbox_settings::RecalboxOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::options(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What Recalbox settings changes would do (reads only).
#[tauri::command]
async fn preview_recalbox_settings(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<recalbox_settings::ConfChange>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || recalbox_settings::preview(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Save reviewed Recalbox settings (journaled, backed up, undoable).
#[tauri::command]
async fn apply_recalbox_settings(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<recalbox_settings::ConfChange>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || recalbox_settings::apply(&db::open(&db_path)?, &installation_id, &changes, &file_sha, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// A RetroBat system's games and their own settings (reads only, ADR-084).
#[tauri::command]
async fn retrobat_game_settings(catalog: State<'_, Catalog>, installation_id: String, system: String) -> Result<customizer::GameSettings, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || customizer::game_settings(&db::open(&db_path)?, &installation_id, &system, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// The games in a ROCKNIX system folder, for settings of one game (reads only).
#[tauri::command]
async fn rocknix_games(catalog: State<'_, Catalog>, installation_id: String, system: String) -> Result<Vec<String>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || rocknix_settings::games(&db::open(&db_path)?, &installation_id, &system, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// The games in a Batocera system folder, for settings of one game (reads only).
#[tauri::command]
async fn batocera_games(catalog: State<'_, Catalog>, installation_id: String, system: String) -> Result<Vec<String>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || batocera_settings::games(&db::open(&db_path)?, &installation_id, &system, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// ROCKNIX settings offered, with current values (reads only; network installs).
#[tauri::command]
async fn rocknix_options(catalog: State<'_, Catalog>, installation_id: String) -> Result<rocknix_settings::RocknixOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || rocknix_settings::options(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What ROCKNIX settings changes would do (reads only).
#[tauri::command]
async fn preview_rocknix_settings(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<rocknix_settings::ConfChange>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || rocknix_settings::preview(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Save reviewed ROCKNIX settings (journaled, backed up, undoable).
#[tauri::command]
async fn apply_rocknix_settings(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<rocknix_settings::ConfChange>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || rocknix_settings::apply(&db::open(&db_path)?, &installation_id, &changes, &file_sha, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Batocera settings and themes offered, with current values (reads only).
#[tauri::command]
async fn batocera_options(catalog: State<'_, Catalog>, installation_id: String) -> Result<batocera_settings::BatoceraOptions, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || batocera_settings::options(&db::open(&db_path)?, &installation_id, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// What Batocera settings changes would do (reads only).
#[tauri::command]
async fn preview_batocera_settings(catalog: State<'_, Catalog>, installation_id: String, changes: Vec<batocera_settings::ConfChange>) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || batocera_settings::preview(&db::open(&db_path)?, &installation_id, &changes, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Save reviewed Batocera settings (journaled, backed up, undoable).
#[tauri::command]
async fn apply_batocera_settings(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, changes: Vec<batocera_settings::ConfChange>, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || batocera_settings::apply(&db::open(&db_path)?, &installation_id, &changes, &file_sha, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// What switching Batocera's theme would change (reads only).
#[tauri::command]
async fn preview_batocera_theme(catalog: State<'_, Catalog>, installation_id: String, theme: String) -> Result<customizer::Preview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || batocera_settings::preview_theme(&db::open(&db_path)?, &installation_id, &theme, &volumes::presence)).await.map_err(|e| e.to_string())?
}

/// Switch Batocera's theme (journaled, backed up, undoable).
#[tauri::command]
async fn apply_batocera_theme(catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, theme: String, file_sha: String) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || batocera_settings::apply_theme(&db::open(&db_path)?, &installation_id, &theme, &file_sha, &volumes::presence)).await.map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Whether an installation's frontend could be running on this PC (and so overwrite
/// its own files when it closes): RetroBat yes; Batocera runs on its own machine.
fn running_for(connection: &rusqlite::Connection, installation_id: &str) -> customizer::RunningFn<'static> {
    let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [installation_id], |r| r.get(0)).unwrap_or_default();
    if adapter == "batocera" || adapter == "retropie" || adapter == "recalbox" || adapter == "rocknix" || adapter == "lakka" {
        &customizer::never_running
    } else if adapter == "esde" {
        &customizer::esde_running
    } else {
        &customizer::retrobat_running
    }
}

/// A note for game lists changed on a Batocera: a running Batocera shows them after reloading.
fn batocera_note(connection: &rusqlite::Connection, installation_id: &str) -> &'static str {
    let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [installation_id], |r| r.get(0)).unwrap_or_default();
    if adapter == "batocera" {
        " If Batocera is running, press Start → Game Settings → Update Gamelists to see them."
    } else if adapter == "retropie" {
        " Restart RetroPie (or EmulationStation) to see them."
    } else if adapter == "recalbox" {
        " If Recalbox is running, update the game lists from its menu (or restart it) to see them."
    } else if adapter == "rocknix" {
        " Restart ROCKNIX (or its EmulationStation) to see them."
    } else {
        ""
    }
}

/// What adding game art and details to the game lists would do (reads only).
#[tauri::command]
async fn preview_retrobat_art(app: AppHandle, catalog: State<'_, Catalog>, installation_id: String, create_lists: Option<bool>) -> Result<gamelists::ArtPreview, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        gamelists::preview(&db::open(&db_path)?, &installation_id, None, create_lists.unwrap_or(false), &volumes::presence, &|phase, done, total| {
            let _ = app.emit("art-progress", ArtProgress { phase: phase.to_string(), done, total });
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize, Clone)]
struct ArtProgress {
    /// `checking` (games) or `copying` (images).
    phase: String,
    done: usize,
    total: usize,
}

/// Add the user's art and catalog details to RetroBat's game lists (reviewed; undoable). Emits `art-progress`.
#[tauri::command]
async fn apply_retrobat_art(app: AppHandle, catalog: State<'_, Catalog>, active: State<'_, ActiveJobs>, installation_id: String, list_shas: std::collections::BTreeMap<String, String>, create_lists: Option<bool>) -> Result<String, String> {
    claim_drive_change(&active)?;
    let db_path = catalog.path()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        let message = gamelists::apply(&connection, &installation_id, None, Some(&list_shas), create_lists.unwrap_or(false), &volumes::presence, running_for(&connection, &installation_id), &|phase, done, total| {
            let _ = app.emit("art-progress", ArtProgress { phase: phase.to_string(), done, total });
        })?;
        Ok::<String, String>(format!("{message}{}", batocera_note(&connection, &installation_id)))
    })
    .await
    .map_err(|e| e.to_string());
    active.undoing.store(false, Ordering::SeqCst);
    result?
}

/// Games and size per platform for the library's Systems page.
#[tauri::command]
async fn system_summary(catalog: State<'_, Catalog>) -> Result<Vec<catalog::SystemSummary>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || catalog::system_summary(&db::open(&db_path)?)).await.map_err(|e| e.to_string())?
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
        return Err("Another scan or copy is running. Wait for it to finish or cancel it first.".into());
    }
    if active.importing.load(Ordering::SeqCst) {
        return Err(BUSY_IMPORTING.into());
    }
    if active.undoing.load(Ordering::SeqCst) {
        return Err(BUSY_UNDOING.into());
    }
    let connection = db::open(&db_path)?;
    let job_id = jobs::create(&connection, jobs::KIND_SCAN, &json!({ "folder": folder }))?;
    drop(connection);

    let cancel = Arc::new(AtomicBool::new(false));
    running.insert(job_id.clone(), ActiveJob { cancel: cancel.clone(), folder: folder.clone(), kind: jobs::KIND_SCAN });
    drop(running);

    let id = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || run_scan(app, db_path, id, folder, cancel));
    Ok(job_id)
}

fn run_scan(app: AppHandle, db_path: PathBuf, job_id: String, folder: String, cancel: Arc<AtomicBool>) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let connection = db::open(&db_path)?;
        let mut observer = UiObserver { app: app.clone(), job_id: job_id.clone(), cancel };
        let summary = scanner::scan_with(&connection, Path::new(&folder), &volumes::probe, &mut observer, scanner::ScanOptions::default())?;
        // Link newly found games to the reference catalog, if one is imported.
        if let Err(e) = reference::match_games(&connection, false) {
            eprintln!("Reference matching after the scan failed: {e}");
        }
        // Refresh table statistics when they're stale, so library pages stay fast as it grows.
        if let Err(e) = connection.execute_batch("PRAGMA optimize;") {
            eprintln!("Optimizing the catalog after the scan failed: {e}");
        }
        Ok(summary)
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
    Ok(running.iter().next().map(|(id, job)| ActiveScan { job_id: id.clone(), folder: job.folder.clone(), kind: job.kind }))
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

/// The saved installations' folders (to mark systems the wizard finds as already added).
fn saved_paths(db_path: &Path) -> Vec<String> {
    db::open(db_path).and_then(|c| installations::list(&c, &volumes::presence)).map(|rows| rows.into_iter().map(|r| r.path).collect()).unwrap_or_default()
}

/// Setup wizard (ADR-100): emulation systems on the connected drives (reads only).
#[tauri::command]
async fn find_systems_on_drives(catalog: State<'_, Catalog>) -> Result<Vec<find_systems::Found>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<PathBuf> = drives::list().into_iter().map(|d| PathBuf::from(d.mount_point)).collect();
        // An installed (not portable) ES-DE reads games from %USERPROFILE%\ROMs.
        let extra: Vec<PathBuf> = std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("ROMs")).into_iter().collect();
        find_systems::on_drives(&roots, &extra, &saved_paths(&db_path))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Setup wizard: emulation systems answering on their usual network names (reads only;
/// names that don't answer within 8 seconds are left out).
#[tauri::command]
async fn find_systems_on_network(catalog: State<'_, Catalog>) -> Result<Vec<find_systems::Found>, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || find_systems::on_network(find_systems::NETWORK_NAMES, &saved_paths(&db_path), std::time::Duration::from_secs(8)))
        .await
        .map_err(|e| e.to_string())
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

/// Choose MAME or FinalBurn Neo (or neither) for an installation's arcade games.
#[tauri::command]
fn set_arcade_system(catalog: State<'_, Catalog>, installation_id: String, system: Option<String>) -> Result<(), String> {
    let connection = db::open(&catalog.path()?)?;
    installations::set_arcade_system(&connection, &installation_id, system.as_deref())
}

/// Deployment plan for an installation (RDS-009): actions, conflicts and capacity.
/// `systems` limits it to those system folders; `None` plans all. Nothing is written.
#[tauri::command]
async fn deployment_plan(
    catalog: State<'_, Catalog>,
    installation_id: String,
    systems: Option<Vec<String>>,
    choices: Option<deployment::Choices>,
    collection_id: Option<String>,
) -> Result<deployment::DeploymentPlan, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&db_path)?;
        deployment::plan_with(&connection, &installation_id, systems.as_deref(), collection_id.as_deref(), &choices.unwrap_or_default(), &volumes::presence, &volumes::space)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The Sync plan (ADR-043): every platform except those left out of Sync. Nothing is written.
#[tauri::command]
async fn sync_plan(catalog: State<'_, Catalog>, installation_id: String, choices: Option<deployment::Choices>) -> Result<deployment::DeploymentPlan, String> {
    let db_path = catalog.path()?;
    tauri::async_runtime::spawn_blocking(move || deployment::plan_sync(&db::open(&db_path)?, &installation_id, &choices.unwrap_or_default(), &volumes::presence, &volumes::space))
        .await
        .map_err(|e| e.to_string())?
}

/// What Sync keeps up to date on this installation (ADR-045).
#[tauri::command]
fn sync_scope(catalog: State<'_, Catalog>, installation_id: String) -> Result<installations::SyncScope, String> {
    deployment::sync_scope(&db::open(&catalog.path()?)?, &installation_id, &volumes::presence)
}

/// Saves what Sync keeps up to date (catalog only; "stop syncing" never touches the drive).
#[tauri::command]
fn set_sync_scope(catalog: State<'_, Catalog>, installation_id: String, scope: installations::SyncScope) -> Result<(), String> {
    installations::set_sync_scope(&db::open(&catalog.path()?)?, &installation_id, &scope)
}

/// The user's saved decisions for this installation (ADR-044).
#[tauri::command]
fn saved_choices(catalog: State<'_, Catalog>, installation_id: String) -> Result<deployment::Choices, String> {
    installations::saved_choices(&db::open(&catalog.path()?)?, &installation_id)
}

/// Saves the user's decisions (catalog only; "replace" is never saved).
#[tauri::command]
fn set_saved_choices(catalog: State<'_, Catalog>, installation_id: String, choices: deployment::Choices) -> Result<(), String> {
    installations::set_saved_choices(&db::open(&catalog.path()?)?, &installation_id, &choices)
}

/// Remembers (or forgets) that a game removed from the drive stays off it (catalog only).
#[tauri::command]
fn set_kept_off(catalog: State<'_, Catalog>, installation_id: String, destination: String, title: Option<String>, kept: bool) -> Result<(), String> {
    installations::set_kept_off(&db::open(&catalog.path()?)?, &installation_id, &destination, title.as_deref(), kept)
}

/// Forwards copy progress to the UI and relays cancellation requests.
struct DeployUi {
    app: AppHandle,
    job_id: String,
    cancel: Arc<AtomicBool>,
}

impl copier::DeployObserver for DeployUi {
    fn cancelled(&mut self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn progress(&mut self, progress: &copier::DeployProgress) {
        let _ = self.app.emit(EVENT_DEPLOY_PROGRESS, json!({ "job_id": self.job_id, "progress": progress }));
    }
}

#[derive(Serialize, Clone)]
struct DeployFinishedEvent {
    job_id: String,
    /// `completed`, `completed_with_issues`, `cancelled`, `interrupted` or `failed`.
    state: String,
    summary: Option<copier::DeploySummary>,
    error: Option<String>,
    /// What happened with game art and details after the copy (RetroBat).
    art: Option<String>,
    /// What happened with controller setups after the copy (the user ticked it).
    controllers: Option<String>,
}

/// Copy the reviewed plan to an installation (RDS-010). `fingerprint` must match
/// the plan the user reviewed; everything is re-checked before the first write.
/// Runs in the background; progress arrives as `deploy-progress`, the result as
/// `deploy-finished`. Only one scan or copy runs at a time.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri commands take their arguments flat.
fn start_deployment(
    app: AppHandle,
    catalog: State<'_, Catalog>,
    active: State<'_, ActiveJobs>,
    installation_id: String,
    systems: Option<Vec<String>>,
    fingerprint: String,
    destination: String,
    choices: Option<deployment::Choices>,
    collection_id: Option<String>,
    add_art: Option<bool>,
    sync: Option<bool>,
    setup_controllers: Option<bool>,
) -> Result<String, String> {
    let db_path = catalog.path()?;
    let mut running = active.jobs.lock().map_err(|_| "Job registry unavailable".to_string())?;
    if !running.is_empty() {
        return Err("Another scan or copy is running. Wait for it to finish or cancel it first.".into());
    }
    if active.importing.load(Ordering::SeqCst) {
        return Err(BUSY_IMPORTING.into());
    }
    if active.undoing.load(Ordering::SeqCst) {
        return Err(BUSY_UNDOING.into());
    }
    let connection = db::open(&db_path)?;
    let input = json!({ "folder": destination, "installation_id": installation_id, "systems": systems, "fingerprint": fingerprint });
    let job_id = jobs::create(&connection, jobs::KIND_DEPLOY, &input)?;
    drop(connection);

    let cancel = Arc::new(AtomicBool::new(false));
    running.insert(job_id.clone(), ActiveJob { cancel: cancel.clone(), folder: destination, kind: jobs::KIND_DEPLOY });
    drop(running);

    let id = job_id.clone();
    let choices = choices.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || run_deployment(app, db_path, id, installation_id, systems, fingerprint, choices, collection_id, add_art.unwrap_or(false), sync.unwrap_or(false), setup_controllers.unwrap_or(false), cancel));
    Ok(job_id)
}

#[allow(clippy::too_many_arguments)]
fn run_deployment(
    app: AppHandle,
    db_path: PathBuf,
    job_id: String,
    installation_id: String,
    systems: Option<Vec<String>>,
    fingerprint: String,
    choices: deployment::Choices,
    collection_id: Option<String>,
    add_art: bool,
    sync: bool,
    setup_controllers: bool,
    cancel: Arc<AtomicBool>,
) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let connection = db::open(&db_path)?;
        let mut observer = DeployUi { app: app.clone(), job_id: job_id.clone(), cancel };
        let request = copier::DeployRequest { installation_id: &installation_id, systems: systems.as_deref(), fingerprint: &fingerprint, choices: &choices, collection: collection_id.as_deref(), sync };
        copier::deploy(&connection, Some(&job_id), &request, &volumes::presence, &volumes::space, &mut observer)
    }))
    .unwrap_or_else(|_| Err("The copy stopped because of an internal error. Files already verified were kept; unfinished temporary files are cleaned up next time.".into()));

    let (state, summary, error) = match outcome {
        Ok(summary) => (summary.status.clone(), Some(summary), None),
        Err(error) => ("failed".to_string(), None, Some(error)),
    };
    // A copy from a plan: Sync keeps what it covered up to date from now on (ADR-045).
    if let Some(s) = &summary {
        if !sync && matches!(s.status.as_str(), "completed" | "completed_with_issues" | "cancelled") && s.files_copied + s.files_replaced + s.files_identical > 0 {
            if let Ok(connection) = db::open(&db_path) {
                let _ = deployment::add_to_sync_scope(&connection, &installation_id, systems.as_deref().unwrap_or(&[]), collection_id.as_deref(), &volumes::presence);
            }
        }
    }
    // Game art and details for the games just copied (RetroBat; the user ticked it when confirming).
    let art = match (&summary, add_art) {
        (Some(s), true) if matches!(s.status.as_str(), "completed" | "completed_with_issues" | "cancelled") && s.files_copied + s.files_replaced + s.files_identical > 0 => {
            db::open(&db_path).ok().and_then(|connection| {
                let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [&installation_id], |r| r.get(0)).unwrap_or_default();
                gamelists::GAMELIST_PLATFORMS.contains(&adapter.as_str()).then(|| match gamelists::apply(&connection, &installation_id, Some(&s.deployment_id), None, false, &volumes::presence, running_for(&connection, &installation_id), &|_, _, _| {}) {
                    Ok(message) => format!("{message}{}", batocera_note(&connection, &installation_id)),
                    // Nothing missing isn't a failure.
                    Err(e) if e.starts_with("There's no art or detail to add") => "Nothing new to add: these games already have their art and details.".to_string(),
                    Err(e) => format!("Game art wasn't added: {e} You can add it later under Customize."),
                })
            })
        }
        _ => None,
    };
    // Lakka shows games only through playlists, so copied games are always added (ADR-086).
    let art = match &summary {
        Some(s) if matches!(s.status.as_str(), "completed" | "completed_with_issues" | "cancelled") && s.files_copied + s.files_replaced + s.files_identical > 0 => db::open(&db_path)
            .ok()
            .and_then(|connection| {
                let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [&installation_id], |r| r.get(0)).unwrap_or_default();
                (adapter == adapters::lakka::ID).then(|| match lakka_playlists::apply(&connection, &installation_id, Some(&s.deployment_id), None, &volumes::presence) {
                    Ok(message) => message,
                    Err(e) if e.starts_with("Nothing new") => "These games are already in Lakka's playlists.".to_string(),
                    Err(e) => format!("The games weren't added to Lakka's playlists: {e} You can add them later under Customize."),
                })
            })
            .or(art),
        _ => art,
    };
    // Controller setups (the user ticked it when confirming; ADR-065). Not tied to the
    // games, so it runs whenever the copy itself didn't fail.
    let controllers = match (&summary, setup_controllers) {
        (Some(s), true) if matches!(s.status.as_str(), "completed" | "completed_with_issues" | "cancelled") => db::open(&db_path).ok().and_then(|connection| match controller_setup::apply_for_installation(&connection, &installation_id, &volumes::presence) {
            Ok(message) => message,
            Err(e) => Some(format!("Your controller wasn't set up: {e} You can do it later on the Controllers page.")),
        }),
        _ => None,
    };
    if let Ok(connection) = db::open(&db_path) {
        if let Some(s) = &summary {
            for issue in &s.issues {
                let _ = jobs::event(&connection, &job_id, "warning", "deploy_issue", &json!({ "game": issue.game, "path": issue.path, "message": issue.message }));
            }
        }
        // Job states stay within the job vocabulary; the detailed status is in the summary.
        let job_state = match state.as_str() {
            "completed" | "completed_with_issues" => "completed",
            "cancelled" => "cancelled",
            _ => "failed",
        };
        let progress = summary.as_ref().and_then(|s| serde_json::to_value(s).ok());
        let failure = error.as_deref().map(|e| ("deploy_refused", e)).or_else(|| (state == "interrupted").then_some(("drive_lost", "The destination drive was disconnected during the copy.")));
        if let Err(e) = jobs::finish(&connection, &job_id, job_state, progress.as_ref(), failure) {
            eprintln!("Could not record the end of job {job_id}: {e}");
        }
    }
    if let Ok(mut running) = app.state::<ActiveJobs>().jobs.lock() {
        running.remove(&job_id);
    }
    let _ = app.emit(EVENT_DEPLOY_FINISHED, DeployFinishedEvent { job_id, state, summary, error, art, controllers });
}

#[tauri::command]
fn list_drives() -> Vec<drives::Drive> {
    drives::list()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("rdmedia", |ctx, request, responder| {
            let db_path = ctx.app_handle().state::<Catalog>().path();
            let uri_path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(serve_media(db_path, &uri_path)));
        })
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
            theme_studio::commands::import_theme_png,
            theme_studio::commands::preview_theme_png,
            theme_studio::commands::prepare_theme_bake,
            theme_studio::commands::theme_save_destination,
            theme_studio::commands::save_theme_for_retrobat,
            theme_studio::commands::open_saved_theme,
            theme_studio::commands::import_theme_video,
            ai_status,
            ai_open_keys_page,
            ai_connect_claude,
            ai_disconnect_claude,
            ai_set_claude_model,
            ai_find_private,
            ai_private_models,
            ai_connect_private,
            ai_disconnect_private,
            ai_suggest_design,
            ai_open_service_keys,
            ai_check_service,
            ai_connect_service,
            ai_service_models,
            ai_set_service_model,
            ai_disconnect_service,
            ai_design_preview,
            put_theme_on_retrobat,
            theme_studio::commands::theme_target_profiles,
            theme_studio::commands::theme_compatibility,
            theme_studio::commands::validate_theme_document,
            theme_studio::commands::list_theme_projects,
            theme_studio::commands::open_theme_project,
            theme_studio::commands::create_theme_project,
            theme_studio::commands::write_theme_project,
            theme_studio::commands::rename_theme_project,
            theme_studio::commands::duplicate_theme_project,
            theme_studio::commands::recover_theme_project,
            theme_studio::commands::discard_theme_project_draft,
            query_games,
            library_stats,
            list_adapters,
            inspect_installation,
            add_installation,
            list_installations,
            find_systems_on_drives,
            find_systems_on_network,
            remove_installation,
            deployment_plan,
            system_summary,
            list_backups,
            preview_retrobat_art,
            apply_retrobat_art,
            list_designs,
            create_design,
            save_design_draft,
            finish_design,
            abandon_design,
            delete_design,
            music_settings,
            preview_design,
            apply_design,
            design_applications,
            undo_design_application,
            look_options,
            preview_look,
            apply_look,
            add_retrobat_media,
            list_themes,
            theme_preview_asset,
            create_theme_remix,
            preview_theme_switch,
            apply_theme_switch,
            emulator_settings,
            preview_emulator_settings,
            apply_emulator_settings,
            emulator_settings_history,
            undo_emulator_settings,
            undo_replace,
            open_backup_folder,
            list_collections,
            set_favorite,
            move_collection,
            create_smart_collection,
            library_genres,
            create_collection,
            rename_collection,
            delete_collection,
            add_to_collection,
            remove_from_collection,
            set_left_out,
            duplicate_cleanup_preview,
            clean_up_duplicates,
            import_reference,
            reference_status,
            remove_reference,
            set_game_art,
            remove_game_art,
            set_arcade_system,
            sync_plan,
            sync_scope,
            set_sync_scope,
            batocera_options,
            recalbox_options,
            game_groups_state,
            preview_game_groups,
            apply_game_groups,
            preview_recalbox_settings,
            apply_recalbox_settings,
            recalbox_games,
            check_bios,
            backup_destination_path,
            open_third_party_notices,
            check_health,
            system_backup_preview,
            run_system_backup,
            list_system_backups,
            check_system_backup,
            open_system_backups,
            save_backup_zip,
            library_folders,
            catalog_removal_preview,
            remove_catalog_entries,
            export_library,
            show_exported_library,
            catalog_reset_preview,
            saved_catalogs,
            reset_catalog,
            put_back_catalog,
            open_catalog_folder,
            add_backup_zip,
            restore_preview,
            apply_restore,
            list_restores,
            undo_restore,
            preview_lakka_playlists,
            apply_lakka_playlists,
            recalbox_game_options,
            recalbox_games_with_settings,
            preview_recalbox_game,
            apply_recalbox_game,
            rocknix_options,
            rocknix_games,
            retrobat_game_settings,
            batocera_games,
            preview_rocknix_settings,
            apply_rocknix_settings,
            retropie_options,
            connected_controllers,
            remember_controller_type,
            controller_setup_preview,
            apply_controller_setup,
            controllers_for_installation,
            light_guns,
            remember_light_gun,
            sinden_status,
            preview_sinden_step,
            apply_sinden_step,
            find_controllers,
            controller_reference,
            preview_bios_step,
            apply_bios_step,
            preview_retropie_emulators,
            apply_retropie_emulators,
            install_image_info,
            install_checksum,
            install_writers,
            open_install_writer,
            open_download_page,
            preview_batocera_settings,
            apply_batocera_settings,
            preview_batocera_theme,
            apply_batocera_theme,
            set_kept_off,
            saved_choices,
            set_saved_choices,
            start_deployment,
            list_storage_locations, start_scan, cancel_job, active_scan, recent_jobs, list_drives])
        .run(tauri::generate_context!())
        .expect("error starting RetroDrive");
}
