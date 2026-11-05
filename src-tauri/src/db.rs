//! SQLite catalog: connection setup and versioned schema migrations (RDS-001).
//!
//! Schema versions are tracked with `PRAGMA user_version`.
//! - v0: the v0.1 starter's single file-centric `game_files` table (no version set).
//! - v1: normalized hybrid library (storage locations, indexed roots, games,
//!   releases, files and file locations) as described in docs/05-DATA-MODEL.md.
//!
//! Migration rules: back up an existing database before upgrading, run the upgrade
//! in one transaction, verify row counts, and refuse to open a database written by
//! a newer schema than this build understands.

use crate::paths;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const SCHEMA_VERSION: i32 = 1;

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
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    report.to_version = SCHEMA_VERSION;
    Ok(report)
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

/// Find a storage location by its root path, creating it when absent.
pub fn upsert_storage_location(tx: &Connection, root_path: &str, kind: &str) -> Result<String, String> {
    if let Some(id) = tx
        .query_row("SELECT id FROM storage_locations WHERE root_path = ?1", [root_path], |r| r.get::<_, String>(0))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for table in ["storage_locations", "indexed_roots", "games", "game_releases", "game_files", "file_locations"] {
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
