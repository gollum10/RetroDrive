//! Leaving files out of the library, and cleaning up duplicates (step 5a; docs/07).
//!
//! "Left out" is a mark in the catalog only: the file stays on the user's drive,
//! untouched, but RetroDrive no longer lists it, counts it, or plans to copy it.
//! It can be brought back at any time. RetroDrive never deletes files.
//!
//! Duplicate cleanup keeps one copy of each group of identical files (same system
//! hint, file name ignoring case, and size, the same key as the duplicate badge) and
//! leaves the others out. The copy kept is the first of: present and on a connected
//! drive, then the shortest path, then alphabetical; the choice is deterministic so
//! the preview and the cleanup agree.

use crate::volumes::Presence;
use crate::paths;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct CleanupExample {
    pub title: String,
    pub keep: String,
    pub leave_out: Vec<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq, Default)]
pub struct CleanupPreview {
    /// Groups of identical files with more than one copy in the library.
    pub groups: i64,
    pub files_to_leave_out: i64,
    pub bytes_to_leave_out: i64,
    /// The first groups, to show what would happen.
    pub examples: Vec<CleanupExample>,
}

struct Candidate {
    file_id: String,
    title: String,
    path: String,
    size: i64,
    usable: bool,
}

/// Groups with their copies ordered best-first (the first one is kept).
fn groups(connection: &Connection, presence: PresenceFn) -> Result<Vec<Vec<Candidate>>, String> {
    let mut statement = connection
        .prepare(
            "SELECT gf.id, g.canonical_title, s.root_path, s.stable_id, fl.relative_path, gf.size_bytes, fl.present_status,
                    lower(gf.filename), gf.platform_hint
             FROM game_files gf
             JOIN game_releases r ON r.id = gf.release_id JOIN games g ON g.id = r.game_id
             JOIN file_locations fl ON fl.game_file_id = gf.id JOIN storage_locations s ON s.id = fl.storage_location_id
             JOIN (SELECT size_bytes AS k_size, lower(filename) AS k_name, platform_hint AS k_hint FROM game_files
                   WHERE size_bytes > 0 AND file_role = 'primary' AND excluded_at IS NULL
                   GROUP BY size_bytes, filename COLLATE NOCASE, platform_hint HAVING COUNT(*) > 1) k
               ON k.k_size = gf.size_bytes AND k.k_name = lower(gf.filename) AND k.k_hint IS gf.platform_hint
             WHERE gf.file_role = 'primary' AND gf.excluded_at IS NULL",
        )
        .map_err(|e| e.to_string())?;
    let mut connected: HashMap<String, bool> = HashMap::new();
    let mut by_key: HashMap<(i64, String, Option<String>), Vec<Candidate>> = HashMap::new();
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (file_id, title, root, stable, relative, size, status, name, hint) = row.map_err(|e| e.to_string())?;
        let online = *connected.entry(root.clone()).or_insert_with(|| presence(&root, stable.as_deref()) == Presence::Connected);
        let path = paths::join(&root, &relative).display().to_string();
        by_key.entry((size, name, hint)).or_default().push(Candidate { file_id, title, path, size, usable: online && status == "present" });
    }
    let mut groups: Vec<Vec<Candidate>> = by_key
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort_by(|a, b| b.usable.cmp(&a.usable).then(a.path.len().cmp(&b.path.len())).then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase())));
            g
        })
        .collect();
    groups.sort_by(|a, b| a[0].title.to_lowercase().cmp(&b[0].title.to_lowercase()).then_with(|| a[0].path.cmp(&b[0].path)));
    Ok(groups)
}

/// What a cleanup would do. Reads only.
pub fn preview(connection: &Connection, presence: PresenceFn) -> Result<CleanupPreview, String> {
    let groups = groups(connection, presence)?;
    let mut result = CleanupPreview { groups: groups.len() as i64, ..CleanupPreview::default() };
    for group in &groups {
        result.files_to_leave_out += (group.len() - 1) as i64;
        result.bytes_to_leave_out += group[1..].iter().map(|c| c.size).sum::<i64>();
        if result.examples.len() < 20 {
            result.examples.push(CleanupExample {
                title: group[0].title.clone(),
                keep: group[0].path.clone(),
                leave_out: group[1..].iter().map(|c| c.path.clone()).collect(),
            });
        }
    }
    Ok(result)
}

/// Keep one copy of each duplicate group and leave the others out. Returns how many
/// files were left out. Catalog only: no file is touched.
pub fn clean_up(connection: &Connection, presence: PresenceFn) -> Result<i64, String> {
    let groups = groups(connection, presence)?;
    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut changed = 0i64;
    {
        let mut mark = tx
            .prepare("UPDATE game_files SET excluded_at = CURRENT_TIMESTAMP, excluded_reason = 'duplicate' WHERE id = ?1 AND excluded_at IS NULL")
            .map_err(|e| e.to_string())?;
        for group in &groups {
            for copy in &group[1..] {
                changed += mark.execute([&copy.file_id]).map_err(|e| e.to_string())? as i64;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

/// Leave files out of the library, or bring them back. Returns how many changed.
pub fn set_left_out(connection: &Connection, file_ids: &[String], left_out: bool) -> Result<i64, String> {
    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut changed = 0i64;
    {
        let mut statement = tx
            .prepare(if left_out {
                "UPDATE game_files SET excluded_at = CURRENT_TIMESTAMP, excluded_reason = 'user' WHERE id = ?1 AND file_role = 'primary' AND excluded_at IS NULL"
            } else {
                "UPDATE game_files SET excluded_at = NULL, excluded_reason = NULL WHERE id = ?1 AND excluded_at IS NOT NULL"
            })
            .map_err(|e| e.to_string())?;
        for id in file_ids {
            changed += statement.execute(params![id]).map_err(|e| e.to_string())? as i64;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{self, GameQuery};
    use crate::volumes::VolumeProbe;
    use crate::{db, scanner};
    use std::fs;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn duplicates_are_left_out_without_touching_files_and_can_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        let put = |rel: &str, body: &[u8]| {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        };
        put("nes/Kid.nes", b"same rom");
        put("more/deeper/nes/Kid.nes", b"same rom");
        put("nes/Clash.nes", b"one");
        put("nesh/Clash.nes", b"different!");
        put("snes/Solo.sfc", b"solo");
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        let before: Vec<_> = walkdir::WalkDir::new(&root).sort_by_file_name().into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();

        let preview = preview(&c, &connected).unwrap();
        assert_eq!((preview.groups, preview.files_to_leave_out), (1, 1), "different files with one name are not duplicates");
        let kid = &preview.examples[0];
        assert!(kid.keep.replace('\\', "/").ends_with("roms/nes/Kid.nes"), "the shorter path is kept: {}", kid.keep);
        assert!(kid.leave_out[0].replace('\\', "/").ends_with("more/deeper/nes/Kid.nes"));

        assert_eq!(clean_up(&c, &connected).unwrap(), 1);
        let stats = catalog::library_stats(&c, &connected).unwrap();
        assert_eq!(stats.files, 4, "left-out files aren't counted");
        assert_eq!(stats.left_out, 1);
        assert_eq!(stats.duplicate_files, 0, "no duplicates remain in the library");
        let listed = catalog::query_games(&c, &GameQuery::default(), &connected).unwrap();
        assert_eq!(listed.total, 4);
        let hidden = catalog::query_games(&c, &GameQuery { left_out: true, ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(hidden.rows.len(), 1);
        assert!(hidden.rows[0].path.replace('\\', "/").ends_with("deeper/nes/Kid.nes"));
        let systems: i64 = catalog::system_summary(&c).unwrap().iter().map(|s| s.games).sum();
        assert_eq!(systems, 4);
        let after: Vec<_> = walkdir::WalkDir::new(&root).sort_by_file_name().into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();
        assert_eq!(before, after, "leaving files out never touches them");

        // Rescans keep the mark; bringing it back restores it.
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        assert_eq!(catalog::library_stats(&c, &connected).unwrap().left_out, 1);
        assert_eq!(set_left_out(&c, &[hidden.rows[0].file_id.clone()], false).unwrap(), 1);
        assert_eq!(catalog::library_stats(&c, &connected).unwrap().files, 5);

        // A clash can be resolved by leaving one version out.
        let clash = catalog::query_games(&c, &GameQuery { text: Some("Clash".into()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(clash.rows.len(), 2);
        assert_eq!(set_left_out(&c, &[clash.rows[0].file_id.clone()], true).unwrap(), 1);
        assert_eq!(set_left_out(&c, &[clash.rows[0].file_id.clone()], true).unwrap(), 0, "already left out");
    }

    #[test]
    fn a_missing_copy_is_never_the_one_kept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        for rel in ["nes/A.nes", "zz/longer/path/nes/A.nes"] {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"rom").unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        c.execute("UPDATE file_locations SET present_status = 'missing' WHERE relative_path LIKE '%/nes/A.nes' AND relative_path NOT LIKE '%longer%'", []).unwrap();
        let p = preview(&c, &connected).unwrap();
        assert!(p.examples[0].keep.contains("longer"), "the copy that is actually there is kept: {}", p.examples[0].keep);
    }
}
