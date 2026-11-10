//! Narrow IPC: every operation opens only RetroDrive's managed catalog.
use super::projects::{
    self, ProjectError, ProjectRecord, ProjectSummary, RecoveredDraft, WriteKind,
};
use crate::{db, Catalog};
use rusqlite::Connection;
use tauri::State;

#[tauri::command]
pub(crate) async fn theme_target_profiles(
    catalog: State<'_, Catalog>,
) -> Result<Vec<super::capabilities::TargetProfile>, ProjectError> {
    run(catalog, |c| {
        super::capabilities::discover(c, &crate::volumes::presence)
            .map_err(|e| projects::error_message("discovery", &e))
    })
    .await
}

#[tauri::command]
pub(crate) async fn theme_compatibility(
    catalog: State<'_, Catalog>,
    document_json: String,
    installation_id: Option<String>,
) -> Result<super::capabilities::CompatibilityReport, ProjectError> {
    run(catalog, move |c| {
        let project = super::validation::parse_project(&document_json).map_err(|issues| {
            projects::error_message("invalid_document", &format!("{issues:?}"))
        })?;
        let profile = match installation_id {
            None => super::capabilities::intent(&project),
            Some(id) => super::capabilities::discover(c, &crate::volumes::presence)
                .map_err(|e| projects::error_message("discovery", &e))?
                .into_iter()
                .find(|p| p.installation_id.as_deref() == Some(&id))
                .ok_or_else(|| {
                    projects::error_message("not_found", "That theme target is no longer saved.")
                })?,
        };
        let adapters = super::adapters::registry();
        let refs: Vec<_> = adapters.iter().map(|a| a.as_ref()).collect();
        Ok(super::capabilities::analyze(&project, profile, &refs))
    })
    .await
}

#[tauri::command]
pub(crate) async fn validate_theme_document(
    document_json: String,
) -> Result<super::model::ThemeProject, Vec<super::validation::Issue>> {
    tauri::async_runtime::spawn_blocking(move || super::validation::parse_project(&document_json))
        .await
        .map_err(|e| {
            vec![super::validation::Issue {
                code: "worker".into(),
                path: "$".into(),
                message: e.to_string(),
            }]
        })?
}

async fn run<T: Send + 'static>(
    catalog: State<'_, Catalog>,
    operation: impl FnOnce(&Connection) -> projects::Result<T> + Send + 'static,
) -> projects::Result<T> {
    let path = catalog
        .path()
        .map_err(|e| projects::error_message("catalog", &e))?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = db::open(&path).map_err(|e| projects::error_message("storage", &e))?;
        operation(&connection)
    })
    .await
    .map_err(|e| projects::error_message("worker", &e.to_string()))?
}

#[tauri::command]
pub(crate) async fn list_theme_projects(
    catalog: State<'_, Catalog>,
    limit: u32,
    offset: u32,
) -> Result<Vec<ProjectSummary>, ProjectError> {
    run(catalog, move |c| projects::list(c, limit, offset)).await
}

#[tauri::command]
pub(crate) async fn open_theme_project(
    catalog: State<'_, Catalog>,
    id: String,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| projects::open(c, &id)).await
}

#[tauri::command]
pub(crate) async fn create_theme_project(
    catalog: State<'_, Catalog>,
    document_json: String,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| projects::create(c, &document_json)).await
}

#[tauri::command]
pub(crate) async fn write_theme_project(
    catalog: State<'_, Catalog>,
    id: String,
    revision: i64,
    document_json: String,
    kind: WriteKind,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| {
        projects::write(c, &id, revision, &document_json, kind)
    })
    .await
}

#[tauri::command]
pub(crate) async fn rename_theme_project(
    catalog: State<'_, Catalog>,
    id: String,
    revision: i64,
    name: String,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| projects::rename(c, &id, revision, &name)).await
}

#[tauri::command]
pub(crate) async fn duplicate_theme_project(
    catalog: State<'_, Catalog>,
    id: String,
    revision: i64,
    name: String,
    use_draft: bool,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| {
        projects::duplicate(c, &id, revision, &name, use_draft)
    })
    .await
}

#[tauri::command]
pub(crate) async fn recover_theme_project(
    catalog: State<'_, Catalog>,
    id: String,
    revision: i64,
) -> Result<RecoveredDraft, ProjectError> {
    run(catalog, move |c| projects::recover(c, &id, revision)).await
}

#[tauri::command]
pub(crate) async fn discard_theme_project_draft(
    catalog: State<'_, Catalog>,
    id: String,
    revision: i64,
) -> Result<ProjectRecord, ProjectError> {
    run(catalog, move |c| projects::discard_draft(c, &id, revision)).await
}

// Do not queue unlimited image decode/preview work; callers retry a busy response.
static IMAGE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// One picture operation at a time. Requests wait their turn (they already run off the UI
/// thread) instead of failing as "busy", and a lock left behind by a failed operation is
/// recovered rather than blocking pictures for the rest of the session (ADR-113).
fn image_gate() -> std::sync::MutexGuard<'static, ()> {
    IMAGE_GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
#[tauri::command]
pub(crate) async fn import_theme_png(catalog: State<'_,Catalog>,project_id: String,png_base64: String) -> Result<super::image_store::StoredImage,ProjectError> {
    run(catalog,move |connection| { let _guard=image_gate(); super::image_store::import_png(connection,&project_id,&png_base64) }).await
}
#[tauri::command]
pub(crate) async fn preview_theme_png(catalog: State<'_,Catalog>,project_id: String,asset_id: String) -> Result<String,ProjectError> {
    run(catalog,move |connection| { let _guard=image_gate(); super::image_store::preview_png(connection,&project_id,&asset_id) }).await
}

/// Save for RetroBat (ADR-113): check a baked picture and say how to declare it (nothing stored).
#[tauri::command]
pub(crate) async fn prepare_theme_bake(png_base64: String) -> Result<super::export::save::BakedInfo, ProjectError> {
    tauri::async_runtime::spawn_blocking(move || super::export::save::prepare_baked(&png_base64))
        .await
        .map_err(|e| projects::error_message("worker", &e.to_string()))?
}

/// Where saved themes go unless the user picks another folder: `Documents\RetroDrive Themes`.
pub(crate) fn theme_destination(app: &tauri::AppHandle, folder: Option<String>) -> Result<std::path::PathBuf, ProjectError> {
    use tauri::Manager;
    match folder.filter(|f| !f.trim().is_empty()) {
        Some(f) => {
            let path = std::path::PathBuf::from(f);
            if !path.is_absolute() {
                return Err(projects::error_message("destination", "Choose a full folder location."));
            }
            Ok(path)
        }
        None => Ok(app
            .path()
            .document_dir()
            .map_err(|e| projects::error_message("destination", &format!("Couldn't find your Documents folder: {e}")))?
            .join("RetroDrive Themes")),
    }
}

#[tauri::command]
pub(crate) fn theme_save_destination(app: tauri::AppHandle, folder: Option<String>) -> Result<String, ProjectError> {
    Ok(theme_destination(&app, folder)?.display().to_string())
}

/// Save for RetroBat (ADR-113, docs/18 §67): build the theme and write it to a new folder.
#[tauri::command]
pub(crate) async fn save_theme_for_retrobat(
    app: tauri::AppHandle,
    catalog: State<'_, Catalog>,
    project_id: String,
    revision: i64,
    export_json: String,
    baked: Vec<String>,
    folder: Option<String>,
) -> Result<super::export::save::SavedTheme, ProjectError> {
    let destination = theme_destination(&app, folder)?;
    run(catalog, move |connection| {
        let files = super::export::save::build(connection, &project_id, revision, &export_json, &baked)?;
        let name = super::validation::parse_project(&export_json)
            .map(|p| p.metadata.name)
            .unwrap_or_default();
        super::export::save::write_new_folder(&destination, &name, &files)
            .map_err(|e| projects::error_message("write", &e))
    })
    .await
}

/// Show a saved theme in File Explorer (only folders RetroDrive saved: they hold its file list).
#[tauri::command]
pub(crate) fn open_saved_theme(folder: String) -> Result<(), ProjectError> {
    let path = std::path::PathBuf::from(&folder);
    if !path.is_absolute() || !path.join("retrodrive-theme.json").is_file() {
        return Err(projects::error_message("not_found", "That isn't a theme RetroDrive saved."));
    }
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| projects::error_message("open", &format!("Couldn't open File Explorer: {e}")))
}

