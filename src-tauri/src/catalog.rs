//! Catalog queries for the UI.

use crate::paths;
use crate::volumes::Presence;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

/// One row per physical file location, joined back to its game.
#[derive(Debug, Serialize)]
pub struct GameRow {
    pub id: String,
    pub game_id: String,
    pub title: String,
    pub platform: String,
    pub path: String,
    pub file_size: i64,
    /// `online`: file was present at the last scan and its volume is reachable.
    /// `offline`: the volume (drive / share) is not attached, or its drive letter
    /// now belongs to a different volume.
    /// `missing`: a complete scan of its folder did not find the file.
    pub availability: &'static str,
}

/// List the catalog. Availability is derived from the last scan result plus one
/// presence check per volume, so listing never touches individual game files
/// (which would stall on large or disconnected libraries).
pub fn list_games(connection: &Connection, presence: &dyn Fn(&str, Option<&str>) -> Presence) -> Result<Vec<GameRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT fl.id, g.id, g.canonical_title, g.platform, s.id, s.root_path, fl.relative_path, gf.size_bytes, fl.present_status, s.stable_id
             FROM file_locations fl
             JOIN game_files gf ON gf.id = fl.game_file_id
             JOIN game_releases r ON r.id = gf.release_id
             JOIN games g ON g.id = r.game_id
             JOIN storage_locations s ON s.id = fl.storage_location_id
             ORDER BY g.canonical_title COLLATE NOCASE, fl.relative_path",
        )
        .map_err(|e| e.to_string())?;
    let mut reachable: HashMap<String, bool> = HashMap::new();
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, Option<String>>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id, game_id, title, platform, storage_id, root, relative, file_size, status, stable_id) = row.map_err(|e| e.to_string())?;
        let volume_ok = *reachable.entry(storage_id).or_insert_with(|| presence(&root, stable_id.as_deref()) == Presence::Connected);
        let availability = if !volume_ok {
            "offline"
        } else if status == "missing" {
            "missing"
        } else {
            "online"
        };
        let path = paths::join(&root, &relative).display().to_string();
        Ok(GameRow { id, game_id, title, platform, path, file_size, availability })
    })
    .collect()
}

/// A drive the catalog knows about, connected or not (Drive Manager "Known drives").
#[derive(Debug, Serialize)]
pub struct StorageRow {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub kind: String,
    pub stable_id: Option<String>,
    pub volume_label: Option<String>,
    pub filesystem: Option<String>,
    pub volume_serial: Option<String>,
    pub total_bytes: Option<i64>,
    pub last_seen_at: Option<String>,
    pub file_count: i64,
    pub missing_count: i64,
    pub presence: Presence,
}

pub fn list_storage_locations(connection: &Connection, presence: &dyn Fn(&str, Option<&str>) -> Presence) -> Result<Vec<StorageRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT s.id, s.display_name, s.root_path, s.kind, s.stable_id, s.volume_label, s.filesystem, s.volume_serial, s.total_bytes, s.last_seen_at,
                    COUNT(fl.id), COALESCE(SUM(fl.present_status = 'missing'), 0)
             FROM storage_locations s LEFT JOIN file_locations fl ON fl.storage_location_id = s.id
             GROUP BY s.id ORDER BY s.last_seen_at DESC, s.display_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok(StorageRow {
                id: r.get(0)?,
                display_name: r.get(1)?,
                root_path: r.get(2)?,
                kind: r.get(3)?,
                stable_id: r.get(4)?,
                volume_label: r.get(5)?,
                filesystem: r.get(6)?,
                volume_serial: r.get(7)?,
                total_bytes: r.get(8)?,
                last_seen_at: r.get(9)?,
                file_count: r.get(10)?,
                missing_count: r.get(11)?,
                presence: Presence::NotConnected,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let mut row = row.map_err(|e| e.to_string())?;
        row.presence = presence(&row.root_path, row.stable_id.as_deref());
        Ok(row)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, scanner};
    use std::fs;
    use std::path::Path;

    #[test]
    fn availability_states() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snes")).unwrap();
        fs::write(dir.path().join("snes/A.sfc"), b"a").unwrap();
        fs::write(dir.path().join("snes/B.sfc"), b"b").unwrap();
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let probe = |_: &str| crate::volumes::VolumeProbe::unknown();
        scanner::scan(&mut c, dir.path(), &probe).unwrap();
        fs::remove_file(dir.path().join("snes/B.sfc")).unwrap();
        scanner::scan(&mut c, dir.path(), &probe).unwrap();

        let rows = list_games(&c, &|_, _| Presence::Connected).unwrap();
        let by_title: HashMap<_, _> = rows.iter().map(|r| (r.title.as_str(), r.availability)).collect();
        assert_eq!(by_title["A"], "online");
        assert_eq!(by_title["B"], "missing");

        // Unplugged drive, or its letter now holds another drive: rows stay, shown offline.
        for state in [Presence::NotConnected, Presence::OtherVolume] {
            let rows = list_games(&c, &|_, _| state).unwrap();
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().all(|r| r.availability == "offline"));
            assert!(rows.iter().all(|r| Path::new(&r.path).is_absolute()));
        }

        let drives = list_storage_locations(&c, &|_, _| Presence::OtherVolume).unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].file_count, 2);
        assert_eq!(drives[0].missing_count, 1);
        assert_eq!(drives[0].presence, Presence::OtherVolume);
    }
}
