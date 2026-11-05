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
//!
//! Migration rules: back up an existing database before upgrading, run the upgrade
//! in one transaction, verify row counts, and refuse to open a database written by
//! a newer schema than this build understands.

use crate::paths;
use crate::volumes::{VolumeIdentity, VolumeProbe};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const SCHEMA_VERSION: i32 = 4;

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
            "The catalog was written by a newer RetroDrive Studio (schema v{from_version}; this build supports v{SCHEMA_VERSION}). \
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
        let file_id = insert_game_file(tx, &title, &platform, &filename, &extension, size, None, "legacy")?;
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
        "INSERT INTO game_files (id, release_id, filename, extension, size_bytes, modified_at, platform_hint, platform_hint_source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![file_id, release_id, filename, extension, size, modified_at, platform, hint_source],
    )
    .map_err(|e| e.to_string())?;
    Ok(file_id)
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
        for table in ["storage_locations", "indexed_roots", "games", "game_releases", "game_files", "file_locations", "jobs", "job_events", "games_fts"] {
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
        insert_game_file(&c, "Existing Adventure", "NES", "e.nes", "nes", 1, None, "folder").unwrap();
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
            let file = insert_game_file(&c, "Kept", "NES", "Kept.nes", "nes", 1, None, "folder").unwrap();
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
            let file = insert_game_file(&c, "Kept", "NES", "Kept.nes", "nes", 1, None, "folder").unwrap();
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
