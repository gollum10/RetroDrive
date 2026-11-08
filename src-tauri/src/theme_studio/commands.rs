//! Narrow IPC: every operation opens only RetroDrive's managed catalog.
use super::projects::{
    self, ProjectError, ProjectRecord, ProjectSummary, RecoveredDraft, WriteKind,
};
use crate::{db, Catalog};
use rusqlite::Connection;
use tauri::State;

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
