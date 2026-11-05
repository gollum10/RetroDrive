//! Catalog queries for the UI.

use crate::paths;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

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
    /// `offline`: the volume (drive / share) is not currently reachable.
    /// `missing`: a complete scan of its folder did not find the file.
    pub availability: &'static str,
}

/// List the catalog. Availability is derived from the last scan result plus one
/// reachability check per volume, so listing never touches individual game files
/// (which would stall on large or disconnected libraries).
pub fn list_games(connection: &Connection, volume_reachable: &dyn Fn(&str) -> bool) -> Result<Vec<GameRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT fl.id, g.id, g.canonical_title, g.platform, s.id, s.root_path, fl.relative_path, gf.size_bytes, fl.present_status
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
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id, game_id, title, platform, storage_id, root, relative, file_size, status) = row.map_err(|e| e.to_string())?;
        let volume_ok = *reachable.entry(storage_id).or_insert_with(|| volume_reachable(&root));
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

pub fn volume_reachable(root: &str) -> bool {
    Path::new(root).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, scanner};
    use std::fs;

    #[test]
    fn availability_states() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snes")).unwrap();
        fs::write(dir.path().join("snes/A.sfc"), b"a").unwrap();
        fs::write(dir.path().join("snes/B.sfc"), b"b").unwrap();
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| "local").unwrap();
        fs::remove_file(dir.path().join("snes/B.sfc")).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| "local").unwrap();

        let rows = list_games(&c, &|_| true).unwrap();
        let by_title: HashMap<_, _> = rows.iter().map(|r| (r.title.as_str(), r.availability)).collect();
        assert_eq!(by_title["A"], "online");
        assert_eq!(by_title["B"], "missing");

        // Unplugged drive: every row stays in the catalog, shown as offline.
        let rows = list_games(&c, &|_| false).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.availability == "offline"));
        assert!(rows.iter().all(|r| Path::new(&r.path).is_absolute()));
    }
}
