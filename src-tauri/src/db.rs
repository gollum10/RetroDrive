//! SQLite catalog: connection setup and versioned schema migrations (RDS-001).
//!
//! Schema versions are tracked with `PRAGMA user_version`.
//! - v0: the v0.1 starter's single file-centric `game_files` table (no version set).
//! - v1: normalized hybrid library (storage locations, indexed roots, games,
//!   releases, files and file locations) as described in docs/05-DATA-MODEL.md.
//! - v2: durable job log (`jobs`, `job_events`) for scans and later operations (RDS-002).
//! - v3: volume identity on `storage_locations` (stable_id unique, root_path no longer
//!   unique, label/filesystem/serial/GUID/size columns) so drives are matched by
//!   fingerprint rather than drive letter (RDS-003).
//! - v4: `games_fts` trigram full-text index kept in sync by triggers, plus indexes
//!   for duplicate candidates and availability filters (RDS-004).
//! - v5: multi-file games. `game_files.file_role` (`primary` / `disc` / `track`),
//!   `parent_file_id`, and `payload_bytes` (total size of a primary file's game)
//!   so `.cue`+`.bin` sets and `.m3u` multi-disc games count once (RDS-005).
//! - v6: `installations` — platform installations recognized by an adapter (RDS-006/007).
//!
//! Migration rules: back up an existing database before upgrading, run the upgrade
//! in one transaction, verify row counts, and refuse to open a database written by
//! a newer schema than this build understands.

use crate::paths;
use crate::volumes::{VolumeIdentity, VolumeProbe};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const SCHEMA_VERSION: i32 = 19;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Open a catalog connection with the pragmas every connection needs.
pub fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|e| format!("Cannot open catalog {}: {e}", path.display()))?;
    configure(&connection)?;
    Ok(connection)
}

pub fn configure(connection: &Connection) -> Result<(), String> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|e| e.to_string())
}

#[derive(Debug, PartialEq)]
pub struct MigrationReport {
    pub from_version: i32,
    pub to_version: i32,
    pub legacy_rows_migrated: usize,
    pub backup_path: Option<PathBuf>,
}

fn user_version(connection: &Connection) -> Result<i32, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
        .map_err(|e| e.to_string())
}

fn has_user_tables(connection: &Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

/// Bring the catalog to `SCHEMA_VERSION`. `db_path` is used to place a backup copy
/// next to the database before an existing catalog is upgraded; pass `None` for
/// in-memory databases (tests).
pub fn migrate(connection: &mut Connection, db_path: Option<&Path>) -> Result<MigrationReport, String> {
    let from_version = user_version(connection)?;
    if from_version > SCHEMA_VERSION {
        return Err(format!(
            "The catalog was written by a newer RetroDrive (schema v{from_version}; this build supports v{SCHEMA_VERSION}). \
             It was not modified. Update the application to open it."
        ));
    }
    let mut report = MigrationReport { from_version, to_version: from_version, legacy_rows_migrated: 0, backup_path: None };
    if from_version == SCHEMA_VERSION {
        return Ok(report);
    }

    if let Some(path) = db_path {
        if has_user_tables(connection)? {
            report.backup_path = Some(backup(connection, path, from_version)?);
        }
    }

    // Table rebuilds need foreign-key enforcement off, and that pragma only takes
    // effect outside a transaction. Integrity is re-checked before commit.
    connection.execute_batch("PRAGMA foreign_keys = OFF;").map_err(|e| e.to_string())?;
    let result = run_steps(connection, from_version, &mut report);
    let restored = connection.execute_batch("PRAGMA foreign_keys = ON;").map_err(|e| e.to_string());
    result?;
    restored?;
    report.to_version = SCHEMA_VERSION;
    Ok(report)
}

fn run_steps(connection: &mut Connection, from_version: i32, report: &mut MigrationReport) -> Result<(), String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    if from_version < 1 {
        let legacy = table_exists(&tx, "game_files")? && !table_exists(&tx, "file_locations")?;
        if legacy {
            tx.execute_batch(
                "DROP INDEX IF EXISTS idx_game_files_platform;
                 ALTER TABLE game_files RENAME TO legacy_v0_game_files;",
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute_batch(SCHEMA_V1).map_err(|e| e.to_string())?;
        if legacy {
            report.legacy_rows_migrated = migrate_v0_rows(&tx)?;
            tx.execute_batch("DROP TABLE legacy_v0_game_files;").map_err(|e| e.to_string())?;
        }
    }
    if from_version < 2 {
        tx.execute_batch(SCHEMA_V2).map_err(|e| e.to_string())?;
    }
    if from_version < 3 {
        tx.execute_batch(SCHEMA_V3).map_err(|e| e.to_string())?;
    }
    if from_version < 4 {
        tx.execute_batch(SCHEMA_V4).map_err(|e| e.to_string())?;
    }
    if from_version < 5 {
        tx.execute_batch(SCHEMA_V5).map_err(|e| e.to_string())?;
    }
    if from_version < 6 {
        tx.execute_batch(SCHEMA_V6).map_err(|e| e.to_string())?;
    }
    if from_version < 7 {
        tx.execute_batch(SCHEMA_V7).map_err(|e| e.to_string())?;
    }
    if from_version < 8 {
        tx.execute_batch(SCHEMA_V8).map_err(|e| e.to_string())?;
    }
    if from_version < 9 {
        tx.execute_batch(SCHEMA_V9).map_err(|e| e.to_string())?;
    }
    if from_version < 10 {
        tx.execute_batch(SCHEMA_V10).map_err(|e| e.to_string())?;
    }
    if from_version < 11 {
        tx.execute_batch(SCHEMA_V11).map_err(|e| e.to_string())?;
    }
    if from_version < 12 {
        tx.execute_batch(SCHEMA_V12).map_err(|e| e.to_string())?;
    }
    if from_version < 13 {
        tx.execute_batch(SCHEMA_V13).map_err(|e| e.to_string())?;
    }
    if from_version < 14 {
        tx.execute_batch(SCHEMA_V14).map_err(|e| e.to_string())?;
    }
    if from_version < 15 {
        tx.execute_batch(SCHEMA_V15).map_err(|e| e.to_string())?;
    }
    if from_version < 16 {
        tx.execute_batch(SCHEMA_V16).map_err(|e| e.to_string())?;
    }
    if from_version < 17 {
        tx.execute_batch(SCHEMA_V17).map_err(|e| e.to_string())?;
    }
    if from_version < 18 {
        tx.execute_batch(SCHEMA_V18).map_err(|e| e.to_string())?;
    }
    if from_version < 19 {
        tx.execute_batch(SCHEMA_V19).map_err(|e| e.to_string())?;
    }
    let violations: i64 = tx
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if violations > 0 {
        return Err(format!("Migration aborted and rolled back: {violations} broken references found."));
    }
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// Write a consistent copy of the database next to it before migrating.
fn backup(connection: &Connection, db_path: &Path, from_version: i32) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = db_path.file_name().and_then(|s| s.to_str()).unwrap_or("retrodrive.sqlite");
    let backup_path = db_path.with_file_name(format!("{file_name}.v{from_version}-backup-{stamp}"));
    let target = backup_path.to_str().ok_or("Backup path is not valid Unicode")?;
    connection
        .execute("VACUUM INTO ?1", [target])
        .map_err(|e| format!("Could not back up the catalog before migrating; nothing was changed: {e}"))?;
    Ok(backup_path)
}

/// Copy every v0.1 row into the normalized tables. Each legacy row becomes its own
/// game/release/file/location: rows are never merged by title (docs/05).
fn migrate_v0_rows(tx: &Connection) -> Result<usize, String> {
    let legacy_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM legacy_v0_game_files", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String, i64, String)> = {
        let mut statement = tx
            .prepare("SELECT path, title, platform, file_size, last_seen FROM legacy_v0_game_files ORDER BY id")
            .map_err(|e| e.to_string())?;
        let mapped = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .map_err(|e| e.to_string())?;
        mapped.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };

    let mut migrated = 0;
    for (raw_path, title, platform, size, last_seen) in rows {
        let clean = paths::strip_verbatim(&raw_path);
        let (volume_root, relative) = paths::split_volume(Path::new(&clean))
            .ok_or_else(|| format!("Migration aborted: cannot interpret stored path {raw_path:?}"))?;
        let storage_id = upsert_storage_location(tx, &volume_root, "unknown")?;
        let filename = Path::new(&clean).file_name().and_then(|s| s.to_str()).unwrap_or(&title).to_string();
        let extension = Path::new(&clean)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let file_id = insert_game_file_v1(tx, &title, &platform, &filename, &extension, size, "legacy")?;
        tx.execute(
            "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path, first_seen_at, last_seen_at, present_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 'present')
             ON CONFLICT(storage_location_id, relative_path) DO NOTHING",
            params![new_id(), file_id, storage_id, relative, last_seen],
        )
        .map_err(|e| e.to_string())?;
        migrated += 1;
    }

    let location_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM file_locations", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if location_count != legacy_count {
        return Err(format!(
            "Migration aborted and rolled back: {legacy_count} legacy rows but {location_count} file locations were produced."
        ));
    }
    Ok(migrated)
}

/// The storage location a scan should use, and the root it was previously seen
/// at when the same volume came back under a different drive letter.
#[derive(Debug, PartialEq)]
pub struct ResolvedLocation {
    pub id: String,
    pub previous_root: Option<String>,
}

/// Match the volume mounted at `root` to a catalog storage location (RDS-003):
/// 1. same fingerprint → that location (updating its root if the letter changed);
/// 2. an older, unidentified location at this root whose files are on this volume
///    → claim it by recording the fingerprint;
/// 3. otherwise a new location. A different volume using a known letter therefore
///    never inherits another drive's files.
pub fn resolve_storage_location(connection: &Connection, root: &str, probe: &VolumeProbe) -> Result<ResolvedLocation, String> {
    let Some(identity) = &probe.identity else {
        let existing = connection
            .query_row(
                "SELECT id FROM storage_locations WHERE root_path = ?1 AND stable_id IS NULL ORDER BY created_at LIMIT 1",
                [root],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let id = match existing {
            Some(id) => id,
            None => insert_location(connection, root, probe.kind, None)?,
        };
        return Ok(ResolvedLocation { id, previous_root: None });
    };

    let by_fingerprint = connection
        .query_row(
            "SELECT id, root_path FROM storage_locations WHERE stable_id = ?1",
            [&identity.stable_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((id, old_root)) = by_fingerprint {
        update_identity(connection, &id, root, probe.kind, identity)?;
        let moved = !old_root.eq_ignore_ascii_case(root);
        return Ok(ResolvedLocation { id, previous_root: moved.then_some(old_root) });
    }

    let unidentified: Vec<String> = {
        let mut statement = connection
            .prepare("SELECT id FROM storage_locations WHERE root_path = ?1 AND stable_id IS NULL ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map([root], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    for id in unidentified {
        if files_found_on_volume(connection, &id, root)? {
            update_identity(connection, &id, root, probe.kind, identity)?;
            return Ok(ResolvedLocation { id, previous_root: None });
        }
    }
    let id = insert_location(connection, root, probe.kind, Some(identity))?;
    Ok(ResolvedLocation { id, previous_root: None })
}

fn display_name(root: &str, identity: Option<&VolumeIdentity>) -> String {
    match identity.and_then(|i| i.label.as_deref()) {
        Some(label) => format!("{label} ({root})"),
        None => root.to_string(),
    }
}

fn insert_location(connection: &Connection, root: &str, kind: &str, identity: Option<&VolumeIdentity>) -> Result<String, String> {
    let id = new_id();
    connection
        .execute(
            "INSERT INTO storage_locations (id, kind, stable_id, display_name, root_path, volume_label, filesystem, volume_serial, volume_guid, total_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                kind,
                identity.map(|i| &i.stable_id),
                display_name(root, identity),
                root,
                identity.and_then(|i| i.label.as_ref()),
                identity.and_then(|i| i.filesystem.as_ref()),
                identity.and_then(|i| i.serial.as_ref()),
                identity.and_then(|i| i.guid.as_ref()),
                identity.and_then(|i| i.total_bytes),
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(id)
}

fn update_identity(connection: &Connection, id: &str, root: &str, kind: &str, identity: &VolumeIdentity) -> Result<(), String> {
    connection
        .execute(
            "UPDATE storage_locations SET root_path = ?2, stable_id = ?3, display_name = ?4, volume_label = ?5, filesystem = ?6,
                 volume_serial = ?7, volume_guid = ?8, total_bytes = ?9,
                 kind = CASE WHEN ?10 <> 'unknown' THEN ?10 ELSE kind END
             WHERE id = ?1",
            params![
                id,
                root,
                identity.stable_id,
                display_name(root, Some(identity)),
                identity.label,
                identity.filesystem,
                identity.serial,
                identity.guid,
                identity.total_bytes,
                kind
            ],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// True when a location has no files yet, or some of its recorded files exist under `root`.
fn files_found_on_volume(connection: &Connection, id: &str, root: &str) -> Result<bool, String> {
    let sample: Vec<String> = {
        let mut statement = connection
            .prepare("SELECT relative_path FROM file_locations WHERE storage_location_id = ?1 ORDER BY present_status DESC LIMIT 25")
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    Ok(sample.is_empty() || sample.iter().any(|relative| paths::join(root, relative).exists()))
}

/// Find a storage location by its root path, creating it when absent. Used for
/// migrated v0.1 data, which has no volume fingerprint.
pub fn upsert_storage_location(tx: &Connection, root_path: &str, kind: &str) -> Result<String, String> {
    if let Some(id) = tx
        .query_row("SELECT id FROM storage_locations WHERE root_path = ?1 ORDER BY created_at LIMIT 1", [root_path], |r| r.get::<_, String>(0))
        .optional()
        .map_err(|e| e.to_string())?
    {
        if kind != "unknown" {
            tx.execute(
                "UPDATE storage_locations SET kind = ?2 WHERE id = ?1 AND kind = 'unknown'",
                params![id, kind],
            )
            .map_err(|e| e.to_string())?;
        }
        return Ok(id);
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO storage_locations (id, kind, display_name, root_path) VALUES (?1, ?2, ?3, ?3)",
        params![id, kind, root_path],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Create a game, an unmatched release and a file record; returns the file id.
#[allow(clippy::too_many_arguments)]
pub fn insert_game_file(
    tx: &Connection,
    title: &str,
    platform: &str,
    filename: &str,
    extension: &str,
    size: i64,
    modified_at: Option<&str>,
    hint_source: &str,
) -> Result<String, String> {
    let game_id = new_id();
    let release_id = new_id();
    let file_id = new_id();
    tx.execute(
        "INSERT INTO games (id, canonical_title, platform) VALUES (?1, ?2, ?3)",
        params![game_id, title, platform],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO game_releases (id, game_id) VALUES (?1, ?2)", params![release_id, game_id])
        .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO game_files (id, release_id, filename, extension, size_bytes, payload_bytes, modified_at, platform_hint, platform_hint_source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8)",
        params![file_id, release_id, filename, extension, size, modified_at, platform, hint_source],
    )
    .map_err(|e| e.to_string())?;
    Ok(file_id)
}

/// `insert_game_file` limited to the v1 columns, for migration steps that run
/// before later columns exist (and for tests that build old-schema catalogs).
fn insert_game_file_v1(tx: &Connection, title: &str, platform: &str, filename: &str, extension: &str, size: i64, hint_source: &str) -> Result<String, String> {
    let game_id = new_id();
    let release_id = new_id();
    let file_id = new_id();
    tx.execute("INSERT INTO games (id, canonical_title, platform) VALUES (?1, ?2, ?3)", params![game_id, title, platform])
        .map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO game_releases (id, game_id) VALUES (?1, ?2)", params![release_id, game_id])
        .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO game_files (id, release_id, filename, extension, size_bytes, platform_hint, platform_hint_source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![file_id, release_id, filename, extension, size, platform, hint_source],
    )
    .map_err(|e| e.to_string())?;
    Ok(file_id)
}

/// Add a file that belongs to an existing game: a disc listed by an `.m3u`
/// playlist or a track referenced by a `.cue` sheet. Returns the file id.
#[allow(clippy::too_many_arguments)]
pub fn insert_member_file(
    tx: &Connection,
    release_id: &str,
    parent_file_id: &str,
    role: &str,
    disc_index: Option<i64>,
    filename: &str,
    extension: &str,
    size: i64,
    modified_at: Option<&str>,
    platform: &str,
) -> Result<String, String> {
    let file_id = new_id();
    tx.execute(
        "INSERT INTO game_files (id, release_id, filename, extension, size_bytes, payload_bytes, modified_at, platform_hint,
                                 platform_hint_source, file_role, parent_file_id, disc_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, 'folder', ?8, ?9, ?10)",
        params![file_id, release_id, filename, extension, size, modified_at, platform, role, parent_file_id, disc_index],
    )
    .map_err(|e| e.to_string())?;
    Ok(file_id)
}

/// Move every file of release `from` into release `into`, then remove the emptied
/// release and its game. Only catalog records are affected, never files on disk.
pub fn merge_release(tx: &Connection, from: &str, into: &str) -> Result<(), String> {
    if from == into {
        return Ok(());
    }
    tx.execute("UPDATE game_files SET release_id = ?2 WHERE release_id = ?1", params![from, into])
        .map_err(|e| e.to_string())?;
    let game: Option<String> = tx
        .query_row("SELECT game_id FROM game_releases WHERE id = ?1", [from], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM game_releases WHERE id = ?1", [from]).map_err(|e| e.to_string())?;
    if let Some(game) = game {
        tx.execute(
            "DELETE FROM games WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM game_releases WHERE game_id = ?1)",
            [&game],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

const SCHEMA_V1: &str = "
CREATE TABLE storage_locations (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL DEFAULT 'unknown' CHECK (kind IN ('local','removable','network','unknown')),
    stable_id     TEXT,
    display_name  TEXT NOT NULL,
    root_path     TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at  TEXT
);

CREATE TABLE indexed_roots (
    id                     TEXT PRIMARY KEY,
    storage_location_id    TEXT NOT NULL REFERENCES storage_locations(id) ON DELETE RESTRICT,
    relative_path          TEXT NOT NULL COLLATE NOCASE,
    added_at               TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_scan_started_at   TEXT,
    last_scan_completed_at TEXT,
    last_scan_status       TEXT,
    UNIQUE (storage_location_id, relative_path)
);

CREATE TABLE games (
    id               TEXT PRIMARY KEY,
    canonical_title  TEXT NOT NULL,
    platform         TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_games_title ON games(canonical_title COLLATE NOCASE);
CREATE INDEX idx_games_platform ON games(platform);

CREATE TABLE game_releases (
    id                TEXT PRIMARY KEY,
    game_id           TEXT NOT NULL REFERENCES games(id) ON DELETE RESTRICT,
    region            TEXT,
    version           TEXT,
    publisher         TEXT,
    release_date      TEXT,
    media_type        TEXT,
    match_confidence  TEXT NOT NULL DEFAULT 'unmatched'
);
CREATE INDEX idx_game_releases_game ON game_releases(game_id);

CREATE TABLE game_files (
    id                    TEXT PRIMARY KEY,
    release_id            TEXT REFERENCES game_releases(id) ON DELETE RESTRICT,
    filename              TEXT NOT NULL,
    extension             TEXT NOT NULL,
    size_bytes            INTEGER NOT NULL DEFAULT 0,
    modified_at           TEXT,
    sha256                TEXT,
    crc32                 TEXT,
    disc_index            INTEGER,
    platform_hint         TEXT NOT NULL,
    platform_hint_source  TEXT NOT NULL CHECK (platform_hint_source IN ('folder','extension','ambiguous','legacy'))
);
CREATE INDEX idx_game_files_release ON game_files(release_id);
CREATE INDEX idx_game_files_sha256 ON game_files(sha256);

CREATE TABLE file_locations (
    id                   TEXT PRIMARY KEY,
    game_file_id         TEXT NOT NULL REFERENCES game_files(id) ON DELETE RESTRICT,
    storage_location_id  TEXT NOT NULL REFERENCES storage_locations(id) ON DELETE RESTRICT,
    relative_path        TEXT NOT NULL COLLATE NOCASE,
    first_seen_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at         TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_scan_id         TEXT,
    present_status       TEXT NOT NULL DEFAULT 'present' CHECK (present_status IN ('present','missing')),
    last_verified_at     TEXT,
    UNIQUE (storage_location_id, relative_path)
);
CREATE INDEX idx_file_locations_file ON file_locations(game_file_id);
";

const SCHEMA_V2: &str = "
CREATE TABLE jobs (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL,
    state          TEXT NOT NULL CHECK (state IN ('running','completed','cancelled','failed','interrupted')),
    input_json     TEXT NOT NULL DEFAULT '{}',
    progress_json  TEXT,
    started_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at    TEXT,
    error_code     TEXT,
    error_message  TEXT
);
CREATE INDEX idx_jobs_started ON jobs(started_at);
CREATE INDEX idx_jobs_state ON jobs(state);

CREATE TABLE job_events (
    id           INTEGER PRIMARY KEY,
    job_id       TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    event_time   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    severity     TEXT NOT NULL CHECK (severity IN ('info','warning','error')),
    event_code   TEXT NOT NULL,
    detail_json  TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_job_events_job ON job_events(job_id);
";

/// v3: rebuild storage_locations so root_path is no longer unique (a drive letter
/// can be reused by another volume) and the fingerprint is unique instead.
const SCHEMA_V3: &str = "
CREATE TABLE storage_locations_v3 (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL DEFAULT 'unknown' CHECK (kind IN ('local','removable','network','unknown')),
    stable_id      TEXT UNIQUE,
    display_name   TEXT NOT NULL,
    root_path      TEXT NOT NULL COLLATE NOCASE,
    volume_label   TEXT,
    filesystem     TEXT,
    volume_serial  TEXT,
    volume_guid    TEXT,
    total_bytes    INTEGER,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at   TEXT
);
INSERT INTO storage_locations_v3 (id, kind, stable_id, display_name, root_path, created_at, last_seen_at)
    SELECT id, kind, stable_id, display_name, root_path, created_at, last_seen_at FROM storage_locations;
DROP TABLE storage_locations;
ALTER TABLE storage_locations_v3 RENAME TO storage_locations;
CREATE INDEX idx_storage_locations_root ON storage_locations(root_path);
";

/// v4: title search and library query indexes.
/// The FTS table stores its own copy of title/platform keyed by `game_id` rather
/// than using external content, because `games` has no INTEGER PRIMARY KEY and its
/// implicit rowids may be renumbered by VACUUM.
const SCHEMA_V4: &str = "
CREATE VIRTUAL TABLE games_fts USING fts5(game_id UNINDEXED, canonical_title, platform, tokenize = 'trigram');
INSERT INTO games_fts (game_id, canonical_title, platform) SELECT id, canonical_title, platform FROM games;
CREATE TRIGGER games_fts_insert AFTER INSERT ON games BEGIN
    INSERT INTO games_fts (game_id, canonical_title, platform) VALUES (new.id, new.canonical_title, new.platform);
END;
CREATE TRIGGER games_fts_update AFTER UPDATE OF canonical_title, platform ON games BEGIN
    UPDATE games_fts SET canonical_title = new.canonical_title, platform = new.platform WHERE game_id = old.id;
END;
CREATE TRIGGER games_fts_delete AFTER DELETE ON games BEGIN
    DELETE FROM games_fts WHERE game_id = old.id;
END;
CREATE INDEX idx_game_files_duplicate_key ON game_files(size_bytes, filename COLLATE NOCASE, platform_hint);
CREATE INDEX idx_file_locations_storage_status ON file_locations(storage_location_id, present_status);
CREATE INDEX idx_games_platform_title ON games(platform, canonical_title COLLATE NOCASE);
";

/// v5: multi-file games (cue/bin tracks, m3u discs).
const SCHEMA_V5: &str = "
ALTER TABLE game_files ADD COLUMN file_role TEXT NOT NULL DEFAULT 'primary' CHECK (file_role IN ('primary','disc','track'));
ALTER TABLE game_files ADD COLUMN parent_file_id TEXT REFERENCES game_files(id) ON DELETE RESTRICT;
ALTER TABLE game_files ADD COLUMN payload_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE game_files ADD COLUMN missing_parts INTEGER NOT NULL DEFAULT 0;
UPDATE game_files SET payload_bytes = size_bytes;
CREATE INDEX idx_game_files_role_payload ON game_files(file_role, payload_bytes);
DROP INDEX idx_game_files_duplicate_key;
CREATE INDEX idx_game_files_duplicate_key ON game_files(size_bytes, filename COLLATE NOCASE, platform_hint, file_role);
CREATE INDEX idx_game_files_release_role ON game_files(release_id, file_role);
";

/// v6: saved platform installations (RetroBat folders etc.).
const SCHEMA_V6: &str = "
CREATE TABLE installations (
    id                   TEXT PRIMARY KEY,
    adapter_id           TEXT NOT NULL,
    adapter_contract     INTEGER NOT NULL,
    storage_location_id  TEXT NOT NULL REFERENCES storage_locations(id) ON DELETE RESTRICT,
    relative_path        TEXT NOT NULL COLLATE NOCASE,
    version_string       TEXT,
    added_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_checked_at      TEXT,
    UNIQUE (storage_location_id, relative_path)
);
";

/// v7 (RDS-010): deployment journal. A deployment and all its items are written
/// *before* any file is copied, and each item's state is updated as it moves
/// through copying -> verified, so an interrupted copy can always be explained and
/// its temporary file cleaned up. History survives forgetting an installation.
const SCHEMA_V7: &str = "
CREATE TABLE deployments (
    id                     TEXT PRIMARY KEY,
    job_id                 TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    installation_id        TEXT REFERENCES installations(id) ON DELETE SET NULL,
    adapter_id             TEXT NOT NULL,
    adapter_contract       INTEGER NOT NULL,
    destination_root       TEXT NOT NULL,
    destination_stable_id  TEXT,
    plan_fingerprint       TEXT NOT NULL,
    systems_json           TEXT,
    status                 TEXT NOT NULL,
    manifest_path          TEXT,
    created_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at            TEXT
);
CREATE TABLE deployment_items (
    id                INTEGER PRIMARY KEY,
    deployment_id     TEXT NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
    game_index        INTEGER NOT NULL,
    game_title        TEXT NOT NULL,
    game_file_id      TEXT REFERENCES game_files(id) ON DELETE SET NULL,
    source_path       TEXT NOT NULL,
    destination_path  TEXT NOT NULL,
    temp_path         TEXT,
    size_bytes        INTEGER NOT NULL,
    planned_action    TEXT NOT NULL,
    state             TEXT NOT NULL,
    sha256            TEXT,
    message           TEXT,
    updated_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_deployment_items_state ON deployment_items(deployment_id, state);
CREATE INDEX idx_deployments_status ON deployments(status);
";

/// v8: per-installation arcade choice. `NULL` = not chosen (arcade games aren't
/// placed); otherwise the emulator family whose romset the user has.
const SCHEMA_V8: &str = "
ALTER TABLE installations ADD COLUMN arcade_system TEXT CHECK (arcade_system IN ('mame', 'fbneo'));
";

/// v9: reference catalog (the user's own game list; `reference.rs`). Names and
/// details only: there is deliberately no column for artwork. Kept apart from the
/// library's own tables, so removing a catalog never changes the library.
const SCHEMA_V9: &str = "
CREATE TABLE reference_catalogs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    file_name     TEXT NOT NULL,
    imported_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    entries       INTEGER NOT NULL,
    genres_added  INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE reference_entries (
    id              INTEGER PRIMARY KEY,
    catalog_id      TEXT NOT NULL REFERENCES reference_catalogs(id) ON DELETE CASCADE,
    entry_key       TEXT,
    title           TEXT NOT NULL,
    platform        TEXT NOT NULL,
    platform_label  TEXT NOT NULL,
    year            INTEGER,
    genre           TEXT,
    publisher       TEXT,
    developer       TEXT,
    source_id       TEXT,
    source_name     TEXT,
    variants        TEXT,
    regions         TEXT
);
CREATE INDEX idx_reference_entries_key ON reference_entries(catalog_id, entry_key);
CREATE TABLE game_reference (
    game_id     TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    entry_id    INTEGER NOT NULL REFERENCES reference_entries(id) ON DELETE CASCADE,
    method      TEXT NOT NULL,
    matched_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_game_reference_entry ON game_reference(entry_id);
";

/// v10: game images (`media.rs`). Stores only where an image is; images are never
/// copied. `source` is `gamelist`, `folder` (found by a scan) or `user` (picked by hand).
const SCHEMA_V10: &str = "
CREATE TABLE game_media (
    id                   INTEGER PRIMARY KEY,
    game_file_id         TEXT NOT NULL REFERENCES game_files(id) ON DELETE CASCADE,
    kind                 TEXT NOT NULL,
    source               TEXT NOT NULL,
    storage_location_id  TEXT NOT NULL REFERENCES storage_locations(id) ON DELETE CASCADE,
    relative_path        TEXT NOT NULL,
    added_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (game_file_id, source, kind)
);
CREATE INDEX idx_game_media_file ON game_media(game_file_id);
";

/// v11: files the user left out of the library (`duplicates.rs`). Catalog marks
/// only; the files stay on disk. `excluded_reason` is `duplicate` or `user`.
const SCHEMA_V11: &str = "
ALTER TABLE game_files ADD COLUMN excluded_at TEXT;
ALTER TABLE game_files ADD COLUMN excluded_reason TEXT;
";

/// v12: where a replaced file's original was moved (`copier.rs`, Replace). Used to
/// restore it after a failure or crash; the backup itself is never deleted.
const SCHEMA_V12: &str = "
ALTER TABLE deployment_items ADD COLUMN backup_path TEXT;
";

/// v13: collections, the user's own game lists (`collections.rs`). Lists only;
/// deleting a collection never touches games or files.
const SCHEMA_V13: &str = "
CREATE TABLE collections (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE collection_games (
    collection_id  TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    game_file_id   TEXT NOT NULL REFERENCES game_files(id) ON DELETE CASCADE,
    added_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (collection_id, game_file_id)
);
CREATE INDEX idx_collection_games_file ON collection_games(game_file_id);
";

/// v14: `games.display_title`, the title the library shows: the reference catalog's
/// title when the game is matched, otherwise the file-based title. Kept in sync by
/// triggers, indexed for the title sort and the A-Z bar, and added to the search
/// index so catalog titles can be searched.
const SCHEMA_V14: &str = "
ALTER TABLE games ADD COLUMN display_title TEXT;
UPDATE games SET display_title = COALESCE(
    (SELECT re.title FROM game_reference gr JOIN reference_entries re ON re.id = gr.entry_id WHERE gr.game_id = games.id),
    canonical_title);
CREATE INDEX idx_games_display_title ON games(display_title COLLATE NOCASE);
CREATE INDEX idx_games_platform_display_title ON games(platform, display_title COLLATE NOCASE);

CREATE TRIGGER games_display_title_insert AFTER INSERT ON games WHEN new.display_title IS NULL BEGIN
    UPDATE games SET display_title = new.canonical_title WHERE id = new.id;
END;
CREATE TRIGGER games_display_title_rename AFTER UPDATE OF canonical_title ON games BEGIN
    UPDATE games SET display_title = COALESCE(
        (SELECT re.title FROM game_reference gr JOIN reference_entries re ON re.id = gr.entry_id WHERE gr.game_id = new.id),
        new.canonical_title) WHERE id = new.id;
END;
CREATE TRIGGER game_reference_display_insert AFTER INSERT ON game_reference BEGIN
    UPDATE games SET display_title = COALESCE((SELECT title FROM reference_entries WHERE id = new.entry_id), canonical_title) WHERE id = new.game_id;
END;
CREATE TRIGGER game_reference_display_update AFTER UPDATE ON game_reference BEGIN
    UPDATE games SET display_title = canonical_title WHERE id = old.game_id;
    UPDATE games SET display_title = COALESCE((SELECT title FROM reference_entries WHERE id = new.entry_id), canonical_title) WHERE id = new.game_id;
END;
CREATE TRIGGER game_reference_display_delete AFTER DELETE ON game_reference BEGIN
    UPDATE games SET display_title = canonical_title WHERE id = old.game_id;
END;

DROP TRIGGER games_fts_insert;
DROP TRIGGER games_fts_update;
DROP TRIGGER games_fts_delete;
DROP TABLE games_fts;
CREATE VIRTUAL TABLE games_fts USING fts5(game_id UNINDEXED, canonical_title, platform, display_title, tokenize = 'trigram');
INSERT INTO games_fts (game_id, canonical_title, platform, display_title) SELECT id, canonical_title, platform, display_title FROM games;
CREATE TRIGGER games_fts_insert AFTER INSERT ON games BEGIN
    INSERT INTO games_fts (game_id, canonical_title, platform, display_title) VALUES (new.id, new.canonical_title, new.platform, new.display_title);
END;
CREATE TRIGGER games_fts_update AFTER UPDATE OF canonical_title, platform, display_title ON games BEGIN
    UPDATE games_fts SET canonical_title = new.canonical_title, platform = new.platform, display_title = new.display_title WHERE game_id = old.id;
END;
CREATE TRIGGER games_fts_delete AFTER DELETE ON games BEGIN
    DELETE FROM games_fts WHERE game_id = old.id;
END;

-- Counting the library's games walks this small index instead of every file (measured: docs/15).
CREATE INDEX idx_game_files_included_role ON game_files(file_role) WHERE excluded_at IS NULL;
-- Table statistics let SQLite pick the title index for the A-Z bar instead of scanning every file.
ANALYZE;
";

/// v15: undoing a replace (`restore.rs`). `undo_path` is where RetroDrive's copy was
/// moved when the original was put back; item states add `undoing` and `undone`.
const SCHEMA_V15: &str = "
ALTER TABLE deployment_items ADD COLUMN undo_path TEXT;
";

/// v16: RetroBat settings changes (`customizer.rs`, docs/18 §12): one journaled row
/// per change, with the drive identity, the backup and temporary paths and both
/// file checksums, so a change can be undone or repaired after a crash.
const SCHEMA_V16: &str = "
CREATE TABLE settings_changes (
    id               INTEGER PRIMARY KEY,
    installation_id  TEXT REFERENCES installations(id) ON DELETE SET NULL,
    file_path        TEXT NOT NULL,
    root_path        TEXT NOT NULL,
    stable_id        TEXT,
    backup_path      TEXT,
    temp_path        TEXT,
    undo_path        TEXT,
    old_sha          TEXT NOT NULL,
    new_sha          TEXT NOT NULL,
    changes_json     TEXT NOT NULL,
    state            TEXT NOT NULL CHECK (state IN ('applying', 'applied', 'failed', 'undoing', 'undone')),
    message          TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_settings_changes_installation ON settings_changes(installation_id);
";

/// v17: designs (`designs.rs`): named RetroBat customizations kept in the catalog
/// (finished spec and unfinished draft), and their applications to installations.
const SCHEMA_V17: &str = "
CREATE TABLE designs (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    platform     TEXT NOT NULL DEFAULT 'retrobat',
    spec_json    TEXT,
    draft_json   TEXT,
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at  TEXT
);
CREATE UNIQUE INDEX idx_designs_name ON designs(name COLLATE NOCASE);
CREATE TABLE design_applications (
    id                   INTEGER PRIMARY KEY,
    design_id            TEXT REFERENCES designs(id) ON DELETE SET NULL,
    design_name          TEXT NOT NULL,
    installation_id      TEXT REFERENCES installations(id) ON DELETE SET NULL,
    settings_change_ids  TEXT NOT NULL,
    added_json           TEXT NOT NULL,
    state                TEXT NOT NULL CHECK (state IN ('applied', 'partial', 'undone')),
    created_at           TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_design_applications_installation ON design_applications(installation_id);
";

/// v18 (Sync, 2026-11-07, ADR-043): the platforms left out of Sync for an installation
/// (`NULL`: none), and games the user removed from the drive and chose to keep off it.
const SCHEMA_V18: &str = "
ALTER TABLE installations ADD COLUMN sync_skip_json TEXT;
CREATE TABLE sync_kept_off (
    installation_id  TEXT NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    destination_key  TEXT NOT NULL,
    title            TEXT,
    decided_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (installation_id, destination_key)
);
";

/// v19 (ADR-044): the user's saved decisions for games that need one (which version
/// to copy, copy both), so a later Sync or plan doesn't ask again. JSON map; NULL = none.
const SCHEMA_V19: &str = "
ALTER TABLE installations ADD COLUMN choices_json TEXT;
";

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(stable: &str, label: &str) -> VolumeProbe {
        VolumeProbe {
            kind: "removable",
            identity: Some(VolumeIdentity {
                stable_id: stable.into(),
                label: Some(label.into()),
                filesystem: Some("exFAT".into()),
                serial: Some("ABCD-1234".into()),
                guid: None,
                total_bytes: Some(64_000_000_000),
            }),
        }
    }

    const V0_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS game_files (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        platform TEXT NOT NULL,
        file_size INTEGER NOT NULL DEFAULT 0,
        last_seen TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    ); CREATE INDEX IF NOT EXISTS idx_game_files_platform ON game_files(platform);";

    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[cfg(windows)]
    const SAMPLE_PATHS: [&str; 3] = [r"\\?\C:\Roms\snes\Super Game.sfc", r"D:\Roms\nes\Ünïcode Quest.nes", r"\\?\UNC\nas\games\gba\Pocket.gba"];
    #[cfg(not(windows))]
    const SAMPLE_PATHS: [&str; 3] = ["/roms/snes/Super Game.sfc", "/media/usb/nes/Ünïcode Quest.nes", "/mnt/nas/gba/Pocket.gba"];

    #[test]
    fn fresh_database_gets_current_schema() {
        let mut c = Connection::open_in_memory().unwrap();
        let report = migrate(&mut c, None).unwrap();
        assert_eq!(report.from_version, 0);
        assert_eq!(report.to_version, SCHEMA_VERSION);
        assert_eq!(user_version(&c).unwrap(), SCHEMA_VERSION);
        for table in ["storage_locations", "indexed_roots", "games", "game_releases", "game_files", "file_locations", "jobs", "job_events", "games_fts", "installations"] {
            assert!(table_exists(&c, table).unwrap(), "{table} missing");
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        let second = migrate(&mut c, None).unwrap();
        assert_eq!(second.from_version, SCHEMA_VERSION);
        assert_eq!(second.legacy_rows_migrated, 0);
    }

    #[test]
    fn newer_schema_is_refused_untouched() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA user_version = 99; CREATE TABLE marker(x);").unwrap();
        let err = migrate(&mut c, None).unwrap_err();
        assert!(err.contains("newer"));
        assert!(table_exists(&c, "marker").unwrap());
        assert_eq!(user_version(&c).unwrap(), 99);
    }

    #[test]
    fn merging_releases_moves_files_and_removes_empty_games() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        let cue = insert_game_file(&c, "Game (Disc 1)", "PlayStation", "Game (Disc 1).cue", "cue", 100, None, "folder").unwrap();
        let m3u = insert_game_file(&c, "Game", "PlayStation", "Game.m3u", "m3u", 50, None, "folder").unwrap();
        let release = |file: &str| -> String { c.query_row("SELECT release_id FROM game_files WHERE id = ?1", [file], |r| r.get(0)).unwrap() };
        let (from, into) = (release(&cue), release(&m3u));
        let track = insert_member_file(&c, &from, &cue, "track", None, "Game (Disc 1).bin", "bin", 9000, None, "PlayStation").unwrap();
        merge_release(&c, &from, &into).unwrap();
        assert_eq!(release(&cue), into);
        assert_eq!(release(&track), into, "the cue's tracks move with it");
        assert_eq!(count(&c, "games"), 1, "the disc's own game record is removed");
        assert_eq!(count(&c, "game_releases"), 1);
        let fts: i64 = c.query_row("SELECT COUNT(*) FROM games_fts", [], |r| r.get(0)).unwrap();
        assert_eq!(fts, 1, "search index follows the delete");
        merge_release(&c, &into, &into).unwrap();
    }

    #[test]
    fn title_search_index_follows_games() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        insert_game_file(&c, "Super Puzzle Quest", "SNES", "a.sfc", "sfc", 1, None, "folder").unwrap();
        let hits = |q: &str| -> i64 {
            c.query_row("SELECT COUNT(*) FROM games_fts WHERE games_fts MATCH ?1", [q], |r| r.get(0)).unwrap()
        };
        assert_eq!(hits("\"puzzle\""), 1, "trigram matches inside words, case-insensitively");
        assert_eq!(hits("\"zzle q\""), 1);
        c.execute("UPDATE games SET canonical_title = 'Renamed Title'", []).unwrap();
        assert_eq!(hits("\"puzzle\""), 0);
        assert_eq!(hits("\"renamed\""), 1);
    }

    #[test]
    fn v3_catalog_gets_search_index_for_existing_games() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA_V1).unwrap();
        c.execute_batch(SCHEMA_V2).unwrap();
        c.execute_batch(SCHEMA_V3).unwrap();
        c.execute_batch("PRAGMA user_version = 3;").unwrap();
        insert_game_file_v1(&c, "Existing Adventure", "NES", "e.nes", "nes", 1, "folder").unwrap();
        let report = migrate(&mut c, None).unwrap();
        assert_eq!(report.to_version, SCHEMA_VERSION);
        let n: i64 = c.query_row("SELECT COUNT(*) FROM games_fts WHERE games_fts MATCH '\"adventure\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "existing titles are back-filled");
    }

    #[test]
    fn same_volume_under_a_new_letter_is_recognized() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        let usb = identity("vol:ABCD-1234:64000000000", "GAMES");
        let first = resolve_storage_location(&c, r"E:\", &usb).unwrap();
        assert_eq!(first.previous_root, None);
        let again = resolve_storage_location(&c, r"F:\", &usb).unwrap();
        assert_eq!(again.id, first.id, "same fingerprint → same location");
        assert_eq!(again.previous_root.as_deref(), Some(r"E:\"));
        let (root, name): (String, String) =
            c.query_row("SELECT root_path, display_name FROM storage_locations", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(root, r"F:\");
        assert_eq!(name, r"GAMES (F:\)");
        // Rescanning at the same letter is not reported as a move.
        assert_eq!(resolve_storage_location(&c, r"F:\", &usb).unwrap().previous_root, None);
    }

    #[test]
    fn different_volume_at_a_known_letter_gets_its_own_location() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();
        let a = resolve_storage_location(&c, r"E:\", &identity("vol:AAAA-AAAA:1", "A")).unwrap();
        let b = resolve_storage_location(&c, r"E:\", &identity("vol:BBBB-BBBB:2", "B")).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(count(&c, "storage_locations"), 2);
    }

    #[test]
    fn unidentified_location_is_claimed_only_when_its_files_are_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Game.sfc"), b"x").unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, None).unwrap();

        // Legacy row whose file exists under `root` → claimed.
        let legacy = upsert_storage_location(&c, &root, "unknown").unwrap();
        let file = insert_game_file(&c, "Game", "SNES", "Game.sfc", "sfc", 1, None, "legacy").unwrap();
        c.execute(
            "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path) VALUES (?1, ?2, ?3, 'Game.sfc')",
            params![new_id(), file, legacy],
        )
        .unwrap();
        let resolved = resolve_storage_location(&c, &root, &identity("vol:1111-1111:1", "X")).unwrap();
        assert_eq!(resolved.id, legacy);

        // Another legacy row at the same root whose files are absent → not claimed.
        let mut c2 = Connection::open_in_memory().unwrap();
        migrate(&mut c2, None).unwrap();
        let other = upsert_storage_location(&c2, &root, "unknown").unwrap();
        let file = insert_game_file(&c2, "Gone", "SNES", "Gone.sfc", "sfc", 1, None, "legacy").unwrap();
        c2.execute(
            "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path) VALUES (?1, ?2, ?3, 'Gone.sfc')",
            params![new_id(), file, other],
        )
        .unwrap();
        let resolved = resolve_storage_location(&c2, &root, &identity("vol:2222-2222:1", "Y")).unwrap();
        assert_ne!(resolved.id, other);
    }

    #[test]
    fn v2_catalog_upgrades_to_v3_keeping_references() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("retrodrive.sqlite");
        {
            let c = Connection::open(&db_path).unwrap();
            c.execute_batch(SCHEMA_V1).unwrap();
            c.execute_batch(SCHEMA_V2).unwrap();
            c.execute_batch("PRAGMA user_version = 2;").unwrap();
            c.execute("INSERT INTO storage_locations (id, display_name, root_path) VALUES ('s1', 'E:', 'E:/')", []).unwrap();
            let file = insert_game_file_v1(&c, "Kept", "NES", "Kept.nes", "nes", 1, "folder").unwrap();
            c.execute(
                "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path) VALUES ('l1', ?1, 's1', 'Kept.nes')",
                [file],
            )
            .unwrap();
            c.execute("INSERT INTO indexed_roots (id, storage_location_id, relative_path) VALUES ('r1', 's1', 'roms')", []).unwrap();
        }
        let mut c = open(&db_path).unwrap();
        let report = migrate(&mut c, Some(&db_path)).unwrap();
        assert_eq!((report.from_version, report.to_version), (2, SCHEMA_VERSION));
        assert!(report.backup_path.is_some());
        assert_eq!(count(&c, "storage_locations"), 1);
        assert_eq!(count(&c, "file_locations"), 1);
        assert_eq!(count(&c, "indexed_roots"), 1);
        // Foreign keys are enforced again and still point at the rebuilt table.
        let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
        assert!(c.execute("DELETE FROM storage_locations WHERE id = 's1'", []).is_err(), "RESTRICT still protects files");
        // root_path is no longer unique.
        c.execute("INSERT INTO storage_locations (id, display_name, root_path) VALUES ('s2', 'E:', 'E:/')", []).unwrap();
    }

    #[test]
    fn v1_catalog_upgrades_to_v2_keeping_data() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("retrodrive.sqlite");
        {
            let c = Connection::open(&db_path).unwrap();
            c.execute_batch(SCHEMA_V1).unwrap();
            c.execute_batch("PRAGMA user_version = 1;").unwrap();
            let storage = upsert_storage_location(&c, SAMPLE_PATHS[1], "local").unwrap();
            let file = insert_game_file_v1(&c, "Kept", "NES", "Kept.nes", "nes", 1, "folder").unwrap();
            c.execute(
                "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path) VALUES (?1, ?2, ?3, 'Kept.nes')",
                params![new_id(), file, storage],
            )
            .unwrap();
        }
        let mut c = open(&db_path).unwrap();
        let report = migrate(&mut c, Some(&db_path)).unwrap();
        assert_eq!((report.from_version, report.to_version), (1, SCHEMA_VERSION));
        assert!(report.backup_path.is_some());
        assert_eq!(count(&c, "games"), 1);
        assert_eq!(count(&c, "file_locations"), 1);
        assert!(table_exists(&c, "jobs").unwrap());
        assert!(table_exists(&c, "job_events").unwrap());
    }

    #[test]
    fn v0_rows_are_preserved_and_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("retrodrive.sqlite");
        {
            let c = Connection::open(&db_path).unwrap();
            c.execute_batch(V0_SCHEMA).unwrap();
            for (i, p) in SAMPLE_PATHS.iter().enumerate() {
                c.execute(
                    "INSERT INTO game_files (path,title,platform,file_size) VALUES (?1,?2,?3,?4)",
                    params![p, format!("Title {i}"), "SNES", 1000 + i as i64],
                )
                .unwrap();
            }
        }
        let mut c = open(&db_path).unwrap();
        let report = migrate(&mut c, Some(&db_path)).unwrap();
        assert_eq!(report.legacy_rows_migrated, 3);
        assert_eq!(count(&c, "games"), 3);
        assert_eq!(count(&c, "game_files"), 3);
        assert_eq!(count(&c, "file_locations"), 3);
        assert!(!table_exists(&c, "legacy_v0_game_files").unwrap());

        // Backup holds the untouched v0 data.
        let backup = report.backup_path.expect("backup created");
        let b = Connection::open(&backup).unwrap();
        assert_eq!(count(&b, "game_files"), 3);
        assert_eq!(user_version(&b).unwrap(), 0);

        // Paths are reconstructable and verbatim prefixes were removed.
        let mut s = c
            .prepare("SELECT s.root_path, fl.relative_path FROM file_locations fl JOIN storage_locations s ON s.id = fl.storage_location_id")
            .unwrap();
        let rebuilt: Vec<String> = s
            .query_map([], |r| Ok(paths::join(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?)))
            .unwrap()
            .map(|p| p.unwrap().to_string_lossy().into_owned())
            .collect();
        for p in SAMPLE_PATHS {
            let expected = paths::strip_verbatim(p);
            assert!(rebuilt.contains(&expected), "{expected} not in {rebuilt:?}");
        }
        assert!(rebuilt.iter().all(|p| !p.starts_with(r"\\?\")));
    }
}
