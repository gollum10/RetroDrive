//! Catalog tools (docs/10 privacy defaults: "export/delete catalog controls that don't
//! delete ROM files"; owner 2026-11-09; ADR-097; docs/18 §61).
//!
//! - **Export** writes the game list to a new CSV file (Excel opens it) in the user's
//!   Documents folder. Only that new file is written; an existing file is never replaced.
//! - **Remove a library folder** forgets a scanned folder and every catalog entry found
//!   only in it (e.g. a drive you no longer use), so the next scan doesn't bring them back.
//! - **Clean up missing entries** forgets files a complete scan found gone.
//!
//! Removing changes **catalog records only**: no file on any drive is read, moved or
//! deleted. Entries kept by another library folder (folders inside each other) stay.
//! Collections and art picks of removed files go with them; the copy history keeps its
//! records (their link to the library file is cleared). A game still on a drive is
//! hidden with **Leave out** instead, which survives rescans.

use crate::volumes::Presence;
use crate::{copier, db};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LibraryFolder {
    pub id: String,
    pub drive: String,
    pub path: String,
    /// Files found in this folder (with folders inside it that are scanned on their own).
    pub files: i64,
    pub missing: i64,
    pub last_scan: Option<String>,
    pub connected: bool,
}

/// What a removal would forget.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct Removal {
    /// File entries forgotten (game files, discs and tracks).
    pub files: i64,
    /// Games that disappear from the library.
    pub games: i64,
    /// Of those files, how many are in your own collections.
    pub in_collections: i64,
    /// Entries kept because another library folder also holds them.
    pub kept: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Exported {
    pub path: String,
    pub name: String,
    pub rows: usize,
}

/// The scanned library folders, by drive and path (reads only).
pub fn folders(connection: &Connection, presence: PresenceFn) -> Result<Vec<LibraryFolder>, String> {
    let mut statement = connection
        .prepare(
            "SELECT r.id, s.display_name, s.root_path, s.stable_id, r.relative_path, r.last_scan_completed_at,
                    (SELECT COUNT(*) FROM file_locations fl WHERE fl.storage_location_id = r.storage_location_id AND (r.relative_path = '' OR lower(substr(fl.relative_path, 1, length(r.relative_path) + 1)) = lower(r.relative_path || '/'))),
                    (SELECT COUNT(*) FROM file_locations fl WHERE fl.storage_location_id = r.storage_location_id AND fl.present_status = 'missing' AND (r.relative_path = '' OR lower(substr(fl.relative_path, 1, length(r.relative_path) + 1)) = lower(r.relative_path || '/')))
             FROM indexed_roots r JOIN storage_locations s ON s.id = r.storage_location_id
             ORDER BY s.root_path, r.relative_path COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            let root: String = r.get(2)?;
            let stable: Option<String> = r.get(3)?;
            let relative: String = r.get(4)?;
            Ok(LibraryFolder {
                id: r.get(0)?,
                drive: r.get(1)?,
                path: crate::paths::join(&root, &relative).display().to_string(),
                files: r.get(6)?,
                missing: r.get(7)?,
                last_scan: r.get(5)?,
                connected: presence(&root, stable.as_deref()) == Presence::Connected,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

/// The locations a removal would forget, and how many it keeps for other folders.
fn folder_locations(connection: &Connection, root_id: &str) -> Result<(Vec<String>, i64), String> {
    let (storage, relative): (String, String) = connection
        .query_row("SELECT storage_location_id, relative_path FROM indexed_roots WHERE id = ?1", [root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("That library folder isn't in the catalog any more.")?;
    let under = |col: &str, root: &str| format!("({root} = '' OR lower(substr({col}, 1, length({root}) + 1)) = lower({root} || '/'))");
    let sql = format!(
        "SELECT fl.id, EXISTS (SELECT 1 FROM indexed_roots o WHERE o.id <> ?3 AND o.storage_location_id = ?1 AND {other})
         FROM file_locations fl WHERE fl.storage_location_id = ?1 AND {mine}",
        other = under("fl.relative_path", "o.relative_path"),
        mine = under("fl.relative_path", "?2"),
    );
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let mut forget = Vec::new();
    let mut kept = 0;
    for row in statement.query_map(params![storage, relative, root_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))).map_err(|e| e.to_string())? {
        let (id, other) = row.map_err(|e| e.to_string())?;
        if other {
            kept += 1;
        } else {
            forget.push(id);
        }
    }
    Ok((forget, kept))
}

fn missing_locations(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare("SELECT id FROM file_locations WHERE present_status = 'missing'").map_err(|e| e.to_string())?;
    let ids = statement.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    ids.map(|r| r.map_err(|e| e.to_string())).collect()
}

/// Forget these locations and whatever is left without any (inside `tx`), or with
/// `dry_run` only count it. Returns the counts.
fn forget(tx: &Connection, locations: &[String], kept: i64, dry_run: bool) -> Result<Removal, String> {
    let json = serde_json::to_string(locations).map_err(|e| e.to_string())?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS temp.forget_locations; DROP TABLE IF EXISTS temp.forget_files; DROP TABLE IF EXISTS temp.forget_games;
         CREATE TEMP TABLE forget_locations (id TEXT PRIMARY KEY);
         CREATE TEMP TABLE forget_files (id TEXT PRIMARY KEY);
         CREATE TEMP TABLE forget_games (id TEXT PRIMARY KEY);",
    )
    .map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO temp.forget_locations SELECT value FROM json_each(?1)", [&json]).map_err(|e| e.to_string())?;
    // Files whose every location is forgotten, then (repeatedly) parts and parents
    // that would be left with nothing: a playlist goes when all its discs go and so on.
    tx.execute(
        "INSERT OR IGNORE INTO temp.forget_files
         SELECT DISTINCT fl.game_file_id FROM file_locations fl JOIN temp.forget_locations x ON x.id = fl.id
         WHERE NOT EXISTS (SELECT 1 FROM file_locations o WHERE o.game_file_id = fl.game_file_id AND o.id NOT IN (SELECT id FROM temp.forget_locations))",
        [],
    )
    .map_err(|e| e.to_string())?;
    // A file can't go while a part that stays points at it.
    loop {
        let n = tx
            .execute(
                "DELETE FROM temp.forget_files WHERE id IN (
                     SELECT f.id FROM temp.forget_files f JOIN game_files c ON c.parent_file_id = f.id
                     WHERE c.id NOT IN (SELECT id FROM temp.forget_files))",
                [],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
    }
    tx.execute(
        "INSERT INTO temp.forget_games
         SELECT DISTINCT r.game_id FROM game_releases r WHERE r.id IN (SELECT gf.release_id FROM game_files gf JOIN temp.forget_files f ON f.id = gf.id)
           AND NOT EXISTS (SELECT 1 FROM game_files o JOIN game_releases orr ON orr.id = o.release_id
                           WHERE orr.game_id = r.game_id AND o.id NOT IN (SELECT id FROM temp.forget_files))",
        [],
    )
    .map_err(|e| e.to_string())?;
    let count = |sql: &str| tx.query_row(sql, [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string());
    let removal = Removal {
        files: count("SELECT COUNT(*) FROM temp.forget_files")?,
        games: count("SELECT COUNT(*) FROM temp.forget_games")?,
        in_collections: count("SELECT COUNT(DISTINCT game_file_id) FROM collection_games WHERE game_file_id IN (SELECT id FROM temp.forget_files)")?,
        kept,
    };
    if !dry_run {
        // The search index's delete trigger looks each game up by an unindexed column,
        // one full pass per game (hours for a 40,000-game drive). Inside this
        // transaction it's set aside, the search rows go in one pass, and it's put back
        // exactly as it was.
        let trigger: Option<String> = tx
            .query_row("SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'games_fts_delete'", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if trigger.is_some() {
            tx.execute_batch("DROP TRIGGER games_fts_delete;").map_err(|e| e.to_string())?;
        }
        tx.execute_batch(
            "DELETE FROM file_locations WHERE id IN (SELECT id FROM temp.forget_locations);
             CREATE TEMP TABLE forget_releases AS SELECT DISTINCT release_id AS id FROM game_files WHERE id IN (SELECT id FROM temp.forget_files) AND release_id IS NOT NULL;
             DELETE FROM game_media WHERE game_file_id IN (SELECT id FROM temp.forget_files);
             UPDATE game_files SET parent_file_id = NULL WHERE id IN (SELECT id FROM temp.forget_files);
             DELETE FROM game_files WHERE id IN (SELECT id FROM temp.forget_files);
             DELETE FROM game_releases WHERE id IN (SELECT id FROM temp.forget_releases)
                 AND NOT EXISTS (SELECT 1 FROM game_files gf WHERE gf.release_id = game_releases.id);
             DROP TABLE temp.forget_releases;
             DELETE FROM games WHERE id IN (SELECT id FROM temp.forget_games)
                 AND NOT EXISTS (SELECT 1 FROM game_releases r WHERE r.game_id = games.id);
             DELETE FROM games_fts WHERE game_id IN (SELECT id FROM temp.forget_games) AND game_id NOT IN (SELECT id FROM games);",
        )
        .map_err(|e| e.to_string())?;
        if let Some(sql) = trigger {
            tx.execute_batch(&sql).map_err(|e| e.to_string())?;
        }
    }
    tx.execute_batch("DROP TABLE temp.forget_locations; DROP TABLE temp.forget_files; DROP TABLE temp.forget_games;").map_err(|e| e.to_string())?;
    Ok(removal)
}

/// What removing this library folder would forget (reads only).
pub fn folder_preview(connection: &mut Connection, root_id: &str) -> Result<Removal, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let (locations, kept) = folder_locations(&tx, root_id)?;
    let r = forget(&tx, &locations, kept, true)?;
    tx.rollback().map_err(|e| e.to_string())?;
    Ok(r)
}

/// Remove a library folder from the catalog. `expected` is the preview the user saw;
/// if anything changed since, nothing is removed.
pub fn remove_folder(connection: &mut Connection, root_id: &str, expected: &Removal) -> Result<Removal, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let (locations, kept) = folder_locations(&tx, root_id)?;
    let r = forget(&tx, &locations, kept, false)?;
    if &r != expected {
        return Err("The catalog changed since you looked (maybe a scan finished), so nothing was removed. Look again.".into());
    }
    tx.execute("DELETE FROM indexed_roots WHERE id = ?1", [root_id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(r)
}

/// What cleaning up missing entries would forget (reads only).
pub fn missing_preview(connection: &mut Connection) -> Result<Removal, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let locations = missing_locations(&tx)?;
    let r = forget(&tx, &locations, 0, true)?;
    tx.rollback().map_err(|e| e.to_string())?;
    Ok(r)
}

/// Forget every file a complete scan found missing. Offline drives aren't affected:
/// their files are never marked missing.
pub fn remove_missing(connection: &mut Connection, expected: &Removal) -> Result<Removal, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let locations = missing_locations(&tx)?;
    let r = forget(&tx, &locations, 0, false)?;
    if &r != expected {
        return Err("The catalog changed since you looked (maybe a scan finished), so nothing was removed. Look again.".into());
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(r)
}

/// A cell Excel won't run as a formula.
fn cell(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    }
}

/// Write the game list to a new CSV file in `folder`; `date` names it.
pub fn export(connection: &Connection, folder: &Path, date: &str, presence: PresenceFn) -> Result<Exported, String> {
    let mut drives: HashMap<String, bool> = HashMap::new();
    {
        let mut s = connection.prepare("SELECT id, root_path, stable_id FROM storage_locations").map_err(|e| e.to_string())?;
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))).map_err(|e| e.to_string())? {
            let (id, root, stable) = row.map_err(|e| e.to_string())?;
            drives.insert(id, presence(&root, stable.as_deref()) == Presence::Connected);
        }
    }
    let mut out = csv::Writer::from_writer(Vec::new());
    out.write_record(["Title", "Platform", "Year", "Genre", "Players", "File", "Size (MB)", "Drive", "Path", "Status", "Collections"]).map_err(|e| e.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT COALESCE(g.display_title, g.canonical_title, gf.filename), COALESCE(g.platform, gf.platform_hint), re.year, re.genre, gd.players,
                    gf.filename, gf.payload_bytes, s.display_name, s.root_path, fl.relative_path, fl.present_status, gf.excluded_at IS NOT NULL, s.id,
                    (SELECT group_concat(c.name, '; ') FROM collection_games cg JOIN collections c ON c.id = cg.collection_id WHERE cg.game_file_id = gf.id)
             FROM game_files gf JOIN file_locations fl ON fl.game_file_id = gf.id JOIN storage_locations s ON s.id = fl.storage_location_id
             LEFT JOIN game_releases r ON r.id = gf.release_id LEFT JOIN games g ON g.id = r.game_id
             LEFT JOIN game_reference gr ON gr.game_id = g.id LEFT JOIN reference_entries re ON re.id = gr.entry_id
             LEFT JOIN game_details gd ON gd.game_file_id = gf.id
             WHERE gf.file_role = 'primary'
             ORDER BY 2 COLLATE NOCASE, 1 COLLATE NOCASE, fl.relative_path",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = 0;
    let mut query = statement.query([]).map_err(|e| e.to_string())?;
    while let Some(r) = query.next().map_err(|e| e.to_string())? {
        let get = |i: usize| r.get::<_, Option<String>>(i).ok().flatten().unwrap_or_default();
        let year = r.get::<_, Option<i64>>(2).ok().flatten().map(|y| y.to_string()).unwrap_or_default();
        let bytes: i64 = r.get(6).unwrap_or(0);
        let left_out: bool = r.get(11).unwrap_or(false);
        let storage = get(12);
        let mut status = if !drives.get(&storage).copied().unwrap_or(false) {
            "Drive not connected"
        } else if get(10) == "missing" {
            "Missing"
        } else {
            "On drive"
        }
        .to_string();
        if left_out {
            status.push_str(", left out");
        }
        let path = crate::paths::join(&get(8), &get(9)).display().to_string();
        let record = [get(0), get(1), year, get(3), get(4), get(5), format!("{:.1}", bytes as f64 / 1_048_576.0), get(7), path, status, get(13)];
        out.write_record(record.iter().map(|t| cell(t))).map_err(|e| e.to_string())?;
        rows += 1;
    }
    let mut bytes = b"\xEF\xBB\xBF".to_vec(); // so Excel reads accents and Japanese titles
    bytes.extend(out.into_inner().map_err(|e| e.to_string())?);

    fs::create_dir_all(folder).map_err(|e| format!("Couldn't create {}: {e}", folder.display()))?;
    let base = format!("RetroDrive library {date}");
    let name = (1..1000)
        .map(|n| if n == 1 { format!("{base}.csv") } else { format!("{base} ({n}).csv") })
        .find(|c| fs::symlink_metadata(folder.join(c)).is_err())
        .ok_or("Couldn't find a free name for the file.")?;
    let target = folder.join(&name);
    let marker = format!("{}{}", copier::TEMP_MARKER, &db::new_id()[..8]);
    let temp = folder.join(format!("{name}{marker}"));
    let written = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written.map_err(|e| e.to_string()).and_then(|_| copier::rename_no_replace(&temp, &target).map_err(|e| e.to_string())) {
        copier::remove_own_temp(&temp, &marker);
        return Err(format!("Couldn't save the game list: {e}"));
    }
    Ok(Exported { path: target.display().to_string(), name, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    use crate::volumes::VolumeProbe;

    fn catalog() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        db::configure(&c).unwrap();
        db::migrate(&mut c, None).unwrap();
        c
    }
    fn local(_: &str) -> VolumeProbe {
        VolumeProbe { kind: "local", identity: None }
    }
    fn here(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = walkdir::WalkDir::new(root).into_iter().map(Result::unwrap).filter(|e| e.file_type().is_file()).map(|e| (e.path().display().to_string(), fs::read(e.path()).unwrap())).collect();
        v.sort();
        v
    }
    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }
    fn titles(c: &Connection) -> Vec<String> {
        let mut s = c.prepare("SELECT canonical_title FROM games ORDER BY 1").unwrap();
        s.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }
    fn library(root: &Path) {
        write(root, "a/snes/Alpha (USA).sfc", b"alpha");
        write(root, "a/nes/Beta (USA).nes", b"beta");
        write(root, "a/psx/Disc Game (USA)/Disc Game (USA) (Disc 1).cue", b"FILE \"Disc Game (USA) (Disc 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n");
        write(root, "a/psx/Disc Game (USA)/Disc Game (USA) (Disc 1).bin", b"bin1");
        write(root, "b/snes/Gamma (USA).sfc", b"gamma");
    }

    #[test]
    fn removing_a_folder_forgets_only_its_entries_and_never_touches_files() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let mut c = catalog();
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        scanner::scan(&mut c, &dir.path().join("b"), &local).unwrap();
        // A collection and an art pick on a file that will go.
        let alpha: String = c.query_row("SELECT gf.id FROM game_files gf WHERE filename LIKE 'Alpha%'", [], |r| r.get(0)).unwrap();
        c.execute("INSERT INTO collections (id, name) VALUES ('c1', 'Mine')", []).unwrap();
        c.execute("INSERT INTO collection_games (collection_id, game_file_id) VALUES ('c1', ?1)", [&alpha]).unwrap();
        let before = snapshot(dir.path());

        let list = folders(&c, &here).unwrap();
        assert_eq!(list.len(), 2);
        let a = list.iter().find(|f| f.path.ends_with('a')).unwrap();
        assert!(a.connected && a.files >= 4, "{a:?}");

        let preview = folder_preview(&mut c, &a.id).unwrap();
        assert_eq!((preview.games, preview.in_collections, preview.kept), (3, 1, 0), "{preview:?}");
        assert_eq!(titles(&c).len(), 4, "the preview changes nothing");

        let done = remove_folder(&mut c, &a.id, &preview).unwrap();
        assert_eq!(done, preview);
        assert_eq!(titles(&c), vec!["Gamma (USA)"]);
        assert_eq!(folders(&c, &here).unwrap().len(), 1);
        assert_eq!(count(&c, "collection_games"), 0);
        assert_eq!(count(&c, "collections"), 1, "the collection itself stays");
        assert_eq!(count(&c, "games_fts"), 1);
        let trigger: i64 = c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'games_fts_delete'", [], |r| r.get(0)).unwrap();
        assert_eq!(trigger, 1, "the search index's trigger is put back");
        assert_eq!(snapshot(dir.path()), before, "files on disk are never touched");

        // Scanning the other folder again doesn't bring them back.
        scanner::scan(&mut c, &dir.path().join("b"), &local).unwrap();
        assert_eq!(titles(&c), vec!["Gamma (USA)"]);
        // Scanning the removed folder again does.
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        assert_eq!(titles(&c).len(), 4);
    }

    #[test]
    fn a_folder_inside_another_keeps_entries_the_other_still_holds() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let mut c = catalog();
        scanner::scan(&mut c, dir.path(), &local).unwrap();
        scanner::scan(&mut c, &dir.path().join("b"), &local).unwrap();
        let list = folders(&c, &here).unwrap();
        let inner = list.iter().find(|f| f.path.ends_with('b')).unwrap();
        let preview = folder_preview(&mut c, &inner.id).unwrap();
        assert_eq!((preview.files, preview.games, preview.kept), (0, 0, 1));
        remove_folder(&mut c, &inner.id, &preview).unwrap();
        assert_eq!(titles(&c).len(), 4, "the outer folder still holds Gamma");
    }

    #[test]
    fn a_stale_preview_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let mut c = catalog();
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        let id = folders(&c, &here).unwrap()[0].id.clone();
        let preview = folder_preview(&mut c, &id).unwrap();
        write(dir.path(), "a/gb/Delta (USA).gb", b"delta");
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        assert!(remove_folder(&mut c, &id, &preview).unwrap_err().contains("changed since"));
        assert_eq!(titles(&c).len(), 4);
        assert!(remove_folder(&mut c, "nope", &preview).is_err());
    }

    #[test]
    fn cleaning_up_missing_entries_forgets_only_files_a_scan_found_gone() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        let mut c = catalog();
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        assert_eq!(missing_preview(&mut c).unwrap(), Removal::default());
        fs::remove_file(dir.path().join("a/nes/Beta (USA).nes")).unwrap();
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        let before = snapshot(dir.path());
        let preview = missing_preview(&mut c).unwrap();
        assert_eq!((preview.files, preview.games), (1, 1));
        remove_missing(&mut c, &preview).unwrap();
        assert_eq!(titles(&c), vec!["Alpha (USA)", "Disc Game (USA) (Disc 1)"]);
        assert_eq!(snapshot(dir.path()), before);
        assert_eq!(count(&c, "indexed_roots"), 1, "the folder stays");
    }

    #[test]
    fn the_export_is_a_new_csv_with_one_row_per_game_file() {
        let dir = tempfile::tempdir().unwrap();
        library(dir.path());
        write(dir.path(), "a/nes/=cmd (USA).nes", b"formula");
        let mut c = catalog();
        scanner::scan(&mut c, &dir.path().join("a"), &local).unwrap();
        let out = dir.path().join("docs");
        let e = export(&c, &out, "2026-11-09", &here).unwrap();
        assert_eq!((e.name.as_str(), e.rows), ("RetroDrive library 2026-11-09.csv", 4));
        let bytes = fs::read(&e.path).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let mut lines = text.lines();
        assert_eq!(lines.next().unwrap(), "Title,Platform,Year,Genre,Players,File,Size (MB),Drive,Path,Status,Collections");
        assert!(text.contains("'=cmd"), "formulas are defused: {text}");
        assert!(text.contains("On drive"));
        assert!(!text.contains(".bin"), "tracks are part of their game, not rows");
        // A second export never replaces the first.
        let again = export(&c, &out, "2026-11-09", &here).unwrap();
        assert_eq!(again.name, "RetroDrive library 2026-11-09 (2).csv");
        assert_eq!(fs::read(&e.path).unwrap(), fs::read(&again.path).unwrap());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 2, "no temporary files left");
    }
}
