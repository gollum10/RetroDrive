//! Vertical games (owner request 2026-11-07: "We should have a setting for Vertical
//! games"; ADR-069). Arcade games made for a portrait (vertical) screen, from
//! Batocera EmulationStation's `resources/arcaderoms.xml` (MIT; the `vert="true"`
//! entries, `resources/vertical_games.tsv`). Matched the way EmulationStation does
//! (`MameNames::isVertical`): an arcade game whose romset name is in the list. This is
//! the same list RetroBat's and Batocera's "vertical arcade" collection uses.

use crate::lightguns::{library_matches, Cache, ARCADE_FOLDERS};
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

const VERTICAL_GAMES: &str = include_str!("../resources/vertical_games.tsv");

fn list() -> &'static HashSet<String> {
    static LIST: OnceLock<HashSet<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        VERTICAL_GAMES
            .lines()
            .filter(|l| !l.starts_with('#'))
            .skip(1)
            .filter_map(|l| l.split('\t').next())
            .map(str::to_lowercase)
            .collect()
    })
}

/// Number of vertical arcade games in the list.
#[cfg(test)]
pub fn count() -> usize {
    list().len()
}

/// Whether a game file is a vertical arcade game (by its romset name).
pub fn is_vertical(folder: &str, stem: &str) -> bool {
    ARCADE_FOLDERS.contains(&folder) && list().contains(&stem.to_lowercase())
}

/// Ids of the library's vertical games (cached until the library changes).
pub fn game_ids(connection: &Connection) -> Result<Arc<HashSet<String>>, String> {
    static CACHE: Mutex<Cache> = Mutex::new(None);
    library_matches(connection, &CACHE, is_vertical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_arcade_games_are_found_by_romset_name() {
        assert!(count() > 3000, "{}", count());
        assert!(is_vertical("mame", "1941") && is_vertical("fbneo", "1941J"));
        assert!(
            !is_vertical("mame", "sf2"),
            "Street Fighter II is horizontal"
        );
        assert!(!is_vertical("nes", "1941"), "only arcade sets");
        // A scanned library: the vertical arcade set is found, the horizontal one isn't.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mame")).unwrap();
        for name in ["1941.zip", "sf2.zip"] {
            std::fs::write(dir.path().join("mame").join(name), vec![0u8; 64]).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        crate::scanner::scan(&mut c, dir.path(), &|_: &str| {
            crate::volumes::VolumeProbe::unknown()
        })
        .unwrap();
        let ids = game_ids(&c).unwrap();
        let files: Vec<String> = ids
            .iter()
            .map(|id| c.query_row("SELECT gf.filename FROM games g JOIN game_releases r ON r.game_id = g.id JOIN game_files gf ON gf.release_id = r.id WHERE g.id = ?1", [id], |r| r.get(0)).unwrap())
            .collect();
        assert_eq!(files, vec!["1941.zip".to_string()], "{files:?}");
    }
}
