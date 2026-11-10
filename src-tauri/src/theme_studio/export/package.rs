//! Read-only catalog snapshot -> in-memory candidate files. No output paths, IPC or install.
use super::retrobat::compile_candidate;
use crate::theme_studio::{
    image_store::verified_png,
    projects::{self, error_message, Result},
    validation::parse_project,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;
#[derive(Debug, PartialEq, Eq)]
pub struct CandidatePackage {
    pub files: BTreeMap<String, Vec<u8>>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub format: String,
    pub status: String,
    pub target: String,
    pub source_sha256: String,
    /// Includes XML, used PNGs and credits. Excludes this manifest to avoid self-hashing.
    pub files: Vec<FileEntry>,
    pub warnings: Vec<String>,
}
#[derive(Serialize)]
struct Credit<'a> {
    path: &'a str,
    license: &'a str,
    attribution: &'a str,
}
#[derive(Serialize)]
struct Credits<'a> {
    theme_author: &'a str,
    theme_license: &'a str,
    assets: Vec<Credit<'a>>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn insert(
    files: &mut BTreeMap<String, Vec<u8>>,
    path: String,
    bytes: Vec<u8>,
    total: &mut usize,
) -> Result<()> {
    if files.contains_key(&path) {
        return Err(error_message(
            "package_collision",
            "Duplicate package path.",
        ));
    }
    if bytes.len() > MAX_PACKAGE_BYTES.saturating_sub(*total) {
        return Err(error_message(
            "package_limit",
            "Candidate package exceeds 32 MiB.",
        ));
    }
    *total += bytes.len();
    files.insert(path, bytes);
    Ok(())
}
/// Builds only the selected stored checkpoint or acknowledged draft at an exact revision.
/// The transaction holds a consistent read snapshot; failures return no partial package.
pub fn assemble(
    connection: &Connection,
    project_id: &str,
    expected_revision: i64,
) -> Result<CandidatePackage> {
    assemble_with_limit(connection, project_id, expected_revision, MAX_PACKAGE_BYTES)
}
fn assemble_with_limit(
    connection: &Connection,
    project_id: &str,
    expected_revision: i64,
    limit: usize,
) -> Result<CandidatePackage> {
    let tx = connection.unchecked_transaction()?;
    let record = projects::open(&tx, project_id)?;
    if record.revision != expected_revision {
        return Err(error_message(
            "conflict",
            "Project changed; review the current revision.",
        ));
    }
    let source = record.draft_json.as_deref().unwrap_or(&record.saved_json);
    let project = parse_project(source)
        .map_err(|_| error_message("invalid_document", "Stored source is invalid."))?;
    let candidate =
        compile_candidate(source).map_err(|e| error_message("compile_blocked", &e.0))?;
    let mut files = BTreeMap::new();
    let mut total = 0;
    // Reserve budget before reading blobs, including declaration lengths bounded by compiler input.
    let declared = candidate
        .required_assets
        .iter()
        .try_fold(candidate.xml.len(), |n, a| {
            usize::try_from(a.bytes)
                .ok()
                .and_then(|bytes| n.checked_add(bytes))
        })
        .ok_or_else(|| error_message("package_limit", "Declared package size exceeds bounds."))?;
    if declared > limit.min(MAX_PACKAGE_BYTES) {
        return Err(error_message(
            "package_limit",
            "Declared package size exceeds the build budget.",
        ));
    }
    insert(
        &mut files,
        candidate.path,
        candidate.xml.into_bytes(),
        &mut total,
    )?;
    let mut used = BTreeSet::new();
    for required in &candidate.required_assets {
        let asset = project
            .assets
            .iter()
            .find(|a| {
                a.path == required.path && a.sha256 == required.sha256 && a.bytes == required.bytes
            })
            .ok_or_else(|| {
                error_message(
                    "image_authorization",
                    "Image is not declared in the selected project.",
                )
            })?;
        let bytes = verified_png(&tx, asset)?;
        used.insert(asset.path.as_str());
        insert(&mut files, asset.path.clone(), bytes, &mut total)?;
    }
    // Retain every declaration's credit for a used path, even when aliases share one blob.
    let mut credits: Vec<_> = project
        .assets
        .iter()
        .filter(|a| used.contains(a.path.as_str()))
        .map(|a| Credit {
            path: &a.path,
            license: &a.license,
            attribution: &a.attribution,
        })
        .collect();
    credits.sort_by(|a, b| {
        (a.path, a.license, a.attribution).cmp(&(b.path, b.license, b.attribution))
    });
    let mut warnings=vec!["Experimental candidate: native compatibility is unverified; export and installation remain disabled.".to_string()];
    if project
        .metadata
        .license
        .trim()
        .eq_ignore_ascii_case("unknown")
        || project
            .metadata
            .license
            .trim()
            .eq_ignore_ascii_case("custom")
        || credits.iter().any(|a| {
            ["unknown", "custom"]
                .iter()
                .any(|v| a.license.trim().eq_ignore_ascii_case(v))
        })
    {
        warnings.push("License review required: unknown/custom declarations do not establish redistribution permission.".into());
    }
    warnings.push("License/provenance declarations are user-supplied; this is not a complete license or privacy audit.".into());
    let credits = serde_json::to_vec_pretty(&Credits {
        theme_author: &project.metadata.author,
        theme_license: &project.metadata.license,
        assets: credits,
    })
    .map_err(|_| error_message("package_json", "Credit serialization failed."))?;
    insert(&mut files, "licenses.json".into(), credits, &mut total)?;
    let manifest = Manifest {
        format: "retrodrive.candidate-manifest.v1".into(),
        status: "experimental_unverified".into(),
        target: "retrobat-format7-candidate".into(),
        source_sha256: digest(
            &serde_json::to_vec(&project)
                .map_err(|_| error_message("package_json", "Source serialization failed."))?,
        ),
        files: files
            .iter()
            .map(|(path, bytes)| FileEntry {
                path: path.clone(),
                bytes: bytes.len(),
                sha256: digest(bytes),
            })
            .collect(),
        warnings,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| error_message("package_json", "Manifest serialization failed."))?;
    insert(
        &mut files,
        "retrodrive-candidate.json".into(),
        bytes,
        &mut total,
    )?;
    if total > limit {
        return Err(error_message(
            "package_limit",
            "Candidate package exceeds build budget.",
        ));
    }
    // No commit: dropping a read transaction cannot mutate source/blob rows.
    Ok(CandidatePackage { files })
}

#[cfg(test)]
pub(super) fn with_limit(
    connection: &Connection,
    id: &str,
    revision: i64,
    limit: usize,
) -> Result<CandidatePackage> {
    assemble_with_limit(connection, id, revision, limit)
}
