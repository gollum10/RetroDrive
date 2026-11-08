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
