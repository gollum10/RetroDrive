//! Read-only folder indexer.
//!
//! Safety properties (docs/02 NFR-02, docs/07 "Offline and rescan rules"):
//! - Scanned folders are only read; nothing in them is created, moved or modified.
//! - Symbolic links and junctions are not followed and are reported as skipped.
//! - Every unreadable entry is reported back to the caller instead of being dropped.
//! - Previously indexed files under the scanned folder are marked `missing` only
//!   after a scan that finished without any errors and was not cancelled; catalog
//!   rows are never deleted.
//! - Work is committed in batches with progress callbacks and cooperative
//!   cancellation (RDS-002).
//! - Multi-file games are grouped after the walk: `.cue` tracks and `.m3u` discs
//!   join one game; references outside the scanned folder are refused (RDS-005).

use crate::volumes::VolumeProbe;
use crate::{classify, db, media, paths};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

/// Detailed issues returned to the UI are capped; `issue_count` holds the total.
const MAX_REPORTED_ISSUES: usize = 200;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ScanIssue {
    pub path: String,
    pub message: String,
    /// `error` (something could not be read; the scan is incomplete) or `warning`.
    pub severity: &'static str,
}

#[derive(Debug, Serialize, Default, Clone)]
pub struct ScanSummary {
    /// Supported game files seen during this scan.
    pub indexed: usize,
    /// Games found: indexed files minus the discs grouped under a playlist.
    pub games: usize,
    /// Files seen for the first time.
    pub new_files: usize,
    /// Previously indexed files not found by this (error-free) scan.
    pub marked_missing: usize,
    /// Files ignored because they are not recognized game files (images are counted separately).
    pub skipped_unsupported: usize,
    /// Image files seen (box art, screenshots…); never copied, only linked.
    pub images_found: usize,
    /// Games linked to an image by this scan (from `gamelist.xml` or folder conventions).
    pub images_linked: usize,
    /// Symbolic links / junctions that were not followed.
    pub skipped_links: usize,
    /// True when every entry was read and the scan was not cancelled;
    /// missing-file detection only runs then.
    pub complete: bool,
    /// True when the scan was stopped by a cancellation request.
    pub cancelled: bool,
    /// Errors plus warnings.
    pub issue_count: usize,
    /// Entries that could not be read. Any error makes the scan incomplete.
    pub error_count: usize,
    /// Problems that do not stop missing-file detection (links, broken references).
    pub warning_count: usize,
    pub issues: Vec<ScanIssue>,
    /// Discs and tracks attached to a multi-file game by its `.m3u` or `.cue`.
    pub grouped_files: usize,
    /// Set when the scanned drive was recognized from an earlier scan under a
    /// different drive letter (e.g. `E:\`); catalog paths now use the new letter.
    pub remounted_from: Option<String>,
}

impl ScanSummary {
    /// Record an entry that could not be read.
    fn issue(&mut self, path: &Path, message: impl Into<String>) {
        self.error_count += 1;
        self.push(path, message.into(), "error");
    }

    /// Record a problem that does not make the scan incomplete.
    fn warn(&mut self, path: &Path, message: impl Into<String>) {
        self.warning_count += 1;
        self.push(path, message.into(), "warning");
    }

    fn push(&mut self, path: &Path, message: String, severity: &'static str) {
        self.issue_count += 1;
        if self.issues.len() < MAX_REPORTED_ISSUES {
            self.issues.push(ScanIssue { path: path.display().to_string(), message, severity });
        }
    }
}

/// Live progress reported while a scan runs.
#[derive(Debug, Serialize, Clone, Default, PartialEq)]
pub struct ScanProgress {
    /// Directory entries examined so far (files and folders).
    pub entries_checked: usize,
    pub indexed: usize,
    pub new_files: usize,
    pub issue_count: usize,
    /// The entry being examined when this progress was reported.
    pub current_path: String,
}

/// Lets the caller observe progress and request cancellation.
pub trait ScanObserver {
    /// Checked before every entry; returning true stops the scan at the next entry.
    fn cancelled(&self) -> bool {
        false
    }
    /// Called at most every `ScanOptions::progress_interval`, and once at the end.
    fn progress(&mut self, _progress: &ScanProgress) {}
}

/// Observer that never cancels and ignores progress.
#[cfg(test)]
pub struct Unobserved;
#[cfg(test)]
impl ScanObserver for Unobserved {}

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Files recorded per committed batch. Committing in batches keeps work that
    /// was already done if the app is closed, and lets other connections write
    /// (job log) between batches.
    pub batch_size: usize,
    /// Longest time between commits regardless of batch size.
    pub max_batch_time: Duration,
    pub progress_interval: Duration,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { batch_size: 500, max_batch_time: Duration::from_secs(1), progress_interval: Duration::from_millis(200) }
    }
}

/// An explicit transaction that rolls back if dropped while open.
struct Batch<'c> {
    connection: &'c Connection,
    open: bool,
}

impl<'c> Batch<'c> {
    fn begin(connection: &'c Connection) -> Result<Self, String> {
        connection.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
        Ok(Self { connection, open: true })
    }
    fn commit(&mut self) -> Result<(), String> {
        self.connection.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        self.open = false;
        Ok(())
    }
    fn restart(&mut self) -> Result<(), String> {
        self.commit()?;
        self.connection.execute_batch("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
        self.open = true;
        Ok(())
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}

/// Scan `folder` into the catalog with default options and no observer.
#[cfg(test)]
pub fn scan(connection: &mut Connection, folder: &Path, volume_probe: &dyn Fn(&str) -> VolumeProbe) -> Result<ScanSummary, String> {
    scan_with(connection, folder, volume_probe, &mut Unobserved, ScanOptions::default())
}

/// Scan `folder` into the catalog. `volume_probe` reports the kind and fingerprint
/// of the volume at a root such as `D:\` (see `volumes::probe`).
///
/// Work is committed in batches. If the scan is cancelled, files recorded so far
/// are kept but missing-file detection is skipped, so a cancelled scan can never
/// mark files missing (QA-002).
pub fn scan_with(
    connection: &Connection,
    folder: &Path,
    volume_probe: &dyn Fn(&str) -> VolumeProbe,
    observer: &mut dyn ScanObserver,
    options: ScanOptions,
) -> Result<ScanSummary, String> {
    let root = paths::normalize(folder).map_err(|e| format!("Cannot open {}: {e}", folder.display()))?;
    if !root.is_dir() {
        return Err("The selected location is not a directory".into());
    }
    let (volume_root, root_relative) = paths::split_volume(&root)
        .ok_or_else(|| format!("Unsupported folder path: {}", root.display()))?;
    let root_name = root.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();

    let probe = volume_probe(&volume_root);
    let mut batch = Batch::begin(connection)?;
    let location = db::resolve_storage_location(connection, &volume_root, &probe)?;
    let storage_id = location.id;
    let indexed_root_id = upsert_indexed_root(connection, &storage_id, &root_relative)?;
    let scan_id = db::new_id();
    batch.restart()?;

    let mut summary = ScanSummary { remounted_from: location.previous_root, ..ScanSummary::default() };
    let mut recorded: HashMap<String, Recorded> = HashMap::new();
    let mut cue_sheets: Vec<ReferenceList> = Vec::new();
    let mut playlists: Vec<ReferenceList> = Vec::new();
    // Images (lower-case volume-relative path -> as written) and gamelists (file, folder).
    let mut images: HashMap<String, String> = HashMap::new();
    let mut gamelists: Vec<(PathBuf, String)> = Vec::new();
    let mut progress = ScanProgress::default();
    let mut pending = 0usize;
    let mut last_commit = Instant::now();
    let mut last_progress: Option<Instant> = None;

    for entry in WalkDir::new(&root).follow_links(false) {
        if observer.cancelled() {
            summary.cancelled = true;
            break;
        }
        progress.entries_checked += 1;
        if last_progress.is_none_or(|t| t.elapsed() >= options.progress_interval) {
            progress.indexed = summary.indexed;
            progress.new_files = summary.new_files;
            progress.issue_count = summary.issue_count;
            progress.current_path = match &entry {
                Ok(e) => e.path().display().to_string(),
                Err(e) => e.path().map(|p| p.display().to_string()).unwrap_or_default(),
            };
            observer.progress(&progress);
            last_progress = Some(Instant::now());
        }

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
            summary.warn(entry.path(), "Symbolic link or junction not followed");
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
            let below = relative_str.replace('\\', "/");
            let relative_path = join_relative(&root_relative, &below);
            if media::is_image(&extension) {
                summary.images_found += 1;
                images.insert(relative_path.to_lowercase(), relative_path);
            } else {
                if path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("gamelist.xml")) {
                    let folder = relative_path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                    gamelists.push((path.to_path_buf(), folder));
                }
                summary.skipped_unsupported += 1;
            }
            continue;
        };

        let classification = if path.file_name().and_then(|n| n.to_str()).is_some_and(classify::looks_like_bios) {
            classify::Classification { platform: classify::BIOS, source: classify::HintSource::Folder }
        } else {
            classification
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

        let outcome = record_file(
            connection,
            &storage_id,
            &relative_path,
            &scan_id,
            &FileFacts { title: &title, filename: &filename, extension: &extension, size, modified: modified.as_deref(), classification },
        )?;
        summary.indexed += 1;
        if outcome.is_new {
            summary.new_files += 1;
        }
        match extension.as_str() {
            "cue" => cue_sheets.push(ReferenceList { path: path.to_path_buf(), relative_path: relative_path.clone() }),
            "m3u" => playlists.push(ReferenceList { path: path.to_path_buf(), relative_path: relative_path.clone() }),
            _ => {}
        }
        recorded.insert(
            relative_path.to_lowercase(),
            Recorded {
                file_id: outcome.file_id,
                release_id: outcome.release_id,
                role: outcome.role,
                walked: true,
                title,
                platform: classification.platform,
            },
        );
        pending += 1;
        if pending >= options.batch_size || last_commit.elapsed() >= options.max_batch_time {
            batch.restart()?;
            pending = 0;
            last_commit = Instant::now();
        }
    }

    // Group multi-file games: tracks under cue sheets first, then discs under playlists.
    let mut grouping = Grouping {
        connection,
        root: &root,
        root_relative: &root_relative,
        storage_id: &storage_id,
        scan_id: &scan_id,
        recorded,
        merged_into: HashMap::new(),
        touched: HashSet::new(),
        linked: HashSet::new(),
    };
    for list in &cue_sheets {
        grouping.process(&mut summary, list, false)?;
    }
    for list in &playlists {
        grouping.process(&mut summary, list, true)?;
    }
    if !summary.cancelled && summary.error_count == 0 {
        grouping.detach_orphans()?;
    }
    grouping.update_payloads()?;
    let walked_and_linked = grouping.recorded.values().filter(|r| r.walked && grouping.linked.contains(&r.file_id)).count();
    summary.games = summary.indexed.saturating_sub(walked_and_linked);

    if !summary.cancelled {
        summary.images_linked = link_media(connection, &storage_id, &grouping.recorded, &grouping.linked, &gamelists, &images, &mut summary)?;
    }

    summary.complete = !summary.cancelled && summary.error_count == 0;
    if summary.complete {
        summary.marked_missing = mark_missing(connection, &storage_id, &root_relative, &scan_id)?;
    }
    let status = if summary.cancelled {
        "cancelled"
    } else if summary.complete {
        "completed"
    } else {
        "completed_with_errors"
    };
    connection
        .execute(
            "UPDATE indexed_roots SET last_scan_completed_at = CURRENT_TIMESTAMP, last_scan_status = ?2 WHERE id = ?1",
            params![indexed_root_id, status],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute("UPDATE storage_locations SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?1", [&storage_id])
        .map_err(|e| e.to_string())?;
    batch.commit()?;

    progress.indexed = summary.indexed;
    progress.new_files = summary.new_files;
    progress.issue_count = summary.issue_count;
    progress.current_path.clear();
    observer.progress(&progress);
    Ok(summary)
}

/// Link games reached by this scan to images: entries in `gamelist.xml` first, then
/// common folder conventions next to the ROM. Only image files seen by this walk are
/// used, so links always point inside the scanned folder. The user's own picks are
/// never touched. Returns how many games got an image.
fn link_media(
    connection: &Connection,
    storage_id: &str,
    recorded: &HashMap<String, Recorded>,
    members: &HashSet<String>,
    gamelists: &[(PathBuf, String)],
    images: &HashMap<String, String>,
    summary: &mut ScanSummary,
) -> Result<usize, String> {
    let mut chosen: HashMap<&str, Vec<(&'static str, &'static str, String)>> = HashMap::new();
    for (file, folder) in gamelists {
        let entries = match media::parse_gamelist(file) {
            Ok(entries) => entries,
            Err(error) => {
                summary.warn(file, format!("Couldn't read this gamelist, so its images weren't linked: {error}"));
                continue;
            }
        };
        for entry in entries {
            let Some(rom) = media::resolve_relative(folder, &entry.path) else { continue };
            let Some(record) = recorded.get(&rom.to_lowercase()) else { continue };
            if !record.walked || members.contains(&record.file_id) {
                continue;
            }
            for (kind, image) in &entry.images {
                if *kind == "marquee" {
                    continue; // a logo, not a cover
                }
                let found = media::resolve_relative(folder, image).and_then(|r| images.get(&r.to_lowercase()));
                if let Some(actual) = found {
                    chosen.entry(record.file_id.as_str()).or_default().push((kind, "gamelist", actual.clone()));
                }
            }
        }
    }
    for (key, record) in recorded {
        if !record.walked || members.contains(&record.file_id) || chosen.contains_key(record.file_id.as_str()) {
            continue;
        }
        let (folder, name) = key.rsplit_once('/').unwrap_or(("", key.as_str()));
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        'conventions: for (kind, pattern) in media::CONVENTIONS {
            let base = pattern.replace("{stem}", stem);
            for extension in media::IMAGE_EXTENSIONS {
                let candidate = if folder.is_empty() { format!("{base}.{extension}") } else { format!("{folder}/{base}.{extension}") };
                if let Some(actual) = images.get(&candidate) {
                    chosen.entry(record.file_id.as_str()).or_default().push((kind, "folder", actual.clone()));
                    break 'conventions;
                }
            }
        }
    }
    let mut clear = connection
        .prepare_cached("DELETE FROM game_media WHERE game_file_id = ?1 AND source IN ('gamelist', 'folder')")
        .map_err(|e| e.to_string())?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO game_media (game_file_id, kind, source, storage_location_id, relative_path) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (game_file_id, source, kind) DO UPDATE SET relative_path = excluded.relative_path, storage_location_id = excluded.storage_location_id",
        )
        .map_err(|e| e.to_string())?;
    for record in recorded.values().filter(|r| r.walked && !members.contains(&r.file_id)) {
        clear.execute([&record.file_id]).map_err(|e| e.to_string())?;
    }
    for (file_id, found) in &chosen {
        for (kind, source, relative) in found {
            insert.execute(params![file_id, kind, source, storage_id, relative]).map_err(|e| e.to_string())?;
        }
    }
    Ok(chosen.len())
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
struct RecordOutcome {
    is_new: bool,
    file_id: String,
    release_id: String,
    role: String,
}

fn record_file(tx: &Connection, storage_id: &str, relative_path: &str, scan_id: &str, facts: &FileFacts) -> Result<RecordOutcome, String> {
    let existing = tx
        .query_row(
            "SELECT fl.id, fl.game_file_id, gf.release_id, gf.file_role FROM file_locations fl JOIN game_files gf ON gf.id = fl.game_file_id
             WHERE fl.storage_location_id = ?1 AND fl.relative_path = ?2",
            params![storage_id, relative_path],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((location_id, file_id, release_id, role)) = existing {
        tx.execute(
            "UPDATE file_locations SET last_seen_at = CURRENT_TIMESTAMP, last_scan_id = ?2, present_status = 'present' WHERE id = ?1",
            params![location_id, scan_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE game_files SET size_bytes = ?2, modified_at = ?3,
                 payload_bytes = CASE WHEN file_role = 'primary' AND NOT EXISTS (
                     SELECT 1 FROM game_files m WHERE m.release_id = game_files.release_id AND m.id <> game_files.id) THEN ?2 ELSE payload_bytes END,
                 platform_hint = CASE WHEN platform_hint_source = 'legacy' THEN platform_hint ELSE ?4 END,
                 platform_hint_source = CASE WHEN platform_hint_source = 'legacy' THEN platform_hint_source ELSE ?5 END
             WHERE id = ?1",
            params![file_id, facts.size, facts.modified, facts.classification.platform, facts.classification.source.as_str()],
        )
        .map_err(|e| e.to_string())?;
        // A newer build may recognize the system now (e.g. a c64 folder that used to be
        // Unclassified): the primary file's game follows. Legacy rows keep their platform.
        if role == "primary" {
            if let Some(release) = &release_id {
                tx.execute(
                    "UPDATE games SET platform = ?2 WHERE id = (SELECT game_id FROM game_releases WHERE id = ?1) AND platform <> ?2
                     AND NOT EXISTS (SELECT 1 FROM game_files WHERE id = ?3 AND platform_hint_source = 'legacy')",
                    params![release, facts.classification.platform, file_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        return Ok(RecordOutcome { is_new: false, file_id, release_id: release_id.unwrap_or_default(), role });
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
    let release_id = tx
        .query_row("SELECT release_id FROM game_files WHERE id = ?1", [&file_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(RecordOutcome { is_new: true, file_id, release_id, role: "primary".into() })
}

/// A file recorded during this scan.
#[derive(Clone)]
struct Recorded {
    file_id: String,
    release_id: String,
    /// Role stored in the catalog before this scan's grouping pass.
    role: String,
    /// True when the file was reached by the folder walk (not only by a reference).
    walked: bool,
    title: String,
    platform: &'static str,
}

/// A `.cue` sheet or `.m3u` playlist found by the walk.
struct ReferenceList {
    path: std::path::PathBuf,
    relative_path: String,
}

/// Cue sheets and playlists are small text files; anything larger is ignored.
const MAX_REFERENCE_FILE_BYTES: u64 = 1024 * 1024;

/// File names referenced by a cue sheet (`FILE "Track 01.bin" BINARY`) or listed
/// in an `.m3u` playlist (one path per line, `#` comments).
fn read_references(path: &Path, playlist: bool) -> Result<Vec<String>, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("Cannot read: {e}"))?.len();
    if size > MAX_REFERENCE_FILE_BYTES {
        return Err(format!("Too large ({size} bytes) to be a cue sheet or playlist"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    Ok(text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if playlist {
                (!line.is_empty() && !line.starts_with('#')).then(|| line.to_string())
            } else {
                cue_file_reference(line)
            }
        })
        .collect())
}

fn cue_file_reference(line: &str) -> Option<String> {
    if !line.get(..5)?.eq_ignore_ascii_case("FILE ") {
        return None;
    }
    let rest = line[5..].trim_start();
    match rest.strip_prefix('"') {
        Some(quoted) => quoted.find('"').map(|end| quoted[..end].to_string()),
        None => rest.split_whitespace().next().map(str::to_string),
    }
}

/// Links multi-file games after the folder walk (RDS-005, docs/07 "Recognition"):
/// `.bin`/`.img` tracks join the game of the `.cue` that references them, and discs
/// listed in an `.m3u` join the playlist's game. Only catalog records change.
struct Grouping<'a> {
    connection: &'a Connection,
    root: &'a Path,
    root_relative: &'a str,
    storage_id: &'a str,
    scan_id: &'a str,
    /// Lower-cased volume-relative path → record.
    recorded: HashMap<String, Recorded>,
    /// Releases merged away during this pass → the release they were merged into.
    merged_into: HashMap<String, String>,
    /// Releases whose total size must be recomputed.
    touched: HashSet<String>,
    /// Files linked as a disc or track during this pass.
    linked: HashSet<String>,
}

impl Grouping<'_> {
    fn release_of(&self, release: &str) -> String {
        let mut current = release.to_string();
        while let Some(next) = self.merged_into.get(&current) {
            current = next.clone();
        }
        current
    }

    /// Process one cue sheet or playlist.
    fn process(&mut self, summary: &mut ScanSummary, list: &ReferenceList, playlist: bool) -> Result<(), String> {
        let Some(parent) = self.recorded.get(&list.relative_path.to_lowercase()).cloned() else {
            return Ok(());
        };
        let references = match read_references(&list.path, playlist) {
            Ok(references) => references,
            Err(message) => {
                // Without its contents the grouping is unknown; treat like any unreadable entry.
                summary.issue(&list.path, message);
                return Ok(());
            }
        };
        let mut missing_parts = 0i64;
        let (role, mut disc) = if playlist { ("disc", Some(0i64)) } else { ("track", None) };
        for reference in references {
            if let Some(n) = disc.as_mut() {
                *n += 1;
            }
            if !self.link(summary, list, &parent, &reference, role, disc)? {
                missing_parts += 1;
            }
        }
        self.connection
            .execute("UPDATE game_files SET missing_parts = ?2 WHERE id = ?1", params![parent.file_id, missing_parts])
            .map_err(|e| e.to_string())?;
        self.touched.insert(self.release_of(&parent.release_id));
        Ok(())
    }

    /// Link one referenced file to `parent`'s game. Returns false when the file is missing.
    fn link(
        &mut self,
        summary: &mut ScanSummary,
        list: &ReferenceList,
        parent: &Recorded,
        reference: &str,
        role: &str,
        disc_index: Option<i64>,
    ) -> Result<bool, String> {
        let parent_dir = list.path.parent().unwrap_or(self.root);
        let Ok(target) = paths::normalize(&parent_dir.join(reference)) else {
            summary.warn(&list.path, format!("References \"{reference}\", which was not found"));
            return Ok(false);
        };
        let Ok(below_root) = target.strip_prefix(self.root) else {
            summary.warn(&list.path, format!("References \"{reference}\" outside the scanned folder; ignored for safety"));
            return Ok(true);
        };
        let Some(below_root) = below_root.to_str() else {
            summary.warn(&list.path, format!("References \"{reference}\", whose name is not valid Unicode"));
            return Ok(true);
        };
        if !target.is_file() {
            summary.warn(&list.path, format!("References \"{reference}\", which is not a file"));
            return Ok(false);
        }
        let relative_path = join_relative(self.root_relative, &below_root.replace('\\', "/"));
        let key = relative_path.to_lowercase();
        if key == list.relative_path.to_lowercase() {
            return Ok(true);
        }
        let parent_release = self.release_of(&parent.release_id);
        let child_id = match self.recorded.get(&key).cloned() {
            Some(child) => {
                let child_release = self.release_of(&child.release_id);
                if child_release != parent_release {
                    db::merge_release(self.connection, &child_release, &parent_release)?;
                    self.merged_into.insert(child_release, parent_release.clone());
                }
                child.file_id
            }
            None => {
                let metadata = match std::fs::metadata(&target) {
                    Ok(m) => m,
                    Err(error) => {
                        summary.issue(&target, format!("Cannot read file details: {error}"));
                        return Ok(true);
                    }
                };
                let file_id = self.record_member(&relative_path, &target, &metadata, &parent_release, parent, role, disc_index)?;
                // The walk skipped this file as unrecognized (e.g. a .bin track); it is part of a game after all.
                summary.skipped_unsupported = summary.skipped_unsupported.saturating_sub(1);
                self.recorded.insert(
                    key,
                    Recorded {
                        file_id: file_id.clone(),
                        release_id: parent_release.clone(),
                        role: role.to_string(),
                        walked: false,
                        title: String::new(),
                        platform: parent.platform,
                    },
                );
                file_id
            }
        };
        self.connection
            .execute(
                "UPDATE game_files SET file_role = ?2, parent_file_id = ?3, disc_index = ?4 WHERE id = ?1",
                params![child_id, role, parent.file_id, disc_index],
            )
            .map_err(|e| e.to_string())?;
        self.linked.insert(child_id);
        summary.grouped_files += 1;
        Ok(true)
    }

    /// Record a file reached only through a reference (e.g. a `.bin` track).
    #[allow(clippy::too_many_arguments)]
    fn record_member(
        &self,
        relative_path: &str,
        target: &Path,
        metadata: &std::fs::Metadata,
        release: &str,
        parent: &Recorded,
        role: &str,
        disc_index: Option<i64>,
    ) -> Result<String, String> {
        let size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
        let modified = metadata.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs().to_string());
        let existing = self
            .connection
            .query_row(
                "SELECT fl.id, fl.game_file_id, gf.release_id FROM file_locations fl JOIN game_files gf ON gf.id = fl.game_file_id
                 WHERE fl.storage_location_id = ?1 AND fl.relative_path = ?2",
                params![self.storage_id, relative_path],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((location_id, file_id, old_release)) = existing {
            self.connection
                .execute(
                    "UPDATE file_locations SET last_seen_at = CURRENT_TIMESTAMP, last_scan_id = ?2, present_status = 'present' WHERE id = ?1",
                    params![location_id, self.scan_id],
                )
                .map_err(|e| e.to_string())?;
            self.connection
                .execute("UPDATE game_files SET size_bytes = ?2, modified_at = ?3 WHERE id = ?1", params![file_id, size, modified])
                .map_err(|e| e.to_string())?;
            if let Some(old) = old_release.filter(|old| old != release) {
                db::merge_release(self.connection, &old, release)?;
            }
            return Ok(file_id);
        }
        let filename = target.file_name().and_then(|s| s.to_str()).unwrap_or(relative_path);
        let extension = target.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let file_id = db::insert_member_file(
            self.connection,
            release,
            &parent.file_id,
            role,
            disc_index,
            filename,
            &extension,
            size,
            modified.as_deref(),
            parent.platform,
        )?;
        self.connection
            .execute(
                "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path, last_scan_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![db::new_id(), file_id, self.storage_id, relative_path, self.scan_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(file_id)
    }

    /// After a complete scan: walked files still marked as discs/tracks that no
    /// playlist or cue sheet references any more become games of their own again.
    fn detach_orphans(&mut self) -> Result<(), String> {
        let orphans: Vec<Recorded> = self
            .recorded
            .values()
            .filter(|r| r.walked && r.role != "primary" && !self.linked.contains(&r.file_id))
            .cloned()
            .collect();
        for orphan in orphans {
            let game_id = db::new_id();
            let release_id = db::new_id();
            self.connection
                .execute("INSERT INTO games (id, canonical_title, platform) VALUES (?1, ?2, ?3)", params![game_id, orphan.title, orphan.platform])
                .map_err(|e| e.to_string())?;
            self.connection
                .execute("INSERT INTO game_releases (id, game_id) VALUES (?1, ?2)", params![release_id, game_id])
                .map_err(|e| e.to_string())?;
            self.connection
                .execute(
                    "UPDATE game_files SET release_id = ?2, file_role = 'primary', parent_file_id = NULL, disc_index = NULL WHERE id = ?1",
                    params![orphan.file_id, release_id],
                )
                .map_err(|e| e.to_string())?;
            self.connection
                .execute("UPDATE game_files SET release_id = ?2 WHERE parent_file_id = ?1", params![orphan.file_id, release_id])
                .map_err(|e| e.to_string())?;
            self.touched.insert(release_id);
        }
        Ok(())
    }

    /// Store each touched game's total size on its primary file (used for display and the size sort).
    fn update_payloads(&self) -> Result<(), String> {
        for release in &self.touched {
            self.connection
                .execute(
                    "UPDATE game_files SET payload_bytes = (SELECT COALESCE(SUM(m.size_bytes), 0) FROM game_files m WHERE m.release_id = ?1)
                     WHERE release_id = ?1 AND file_role = 'primary'",
                    [release],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
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

    fn local(_: &str) -> VolumeProbe {
        VolumeProbe { kind: "local", identity: None }
    }

    fn fingerprint(id: &'static str) -> impl Fn(&str) -> VolumeProbe {
        move |_| VolumeProbe {
            kind: "removable",
            identity: Some(crate::volumes::VolumeIdentity {
                stable_id: id.into(),
                label: Some("GAMES".into()),
                filesystem: Some("exFAT".into()),
                serial: None,
                guid: None,
                total_bytes: Some(1),
            }),
        }
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

    struct CancelAfter {
        checks_left: std::cell::Cell<usize>,
        progress_calls: usize,
        last: ScanProgress,
    }

    impl CancelAfter {
        fn new(n: usize) -> Self {
            Self { checks_left: std::cell::Cell::new(n), progress_calls: 0, last: ScanProgress::default() }
        }
    }

    impl ScanObserver for CancelAfter {
        fn cancelled(&self) -> bool {
            let left = self.checks_left.get();
            if left == 0 {
                return true;
            }
            self.checks_left.set(left - 1);
            false
        }
        fn progress(&mut self, progress: &ScanProgress) {
            self.progress_calls += 1;
            self.last = progress.clone();
        }
    }

    fn eager() -> ScanOptions {
        ScanOptions { batch_size: 1, max_batch_time: Duration::from_secs(60), progress_interval: Duration::ZERO }
    }

    #[test]
    fn cancelled_scan_never_marks_files_missing() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        fs::remove_file(dir.path().join("snes/Super Game (USA).sfc")).unwrap();

        let mut observer = CancelAfter::new(3);
        let summary = scan_with(&c, dir.path(), &local, &mut observer, eager()).unwrap();
        assert!(summary.cancelled);
        assert!(!summary.complete);
        assert_eq!(summary.marked_missing, 0);
        assert!(statuses(&c).iter().all(|(_, s)| s == "present"), "QA-002: no false missing after cancel");
        let status: String = c.query_row("SELECT last_scan_status FROM indexed_roots", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "cancelled");
    }

    #[test]
    fn cancelled_scan_keeps_files_already_recorded() {
        let dir = fixture();
        let c = catalog();
        // Enough checks to get past the root and into some files, but not all.
        let mut observer = CancelAfter::new(8);
        let summary = scan_with(&c, dir.path(), &local, &mut observer, eager()).unwrap();
        assert!(summary.cancelled);
        let kept: i64 = c.query_row("SELECT COUNT(*) FROM file_locations", [], |r| r.get(0)).unwrap();
        assert_eq!(kept as usize, summary.indexed);
        assert!(summary.indexed < 4, "scan should have stopped early: {summary:?}");
    }

    #[test]
    fn progress_is_reported_and_final_counts_match() {
        let dir = fixture();
        let c = catalog();
        let mut observer = CancelAfter::new(usize::MAX);
        let summary = scan_with(&c, dir.path(), &local, &mut observer, eager()).unwrap();
        assert!(summary.complete);
        assert!(observer.progress_calls > 1);
        assert_eq!(observer.last.indexed, 4);
        assert_eq!(observer.last.new_files, 4);
        assert!(observer.last.entries_checked >= 8);
        assert!(observer.last.current_path.is_empty(), "final progress clears the current path");
    }

    #[test]
    fn batched_commits_produce_the_same_catalog() {
        let dir = fixture();
        let c = catalog();
        scan_with(&c, dir.path(), &local, &mut Unobserved, eager()).unwrap();
        let mut c2 = catalog();
        scan(&mut c2, dir.path(), &local).unwrap();
        assert_eq!(statuses(&c), statuses(&c2));
        // No transaction is left open after a scan.
        assert!(c.is_autocommit());
    }

    #[test]
    fn errors_leave_no_open_transaction() {
        let dir = fixture();
        let c = catalog();
        c.execute_batch("DROP TABLE file_locations;").unwrap();
        assert!(scan_with(&c, dir.path(), &local, &mut Unobserved, eager()).is_err());
        assert!(c.is_autocommit(), "failed batch must be rolled back");
    }

    #[test]
    fn drive_letter_change_is_recognized_without_duplicates() {
        let dir = fixture();
        let mut c = catalog();
        let usb = fingerprint("vol:ABCD-1234:1");
        let first = scan(&mut c, dir.path(), &usb).unwrap();
        assert_eq!(first.remounted_from, None);
        // Pretend the drive was last seen under another letter.
        c.execute("UPDATE storage_locations SET root_path = 'Q:\\'", []).unwrap();

        let second = scan(&mut c, dir.path(), &usb).unwrap();
        assert_eq!(second.remounted_from.as_deref(), Some("Q:\\"));
        assert_eq!(second.new_files, 0, "same files, so nothing is added twice");
        assert_eq!(statuses(&c).len(), 4);
        assert!(statuses(&c).iter().all(|(_, s)| s == "present"));
        let locations: i64 = c.query_row("SELECT COUNT(*) FROM storage_locations", [], |r| r.get(0)).unwrap();
        assert_eq!(locations, 1);
    }

    #[test]
    fn a_different_drive_at_the_same_letter_does_not_inherit_files() {
        let dir = fixture();
        let mut c = catalog();
        scan(&mut c, dir.path(), &fingerprint("vol:AAAA-AAAA:1")).unwrap();
        let other = scan(&mut c, dir.path(), &fingerprint("vol:BBBB-BBBB:1")).unwrap();
        assert_eq!(other.new_files, 4, "files on the other volume are new records");
        assert_eq!(other.marked_missing, 0, "the first drive's files are untouched");
        let locations: i64 = c.query_row("SELECT COUNT(*) FROM storage_locations", [], |r| r.get(0)).unwrap();
        assert_eq!(locations, 2);
    }

    #[cfg(windows)]
    #[test]
    fn paths_longer_than_260_characters_are_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().join("snes");
        while deep.as_os_str().len() < 300 {
            deep.push("a fairly long folder name for testing");
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Long Path Game.sfc"), b"rom").unwrap();
        assert!(deep.join("Long Path Game.sfc").as_os_str().len() > 260);

        let mut c = catalog();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(summary.indexed, 1, "{:?}", summary.issues);
        assert!(summary.complete);
        let (root, rel): (String, String) = c
            .query_row("SELECT s.root_path, fl.relative_path FROM file_locations fl JOIN storage_locations s ON s.id = fl.storage_location_id", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert!(!root.starts_with(r"\\?\"), "{root}");
        assert!(paths::join(&root, &rel).exists(), "stored long path must resolve");

        // Scanning the deep folder itself (root path > 260 characters) also works.
        let summary = scan(&mut c, &deep, &local).unwrap();
        assert_eq!(summary.new_files, 0, "same file, same record");
        let roots: i64 = c.query_row("SELECT COUNT(*) FROM storage_locations", [], |r| r.get(0)).unwrap();
        assert_eq!(roots, 1, r"no separate \\?\ volume created");
    }

    // ---- RDS-005: multi-file games ----

    fn files_by_role(c: &Connection) -> Vec<(String, String, Option<i64>)> {
        let mut s = c.prepare("SELECT filename, file_role, disc_index FROM game_files ORDER BY file_role, filename").unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    fn count_rows(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn disc_game(root: &Path) {
        write(root, "psx/Single (USA)/Single (USA).cue", b"FILE \"Single (USA) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE \"Single (USA) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n");
        write(root, "psx/Single (USA)/Single (USA) (Track 1).bin", &[0u8; 1000]);
        write(root, "psx/Single (USA)/Single (USA) (Track 2).bin", &[0u8; 500]);
    }

    fn multi_disc_game(root: &Path) {
        write(root, "psx/Saga (USA)/Saga (USA).m3u", b"# discs\nSaga (USA) (Disc 1).cue\nSaga (USA) (Disc 2).cue\n");
        write(root, "psx/Saga (USA)/Saga (USA) (Disc 1).cue", b"FILE Saga_D1.bin BINARY\n");
        write(root, "psx/Saga (USA)/Saga (USA) (Disc 2).cue", b"file \"Saga_D2.bin\" binary\n");
        write(root, "psx/Saga (USA)/Saga_D1.bin", &[0u8; 2000]);
        write(root, "psx/Saga (USA)/Saga_D2.bin", &[0u8; 3000]);
    }

    #[test]
    fn cue_sheet_tracks_join_one_game() {
        let dir = tempfile::tempdir().unwrap();
        disc_game(dir.path());
        let mut c = catalog();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert!(summary.complete, "{:?}", summary.issues);
        assert_eq!(summary.grouped_files, 2);
        assert_eq!(count_rows(&c, "SELECT COUNT(*) FROM games"), 1, "one game, not three");
        let roles = files_by_role(&c);
        assert_eq!(roles.iter().filter(|r| r.1 == "track").count(), 2);
        assert_eq!(roles.iter().filter(|r| r.1 == "primary").map(|r| r.0.as_str()).collect::<Vec<_>>(), ["Single (USA).cue"]);
        let payload: i64 = c.query_row("SELECT payload_bytes FROM game_files WHERE file_role = 'primary'", [], |r| r.get(0)).unwrap();
        let cue_len = fs::metadata(dir.path().join("psx/Single (USA)/Single (USA).cue")).unwrap().len() as i64;
        assert_eq!(payload, cue_len + 1500, "total size includes the tracks");
        assert_eq!(count_rows(&c, "SELECT COUNT(*) FROM file_locations"), 3, "tracks have locations for presence checks");
    }

    #[test]
    fn playlist_discs_join_one_game_in_order() {
        let dir = tempfile::tempdir().unwrap();
        multi_disc_game(dir.path());
        let mut c = catalog();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert!(summary.complete, "{:?}", summary.issues);
        assert_eq!(count_rows(&c, "SELECT COUNT(*) FROM games"), 1);
        assert_eq!(summary.games, 1, "{summary:?}");
        assert_eq!(summary.skipped_unsupported, 0, "grouped tracks are not reported as ignored");
        let title: String = c.query_row("SELECT canonical_title FROM games", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "Saga (USA)");
        let roles = files_by_role(&c);
        let discs: Vec<(&str, Option<i64>)> = roles.iter().filter(|r| r.1 == "disc").map(|r| (r.0.as_str(), r.2)).collect();
        assert_eq!(discs, [("Saga (USA) (Disc 1).cue", Some(1)), ("Saga (USA) (Disc 2).cue", Some(2))]);
        assert_eq!(roles.iter().filter(|r| r.1 == "track").count(), 2, "unquoted and lower-case FILE lines both parsed");
        assert_eq!(roles.iter().filter(|r| r.1 == "primary").count(), 1);
        let payload: i64 = c.query_row("SELECT payload_bytes FROM game_files WHERE file_role = 'primary'", [], |r| r.get(0)).unwrap();
        assert!(payload >= 5000, "{payload}");
    }

    #[test]
    fn rescans_keep_grouping_without_duplicates_or_false_missing() {
        let dir = tempfile::tempdir().unwrap();
        disc_game(dir.path());
        multi_disc_game(dir.path());
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        let before = files_by_role(&c);
        let again = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(again.new_files, 0);
        assert_eq!(again.marked_missing, 0, "tracks reached through cue sheets are seen too");
        assert_eq!(files_by_role(&c), before);
        assert_eq!(count_rows(&c, "SELECT COUNT(*) FROM games"), 2);
    }

    #[test]
    fn missing_track_is_a_warning_and_marks_the_game_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        disc_game(dir.path());
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        fs::remove_file(dir.path().join("psx/Single (USA)/Single (USA) (Track 2).bin")).unwrap();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert!(summary.complete, "a broken reference is a warning, not a read error");
        assert_eq!(summary.warning_count, 1, "{:?}", summary.issues);
        assert_eq!(summary.issues[0].severity, "warning");
        assert_eq!(summary.marked_missing, 1, "the vanished track's record is marked missing");
        let missing_parts: i64 = c.query_row("SELECT missing_parts FROM game_files WHERE file_role = 'primary'", [], |r| r.get(0)).unwrap();
        assert_eq!(missing_parts, 1);
    }

    #[test]
    fn references_outside_the_scanned_folder_are_refused() {
        let outer = tempfile::tempdir().unwrap();
        write(outer.path(), "outside/Secret.cue", b"FILE \"Secret.bin\" BINARY\n");
        write(outer.path(), "outside/Secret.bin", b"x");
        write(outer.path(), "lib/psx/Trap/Trap.m3u", b"..\\..\\..\\outside\\Secret.cue\n../../../outside/Secret.bin\n");
        let mut c = catalog();
        let summary = scan(&mut c, &outer.path().join("lib"), &local).unwrap();
        assert_eq!(summary.warning_count, 2, "{:?}", summary.issues);
        assert!(summary.issues.iter().all(|i| i.message.contains("outside the scanned folder")));
        let outside: i64 = count_rows(&c, "SELECT COUNT(*) FROM file_locations WHERE relative_path LIKE '%outside%'");
        assert_eq!(outside, 0, "nothing outside the authorized folder is recorded");
    }

    #[test]
    fn removing_a_playlist_returns_its_discs_to_separate_games() {
        let dir = tempfile::tempdir().unwrap();
        multi_disc_game(dir.path());
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        fs::remove_file(dir.path().join("psx/Saga (USA)/Saga (USA).m3u")).unwrap();
        scan(&mut c, dir.path(), &local).unwrap();
        let primaries: Vec<String> = files_by_role(&c).into_iter().filter(|r| r.1 == "primary").map(|r| r.0).collect();
        assert_eq!(primaries, ["Saga (USA) (Disc 1).cue", "Saga (USA) (Disc 2).cue", "Saga (USA).m3u"], "the deleted playlist's record stays, marked missing");
        let disc_games: i64 = count_rows(
            &c,
            "SELECT COUNT(DISTINCT gf.release_id) FROM game_files gf WHERE gf.file_role = 'primary' AND gf.extension = 'cue'",
        );
        assert_eq!(disc_games, 2, "each disc is its own game again");
        let tracks_follow: i64 = count_rows(
            &c,
            "SELECT COUNT(*) FROM game_files t JOIN game_files p ON p.id = t.parent_file_id WHERE t.file_role = 'track' AND t.release_id = p.release_id",
        );
        assert_eq!(tracks_follow, 2, "tracks moved with their disc");
    }

    #[test]
    fn cancelled_scan_does_not_split_grouped_games() {
        let dir = tempfile::tempdir().unwrap();
        multi_disc_game(dir.path());
        let mut c = catalog();
        scan(&mut c, dir.path(), &local).unwrap();
        let before = files_by_role(&c);
        let mut observer = CancelAfter::new(4);
        let summary = scan_with(&c, dir.path(), &local, &mut observer, eager()).unwrap();
        assert!(summary.cancelled);
        assert_eq!(files_by_role(&c), before);
    }

    /// QA-009: a file held open exclusively by another program.
    #[cfg(windows)]
    #[test]
    fn locked_files_are_reported_and_the_scan_continues() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        disc_game(dir.path());
        write(dir.path(), "snes/Locked Rom.sfc", b"rom");
        write(dir.path(), "snes/Other.sfc", b"rom");
        let lock = |p: &str| fs::OpenOptions::new().read(true).share_mode(0).open(dir.path().join(p)).unwrap();
        let _cue_lock = lock("psx/Single (USA)/Single (USA).cue");
        let _rom_lock = lock("snes/Locked Rom.sfc");

        let mut c = catalog();
        let summary = scan(&mut c, dir.path(), &local).unwrap();
        assert_eq!(summary.error_count, 1, "only the cue sheet's contents are unreadable: {:?}", summary.issues);
        assert!(summary.issues[0].message.starts_with("Cannot read"), "{:?}", summary.issues);
        assert!(!summary.complete, "unreadable entries make the scan incomplete");
        let titles: Vec<String> = c.prepare("SELECT canonical_title FROM games ORDER BY 1").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(titles, ["Locked Rom", "Other", "Single (USA)"], "locked files are still indexed from their directory entry");
    }

    #[test]
    fn box_art_is_linked_from_gamelists_and_folder_conventions_without_copying() {
        let unknown = |_: &str| VolumeProbe::unknown();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        let put = |rel: &str, body: &[u8]| {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        };
        put("nes/Tom & Jerry (USA).zip", b"rom");
        put("nes/Kid Icarus (USA).zip", b"rom");
        put("nes/No Art (USA).zip", b"rom");
        put("nes/media/screenshot/Tom & Jerry (USA).png", b"png");
        put("nes/images/Kid Icarus (USA)-image.jpg", b"jpg");
        put("nes/media/screenshot/Orphan.png", b"png");
        put("outside.png", b"png");
        put(
            "nes/gamelist.xml",
            br#"<gameList><game><path>./Tom &amp; Jerry (USA).zip</path><image>./media/screenshot/Tom &amp; Jerry (USA).png</image></game>
<game><path>./No Art (USA).zip</path><image>../../outside.png</image><marquee>./media/screenshot/Orphan.png</marquee></game></gameList>"#,
        );
        let before: Vec<_> = walkdir::WalkDir::new(&root).sort_by_file_name().into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();
        let mut c = catalog();
        let summary = scan(&mut c, &root, &unknown).unwrap();
        assert_eq!(summary.games, 3);
        assert_eq!(summary.images_found, 4);
        assert_eq!(summary.images_linked, 2);
        assert_eq!(summary.skipped_unsupported, 1, "only gamelist.xml; images are counted separately");
        let art = |file: &str| -> Option<(String, String, String)> {
            c.query_row(
                "SELECT m.kind, m.source, m.relative_path FROM game_media m JOIN game_files gf ON gf.id = m.game_file_id WHERE gf.filename = ?1",
                [file],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .unwrap()
        };
        let tom = art("Tom & Jerry (USA).zip").unwrap();
        assert_eq!((tom.0.as_str(), tom.1.as_str()), ("image", "gamelist"));
        assert!(tom.2.ends_with("nes/media/screenshot/Tom & Jerry (USA).png"), "{}", tom.2);
        let kid = art("Kid Icarus (USA).zip").unwrap();
        assert_eq!((kid.0.as_str(), kid.1.as_str()), ("image", "folder"));
        assert_eq!(art("No Art (USA).zip"), None, "an image outside the scanned folder and a marquee logo are not used");
        let after: Vec<_> = walkdir::WalkDir::new(&root).sort_by_file_name().into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();
        assert_eq!(before, after, "scanning never creates or copies files");

        // The user's own pick survives rescans; scanned links aren't duplicated.
        let kid_id: String = c.query_row("SELECT id FROM game_files WHERE filename = 'Kid Icarus (USA).zip'", [], |r| r.get(0)).unwrap();
        let picked = dir.path().join("my art.png");
        fs::write(&picked, b"png").unwrap();
        media::set_user_art(&c, &kid_id, &picked, &unknown).unwrap();
        scan(&mut c, &root, &unknown).unwrap();
        let rows: i64 = c.query_row("SELECT COUNT(*) FROM game_media", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 3, "two scanned links plus the user's pick");
        let best = media::resolve(&c, &kid_id, &|_, _| crate::volumes::Presence::Connected).unwrap().unwrap();
        assert!(best.0.ends_with("my art.png"), "the user's pick wins: {:?}", best.0);
        assert_eq!(best.1, "image/png");
        assert!(media::remove_user_art(&c, &kid_id).unwrap());
        let best = media::resolve(&c, &kid_id, &|_, _| crate::volumes::Presence::Connected).unwrap().unwrap();
        assert!(best.0.to_string_lossy().ends_with("Kid Icarus (USA)-image.jpg"));
        assert!(picked.exists(), "removing art never deletes the image file");
        assert_eq!(media::resolve(&c, &kid_id, &|_, _| crate::volumes::Presence::NotConnected).unwrap(), None, "nothing is served from a drive that isn't connected");
        assert!(media::set_user_art(&c, &kid_id, &root.join("nes/Kid Icarus (USA).zip"), &unknown).unwrap_err().contains("image file"));
    }

    #[test]
    fn rescans_move_games_to_newly_recognized_systems_and_mark_bios() {
        let unknown = |_: &str| VolumeProbe::unknown();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        fs::create_dir_all(root.join("c64")).unwrap();
        fs::create_dir_all(root.join("gba")).unwrap();
        fs::write(root.join("c64/Pyramid Quiz (USA).zip"), b"c64").unwrap();
        fs::write(root.join("gba/[BIOS] Game Boy Advance (World).zip"), b"bios").unwrap();
        let mut c = catalog();
        scan(&mut c, &root, &unknown).unwrap();
        let platform = |c: &Connection, file: &str| -> String {
            c.query_row(
                "SELECT g.platform FROM games g JOIN game_releases r ON r.game_id = g.id JOIN game_files gf ON gf.release_id = r.id WHERE gf.filename = ?1",
                [file],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(platform(&c, "Pyramid Quiz (USA).zip"), "Commodore 64");
        assert_eq!(platform(&c, "[BIOS] Game Boy Advance (World).zip"), "BIOS", "BIOS files aren't listed as GBA games");

        // Simulate a catalog from an older build that didn't know the c64 folder.
        c.execute("UPDATE games SET platform = 'Unclassified' WHERE canonical_title LIKE 'Pyramid Quiz%'", []).unwrap();
        scan(&mut c, &root, &unknown).unwrap();
        assert_eq!(platform(&c, "Pyramid Quiz (USA).zip"), "Commodore 64", "a rescan moves the game to its system");
        let found: i64 = c.query_row("SELECT COUNT(*) FROM games_fts WHERE games_fts MATCH 'Commodore'", [], |r| r.get(0)).unwrap();
        assert_eq!(found, 1, "search follows the new system");
    }
}
