//! RetroDrive native backend: Tauri commands over the catalog services.
//! The frontend is unprivileged; every command validates its own inputs.

mod adapters;
mod batocera_settings;
mod bios_step;
mod catalog;
mod classify;
mod collections;
mod controller_setup;
mod controllers;
mod customizer;
mod designs;
mod copier;
mod db;
mod deployment;
mod drives;
mod gamelists;
mod duplicates;
mod install_helper;
mod installations;
mod jobs;
mod media;
mod paths;
mod reference;
mod restore;
mod retropie_settings;
mod themes;
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

#[tauri::command]
fn list_collections(catalog: State<'_, Catalog>) -> Result<Vec<collections::Collection>, String> {
    collections::list(&db::open(&catalog.path()?)?)
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
    if adapter == "batocera" || adapter == "retropie" || adapter == "recalbox" {
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
            query_games,
            library_stats,
            list_adapters,
            inspect_installation,
            add_installation,
            list_installations,
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
            retropie_options,
            connected_controllers,
            remember_controller_type,
            controller_setup_preview,
            apply_controller_setup,
            controllers_for_installation,
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
