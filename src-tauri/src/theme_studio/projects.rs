//! Authoritative local project storage. Only the supplied catalog is mutated.
use super::{model::ThemeProject, validation::parse_project};
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectError {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, ProjectError>;

impl From<rusqlite::Error> for ProjectError {
    fn from(error: rusqlite::Error) -> Self {
        error_message("storage", &error.to_string())
    }
}

pub(crate) fn error_message(code: &str, message: &str) -> ProjectError {
    ProjectError {
        code: code.into(),
        message: message.into(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub schema_version: String,
    /// Return original stored source, even if a later build cannot edit its schema.
    pub saved_json: String,
    pub draft_json: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub saved_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub schema_version: String,
    pub revision: i64,
    pub has_draft: bool,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredDraft {
    pub id: String,
    pub document_json: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteKind {
    Autosave,
    Checkpoint,
}

fn check_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(error_message("invalid_id", "Invalid project identifier."));
    }
    Ok(())
}

fn check_revision(revision: i64) -> Result<()> {
    if !(0..9_007_199_254_740_991).contains(&revision) {
        return Err(error_message(
            "invalid_revision",
            "Revision is outside the writable range.",
        ));
    }
    Ok(())
}

fn normalized(json: &str) -> Result<ThemeProject> {
    let mut project = parse_project(json).map_err(|issues| {
        error_message(
            "invalid_document",
            &issues
                .iter()
                .map(|i| format!("{}: {}", i.code, i.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    project.metadata.name = project
        .metadata
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(project)
}

fn encoded(project: &ThemeProject) -> Result<String> {
    serde_json::to_string(project).map_err(|e| error_message("invalid_document", &e.to_string()))
}

pub fn open(connection: &Connection, id: &str) -> Result<ProjectRecord> {
    check_id(id)?;
    connection.query_row(
        "SELECT id,name,schema_version,saved_json,draft_json,revision,created_at,updated_at,saved_at FROM theme_projects WHERE id=?1", [id],
        |r| Ok(ProjectRecord { id:r.get(0)?, name:r.get(1)?, schema_version:r.get(2)?, saved_json:r.get(3)?, draft_json:r.get(4)?, revision:r.get(5)?, created_at:r.get(6)?, updated_at:r.get(7)?, saved_at:r.get(8)? })
    ).optional()?.ok_or_else(|| error_message("not_found", "That theme project was not found."))
}

pub fn list(connection: &Connection, limit: u32, offset: u32) -> Result<Vec<ProjectSummary>> {
    if !(1..=100).contains(&limit) {
        return Err(error_message("invalid_page", "Page size must be 1–100."));
    }
    let mut query = connection.prepare("SELECT id,name,schema_version,revision,draft_json IS NOT NULL,updated_at FROM theme_projects ORDER BY updated_at DESC,id LIMIT ?1 OFFSET ?2")?;
    let rows = query.query_map(params![limit, offset], |r| {
        Ok(ProjectSummary {
            id: r.get(0)?,
            name: r.get(1)?,
            schema_version: r.get(2)?,
            revision: r.get(3)?,
            has_draft: r.get(4)?,
            updated_at: r.get(5)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn insert(connection: &Connection, mut project: ThemeProject) -> Result<ProjectRecord> {
    project.id = db::new_id();
    let json = encoded(&project)?;
    connection.execute(
        "INSERT INTO theme_projects(id,name,schema_version,saved_json) VALUES(?1,?2,?3,?4)",
        params![
            project.id,
            project.metadata.name,
            project.schema_version,
            json
        ],
    )?;
    open(connection, &project.id)
}

/// Client supplies portable source; identity is always generated locally.
pub fn create(connection: &Connection, document_json: &str) -> Result<ProjectRecord> {
    let project = normalized(document_json)?;
    let tx = connection.unchecked_transaction()?;
    let result = insert(&tx, project)?;
    tx.commit()?;
    Ok(result)
}

fn expected(connection: &Connection, id: &str, revision: i64) -> Result<ProjectRecord> {
    check_revision(revision)?;
    let record = open(connection, id)?;
    if record.revision != revision {
        return Err(error_message("conflict", "Project changed since it was read. Reopen it before saving; your newer source was kept."));
    }
    Ok(record)
}

fn updated(connection: &Connection, id: &str, revision: i64, count: usize) -> Result<()> {
    if count != 1 {
        expected(connection, id, revision)?;
        return Err(error_message("conflict", "Project could not be updated."));
    }
    Ok(())
}

/// Compare-and-swap happens inside the SQL write, not just in a preceding read.
pub fn write(
    connection: &Connection,
    id: &str,
    revision: i64,
    document_json: &str,
    kind: WriteKind,
) -> Result<ProjectRecord> {
    check_id(id)?;
    check_revision(revision)?;
    let project = normalized(document_json)?;
    if project.id != id {
        return Err(error_message(
            "identity_mismatch",
            "The source belongs to another project.",
        ));
    }
    let json = encoded(&project)?;
    let tx = connection.unchecked_transaction()?;
    let sql = match kind {
        WriteKind::Autosave => "UPDATE theme_projects SET name=?3,schema_version=?4,draft_json=?5,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND revision=?2 AND schema_version=?4",
        WriteKind::Checkpoint => "UPDATE theme_projects SET name=?3,schema_version=?4,saved_json=?5,draft_json=NULL,revision=revision+1,updated_at=CURRENT_TIMESTAMP,saved_at=CURRENT_TIMESTAMP WHERE id=?1 AND revision=?2 AND schema_version=?4",
    };
    let count = tx.execute(
        sql,
        params![
            id,
            revision,
            project.metadata.name,
            project.schema_version,
            json
        ],
    )?;
    updated(&tx, id, revision, count)?;
    let result = open(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

/// Selecting recovery returns a validated draft without promoting the checkpoint.
pub fn recover(connection: &Connection, id: &str, revision: i64) -> Result<RecoveredDraft> {
    let record = expected(connection, id, revision)?;
    let draft = record
        .draft_json
        .ok_or_else(|| error_message("no_draft", "There is no autosaved draft to recover."))?;
    normalized(&draft)?;
    Ok(RecoveredDraft {
        id: record.id,
        document_json: draft,
        revision: record.revision,
    })
}

pub fn discard_draft(connection: &Connection, id: &str, revision: i64) -> Result<ProjectRecord> {
    let tx = connection.unchecked_transaction()?;
    let record = expected(&tx, id, revision)?;
    // Do not clear an uneditable/future draft by treating it as a default project.
    normalized(&record.saved_json)?;
    if let Some(draft) = &record.draft_json {
        normalized(draft)?;
    }
    let count = tx.execute("UPDATE theme_projects SET name=json_extract(saved_json,'$.metadata.name'),draft_json=NULL,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND revision=?2",params![id,revision])?;
    updated(&tx, id, revision, count)?;
    let result = open(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

/// Explicit rename updates only names in checkpoint and draft, preserving draft edits.
pub fn rename(
    connection: &Connection,
    id: &str,
    revision: i64,
    name: &str,
) -> Result<ProjectRecord> {
    let tx = connection.unchecked_transaction()?;
    let record = expected(&tx, id, revision)?;
    let rename_json = |json: &str| -> Result<String> {
        let mut project = normalized(json)?;
        project.metadata.name = name.into();
        encoded(&normalized(&encoded(&project)?)?)
    };
    let saved = rename_json(&record.saved_json)?;
    let draft = record.draft_json.as_deref().map(rename_json).transpose()?;
    let active = normalized(draft.as_deref().unwrap_or(&saved))?;
    let count = tx.execute("UPDATE theme_projects SET name=?3,saved_json=?4,draft_json=?5,revision=revision+1,updated_at=CURRENT_TIMESTAMP,saved_at=CURRENT_TIMESTAMP WHERE id=?1 AND revision=?2",params![id,revision,active.metadata.name,saved,draft])?;
    updated(&tx, id, revision, count)?;
    let result = open(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

/// Save As/duplicate explicitly chooses checkpoint or draft; original stays unchanged.
pub fn duplicate(
    connection: &Connection,
    id: &str,
    revision: i64,
    name: &str,
    use_draft: bool,
) -> Result<ProjectRecord> {
    let tx = connection.unchecked_transaction()?;
    let record = expected(&tx, id, revision)?;
    let source = if use_draft {
        record.draft_json.as_deref().ok_or_else(|| {
            error_message("no_draft", "No draft exists; choose the saved checkpoint.")
        })?
    } else {
        &record.saved_json
    };
    let mut project = normalized(source)?;
    project.metadata.name = name.into();
    let result = insert(&tx, normalized(&encoded(&project)?)?)?;
    tx.commit()?;
    Ok(result)
}
