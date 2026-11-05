//! Read-only folder indexer.
//!
//! Safety properties (docs/02 NFR-02, docs/07 "Offline and rescan rules"):
//! - Scanned folders are only read; nothing in them is created, moved or modified.
//! - Symbolic links and junctions are not followed and are reported as skipped.
//! - Every unreadable entry is reported back to the caller instead of being dropped.
//! - Previously indexed files under the scanned folder are marked `missing` only
//!   after a scan that finished without any errors; catalog rows are never deleted.

use crate::{classify, db, paths};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

/// Detailed issues returned to the UI are capped; `issue_count` holds the total.
const MAX_REPORTED_ISSUES: usize = 200;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ScanIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Default)]
pub struct ScanSummary {
    /// Supported game files seen during this scan.
    pub indexed: usize,
    /// Files seen for the first time.
    pub new_files: usize,
    /// Previously indexed files not found by this (error-free) scan.
    pub marked_missing: usize,
    /// Files ignored because they are not recognized game files.
    pub skipped_unsupported: usize,
    /// Symbolic links / junctions that were not followed.
    pub skipped_links: usize,
    /// True when every entry was read; missing-file detection only runs then.
    pub complete: bool,
    pub issue_count: usize,
    pub issues: Vec<ScanIssue>,
}

impl ScanSummary {
    fn issue(&mut self, path: &Path, message: impl Into<String>) {
        self.issue_count += 1;
        if self.issues.len() < MAX_REPORTED_ISSUES {
            self.issues.push(ScanIssue { path: path.display().to_string(), message: message.into() });
        }
    }
}

/// Scan `folder` into the catalog. `volume_kind` maps a volume root such as `D:\`
/// to `local`, `removable`, `network` or `unknown`.
pub fn scan(connection: &mut Connection, folder: &Path, volume_kind: &dyn Fn(&str) -> &'static str) -> Result<ScanSummary, String> {
    let root = paths::normalize(folder).map_err(|e| format!("Cannot open {}: {e}", folder.display()))?;
    if !root.is_dir() {
        return Err("The selected location is not a directory".into());
    }
    let (volume_root, root_relative) = paths::split_volume(&root)
        .ok_or_else(|| format!("Unsupported folder path: {}", root.display()))?;
    let root_name = root.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();

    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let storage_id = db::upsert_storage_location(&tx, &volume_root, volume_kind(&volume_root))?;
    let indexed_root_id = upsert_indexed_root(&tx, &storage_id, &root_relative)?;
    let scan_id = db::new_id();

    let mut summary = ScanSummary::default();
    for entry in WalkDir::new(&root).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let path = error.path().map(Path::to_path_buf).unwrap_or_else(|| root.clone());
                summary.issue(&path, error.to_string());
                continue;
            }
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            summary.skipped_links += 1;
            summary.issue(entry.path(), "Symbolic link or junction not followed");
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(relative_to_root) = path.strip_prefix(&root) else {
            summary.issue(path, "Path is outside the scanned folder");
            continue;
        };
        let Some(relative_str) = relative_to_root.to_str() else {
            summary.issue(path, "File name is not valid Unicode and was skipped");
            continue;
        };

        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let mut folders: Vec<&str> = vec![root_name.as_str()];
        if let Some(parent) = relative_to_root.parent() {
            folders.extend(parent.components().filter_map(|c| c.as_os_str().to_str()));
        }
        let Some(classification) = classify::classify(&folders, &extension) else {
            summary.skipped_unsupported += 1;
            continue;
        };

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(error) => {
                summary.issue(path, format!("Cannot read file details: {error}"));
                continue;
            }
        };
        let size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
        let modified = metadata.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs().to_string());

        let relative_path = join_relative(&root_relative, &relative_str.replace('\\', "/"));
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or(relative_str).to_string();
        let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").replace('_', " ");

        let is_new = record_file(
            &tx,
            &storage_id,
            &relative_path,
            &scan_id,
            &FileFacts { title: &title, filename: &filename, extension: &extension, size, modified: modified.as_deref(), classification },
        )?;
        summary.indexed += 1;
        if is_new {
            summary.new_files += 1;
        }
    }

    summary.complete = summary.issue_count == summary.skipped_links;
    if summary.complete {
        summary.marked_missing = mark_missing(&tx, &storage_id, &root_relative, &scan_id)?;
    }
    let status = if summary.complete { "completed" } else { "completed_with_errors" };
    tx.execute(
        "UPDATE indexed_roots SET last_scan_completed_at = CURRENT_TIMESTAMP, last_scan_status = ?2 WHERE id = ?1",
        params![indexed_root_id, status],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE storage_locations SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?1", [&storage_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(summary)
}

struct FileFacts<'a> {
    title: &'a str,
    filename: &'a str,
    extension: &'a str,
    size: i64,
    modified: Option<&'a str>,
    classification: classify::Classification,
}

fn join_relative(root_relative: &str, below_root: &str) -> String {
    if root_relative.is_empty() {
        below_root.to_string()
    } else {
        format!("{root_relative}/{below_root}")
    }
}

fn upsert_indexed_root(tx: &Connection, storage_id: &str, relative: &str) -> Result<String, String> {
    let existing = tx
        .query_row(
            "SELECT id FROM indexed_roots WHERE storage_location_id = ?1 AND relative_path = ?2",
            params![storage_id, relative],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let id = match existing {
        Some(id) => id,
        None => {
            let id = db::new_id();
            tx.execute(
                "INSERT INTO indexed_roots (id, storage_location_id, relative_path) VALUES (?1, ?2, ?3)",
                params![id, storage_id, relative],
            )
            .map_err(|e| e.to_string())?;
            id
        }
    };
    tx.execute(
        "UPDATE indexed_roots SET last_scan_started_at = CURRENT_TIMESTAMP, last_scan_status = 'running' WHERE id = ?1",
        [&id],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Insert or refresh one file location; returns true when the file is new.
/// Titles of existing games are left alone so manual corrections survive rescans.
fn record_file(tx: &Connection, storage_id: &str, relative_path: &str, scan_id: &str, facts: &FileFacts) -> Result<bool, String> {
    let existing = tx
        .query_row(
            "SELECT id, game_file_id FROM file_locations WHERE storage_location_id = ?1 AND relative_path = ?2",
            params![storage_id, relative_path],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((location_id, file_id)) = existing {
        tx.execute(
            "UPDATE file_locations SET last_seen_at = CURRENT_TIMESTAMP, last_scan_id = ?2, present_status = 'present' WHERE id = ?1",
            params![location_id, scan_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE game_files SET size_bytes = ?2, modified_at = ?3,
                 platform_hint = CASE WHEN platform_hint_source = 'legacy' THEN platform_hint ELSE ?4 END,
                 platform_hint_source = CASE WHEN platform_hint_source = 'legacy' THEN platform_hint_source ELSE ?5 END
             WHERE id = ?1",
            params![file_id, facts.size, facts.modified, facts.classification.platform, facts.classification.source.as_str()],
        )
        .map_err(|e| e.to_string())?;
        return Ok(false);
    }
    let file_id = db::insert_game_file(
        tx,
        facts.title,
        facts.classification.platform,
        facts.filename,
        facts.extension,
        facts.size,
        facts.modified,
        facts.classification.source.as_str(),
    )?;
    tx.execute(
        "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path, last_scan_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![db::new_id(), file_id, storage_id, relative_path, scan_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Mark present locations under the scanned folder that this scan did not see.
fn mark_missing(tx: &Connection, storage_id: &str, root_relative: &str, scan_id: &str) -> Result<usize, String> {
    let prefix = if root_relative.is_empty() { String::new() } else { format!("{root_relative}/") };
    tx.execute(
        "UPDATE file_locations SET present_status = 'missing'
         WHERE storage_location_id = ?1
           AND present_status = 'present'
           AND (last_scan_id IS NULL OR last_scan_id <> ?2)
           AND substr(relative_path, 1, length(?3)) = ?3 COLLATE NOCASE",
        params![storage_id, scan_id, prefix],
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn catalog() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        db::configure(&c).unwrap();
        db::migrate(&mut c, None).unwrap();
        c
    }

    fn local(_: &str) -> &'static str {
        "local"
    }

    fn write(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = WalkDir::new(root)
            .into_iter()
            .map(Result::unwrap)
            .filter(|e| e.file_type().is_file())
            .map(|e| (e.path().display().to_string(), fs::read(e.path()).unwrap()))
            .collect();
        files.sort();
        files
    }

    fn statuses(c: &Connection) -> Vec<(String, String)> {
        let mut s = c.prepare("SELECT relative_path, present_status FROM file_locations ORDER BY relative_path").unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "snes/Super Game (USA).sfc", b"sfc");
        write(root, "snes/readme.txt", b"not a game");
        write(root, "snes/Super Game (USA).srm", b"save");
        write(root, "snes/gamelist.xml", b"<gameList/>");
        write(root, "nes/Ünïcode Quest – 日本.nes", b"nes");
        write(root, "psx/Disc Game (USA)/Disc Game (USA).cue", b"cue");
        write(root, "psx/Disc Game (USA)/Disc Game (USA) (Track 1).bin", b"bin");
        write(root, "loose/mystery.zip", b"zip");
        dir
    }

    #[test]
    fn indexes_only_game_files_and_never_modifies_source() {
        let dir = fixture();
        let before = snapshot(dir.path());
        let mut c = catalog();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(summary.indexed, 4, "{summary:?}");
        assert_eq!(summary.new_files, 4);
        assert_eq!(summary.skipped_unsupported, 4);
        assert!(summary.complete);
        assert_eq!(summary.issue_count, 0);
        assert_eq!(snapshot(dir.path()), before, "scan must not modify the source folder");

        let platforms: Vec<(String, String)> = c
            .prepare("SELECT canonical_title, platform FROM games ORDER BY canonical_title")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(platforms.contains(&("Super Game (USA)".into(), "SNES".into())));
        assert!(platforms.contains(&("Disc Game (USA)".into(), "PlayStation".into())));
        assert!(platforms.contains(&("mystery".into(), "Unclassified".into())));
        assert!(platforms.contains(&("Ünïcode Quest – 日本".into(), "NES".into())));
    }

    #[test]
    fn rescans_do_not_duplicate() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        let second = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(second.indexed, 4);
        assert_eq!(second.new_files, 0);
        let games: i64 = c.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
        assert_eq!(games, 4);
    }

    #[test]
    fn removed_files_become_missing_not_deleted_and_return_when_restored() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        let rom = dir.path().join("snes/Super Game (USA).sfc");
        fs::remove_file(&rom).unwrap();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(summary.marked_missing, 1);
        let rows = statuses(&c);
        assert_eq!(rows.len(), 4, "catalog rows are kept");
        assert_eq!(rows.iter().filter(|(_, s)| s == "missing").count(), 1);

        fs::write(&rom, b"sfc").unwrap();
        scan(&mut c, dir.path(), &local).unwrap();
        assert!(statuses(&c).iter().all(|(_, s)| s == "present"));
    }

    #[test]
    fn scanning_a_subfolder_only_affects_that_subfolder() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        fs::remove_file(dir.path().join("nes/Ünïcode Quest – 日本.nes")).unwrap();
        // Rescanning only snes/ must not mark the (now deleted) nes file missing.
        let summary = scan(&mut c, &dir.path().join("snes"), &local).unwrap();
        assert_eq!(summary.marked_missing, 0);
        assert!(statuses(&c).iter().all(|(_, s)| s == "present"));
        // Same storage location and relative paths, so no duplicates were created.
        assert_eq!(statuses(&c).len(), 4);
    }

    #[test]
    fn manual_title_edits_survive_rescan() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        c.execute("UPDATE games SET canonical_title = 'Corrected Title' WHERE canonical_title = 'mystery'", []).unwrap();
        scan(&mut c, dir.path(), &local).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM games WHERE canonical_title = 'Corrected Title'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn rejects_non_directories() {
        let dir = fixture();
        let mut c = catalog();
        assert!(scan(&mut c, &dir.path().join("snes/readme.txt"), &local).is_err());
        assert!(scan(&mut c, &dir.path().join("does-not-exist"), &local).is_err());
    }

    #[test]
    fn stored_paths_have_no_verbatim_prefix() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        let root: String = c.query_row("SELECT root_path FROM storage_locations", [], |r| r.get(0)).unwrap();
        assert!(!root.starts_with(r"\\?\"), "{root}");
    }
}
