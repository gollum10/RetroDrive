//! Reset the library catalog (owner 2026-11-09: catalog tools with a reset, "Library
//! only"; ADR-098; docs/18 §62).
//!
//! **Reset** first saves a full copy of the catalog next to it
//! (`retrodrive-before-reset-<date time>.sqlite`, made with SQLite's `VACUUM INTO`
//! under a temporary name, checked, then named without replacing anything), and only
//! then clears the **library**: scanned folders, games, files, collections, art picks,
//! game details, catalog matches, Leave-out choices and scan history. It keeps the
//! emulation systems and their choices, copy history, designs, settings history,
//! controller and gun choices, Theme Studio projects and the imported reference catalog.
//! No file on any drive is touched.
//!
//! **Put the old catalog back** first saves the current catalog the same way
//! (`retrodrive-before-put-back-…`), then replaces the library with the saved copy's
//! (a working copy of it is brought to this version's schema first, so a copy from an
//! older RetroDrive works too). Saved copies are never deleted by RetroDrive.

use crate::{copier, db};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::fs;
use std::path::Path;

const PREFIX: &str = "retrodrive-before-";

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct Counts {
    pub games: i64,
    pub files: i64,
    pub folders: i64,
    pub collections: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResetPreview {
    /// What the reset clears.
    pub clears: Counts,
    /// What it keeps.
    pub systems: i64,
    pub reference_entries: i64,
    pub theme_projects: i64,
    pub designs: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SavedCatalog {
    pub path: String,
    pub name: String,
    /// `reset` or `put-back`: what was about to happen when it was saved.
    pub before: String,
    pub saved_at: String,
    pub counts: Option<Counts>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Done {
    /// The copy saved first.
    pub saved: SavedCatalog,
    /// The library now.
    pub counts: Counts,
}

fn count(c: &Connection, sql: &str) -> Result<i64, String> {
    c.query_row(sql, [], |r| r.get(0)).map_err(|e| e.to_string())
}

pub fn counts(c: &Connection) -> Result<Counts, String> {
    Ok(Counts {
        games: count(c, "SELECT COUNT(*) FROM games")?,
        files: count(c, "SELECT COUNT(*) FROM game_files")?,
        folders: count(c, "SELECT COUNT(*) FROM indexed_roots")?,
        collections: count(c, "SELECT COUNT(*) FROM collections")?,
    })
}

/// What a reset would clear and keep (reads only).
pub fn preview(c: &Connection) -> Result<ResetPreview, String> {
    Ok(ResetPreview {
        clears: counts(c)?,
        systems: count(c, "SELECT COUNT(*) FROM installations")?,
        reference_entries: count(c, "SELECT COUNT(*) FROM reference_entries")?,
        theme_projects: count(c, "SELECT COUNT(*) FROM theme_projects")?,
        designs: count(c, "SELECT COUNT(*) FROM designs")?,
    })
}

/// The triggers that keep titles and the search index in step row by row. Several of
/// them look a game up in the search index by an unindexed column (one full pass per
/// row: hours for a big library), so bulk changes set them all aside (inside the
/// caller's transaction), recompute titles and rebuild the search index in one go,
/// and put every trigger back exactly as it was.
const ROW_TRIGGERS: &[&str] = &[
    "games_fts_insert",
    "games_fts_update",
    "games_fts_delete",
    "games_display_title_insert",
    "games_display_title_rename",
    "game_reference_display_insert",
    "game_reference_display_update",
    "game_reference_display_delete",
];

fn in_bulk<T>(tx: &Connection, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let mut saved = Vec::new();
    for name in ROW_TRIGGERS {
        let sql: Option<String> = tx
            .query_row("SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = ?1", [name], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(sql) = sql {
            tx.execute_batch(&format!("DROP TRIGGER \"{name}\";")).map_err(|e| e.to_string())?;
            saved.push(sql);
        }
    }
    let result = work()?;
    tx.execute_batch(
        "UPDATE games SET display_title = COALESCE(
             (SELECT re.title FROM game_reference gr JOIN reference_entries re ON re.id = gr.entry_id WHERE gr.game_id = games.id),
             canonical_title);
         DELETE FROM games_fts;
         INSERT INTO games_fts (game_id, canonical_title, platform, display_title) SELECT id, canonical_title, platform, display_title FROM games;",
    )
    .map_err(|e| e.to_string())?;
    for sql in saved {
        tx.execute_batch(&sql).map_err(|e| e.to_string())?;
    }
    Ok(result)
}

/// Clear the library tables (inside `tx`). Everything else stays.
fn clear_library(tx: &Connection) -> Result<(), String> {
    in_bulk(tx, || {
        tx.execute_batch(
            "DELETE FROM collection_games;
             DELETE FROM collections;
             DELETE FROM game_media;
             DELETE FROM game_details;
             DELETE FROM game_reference;
             DELETE FROM file_locations;
             UPDATE game_files SET parent_file_id = NULL WHERE parent_file_id IS NOT NULL;
             DELETE FROM game_files;
             DELETE FROM game_releases;
             DELETE FROM games;
             DELETE FROM games_fts;
             DELETE FROM indexed_roots;
             DELETE FROM job_events WHERE job_id IN (SELECT id FROM jobs WHERE kind = 'library_scan');
             DELETE FROM jobs WHERE kind = 'library_scan' AND id NOT IN (SELECT job_id FROM deployments WHERE job_id IS NOT NULL);
             DELETE FROM storage_locations WHERE id NOT IN (SELECT storage_location_id FROM installations);",
        )
        .map_err(|e| e.to_string())
    })
}

/// `retrodrive-before-reset-2026-11-09 21.30.05.sqlite`, or with ` (2)` if taken.
fn free_name(dir: &Path, before: &str, stamp: &str) -> Result<String, String> {
    (1..1000)
        .map(|n| if n == 1 { format!("{PREFIX}{before}-{stamp}.sqlite") } else { format!("{PREFIX}{before}-{stamp} ({n}).sqlite") })
        .find(|c| fs::symlink_metadata(dir.join(c)).is_err())
        .ok_or_else(|| "Couldn't find a free name for the saved catalog.".into())
}

fn read_only(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| e.to_string())
}

/// Save a full, checked copy of the catalog next to it. Returns it.
fn save_copy(c: &Connection, db_path: &Path, before: &str, stamp: &str) -> Result<SavedCatalog, String> {
    let dir = db_path.parent().ok_or("The catalog has no folder.")?;
    let name = free_name(dir, before, stamp)?;
    let target = dir.join(&name);
    let marker = format!("{}{}", copier::TEMP_MARKER, &db::new_id()[..8]);
    let temp = dir.join(format!("{name}{marker}"));
    let fail = |m: String| -> Result<SavedCatalog, String> {
        copier::remove_own_temp(&temp, &marker);
        Err(format!("Couldn't save a copy of the catalog ({m}), so nothing was changed."))
    };
    let expected = counts(c)?;
    if let Err(e) = c.execute("VACUUM INTO ?1", [temp.display().to_string()]) {
        return fail(e.to_string());
    }
    let checked = (|| -> Result<(), String> {
        let copy = read_only(&temp)?;
        let ok: String = copy.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        if ok != "ok" {
            return Err(format!("check: {ok}"));
        }
        if db::user_version(&copy)? != db::user_version(c)? || counts(&copy)? != expected {
            return Err("the copy didn't match".into());
        }
        Ok(())
    })();
    if let Err(e) = checked {
        return fail(e);
    }
    if let Err(e) = copier::rename_no_replace(&temp, &target) {
        return fail(e.to_string());
    }
    Ok(SavedCatalog { path: target.display().to_string(), name, before: before.into(), saved_at: stamp.replace('.', ":"), counts: Some(expected) })
}

/// Reset the library. `expected` is the preview the user saw (`clears`).
pub fn reset(db_path: &Path, expected: &Counts, stamp: &str) -> Result<Done, String> {
    let mut c = db::open(db_path)?;
    if &counts(&c)? != expected {
        return Err("The library changed since you looked (maybe a scan finished), so nothing was reset. Look again.".into());
    }
    let saved = save_copy(&c, db_path, "reset", stamp)?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    clear_library(&tx)?;
    let violations = count(&tx, "SELECT COUNT(*) FROM pragma_foreign_key_check")?;
    if violations > 0 {
        return Err(format!("The reset would leave {violations} broken references, so nothing was changed. Your catalog was also saved as {}.", saved.name));
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Done { counts: counts(&c)?, saved })
}

/// The saved catalogs next to the catalog, newest first (reads only).
pub fn saved(db_path: &Path) -> Vec<SavedCatalog> {
    let Some(dir) = db_path.parent() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SavedCatalog> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(PREFIX)?.strip_suffix(".sqlite")?;
            let (before, stamp) = if let Some(s) = rest.strip_prefix("reset-") { ("reset", s) } else { ("put-back", rest.strip_prefix("put-back-")?) };
            let saved_at = stamp.split(" (").next().unwrap_or(stamp).replace('.', ":");
            let before = before.to_string();
            let counts = read_only(&e.path()).ok().and_then(|c| counts(&c).ok());
            Some(SavedCatalog { path: e.path().display().to_string(), name, before, saved_at, counts })
        })
        .collect();
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(b.name.cmp(&a.name)));
    out
}

/// The columns of `table` in the main catalog.
fn columns(tx: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut s = tx.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).map_err(|e| e.to_string())?;
    let names = s.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    names.map(|r| r.map_err(|e| e.to_string())).collect()
}

/// Copy `table`'s rows from the attached `old` catalog; `map` replaces a column's value
/// with an expression over `t` (the old row); `filter` limits the rows.
fn copy_rows(tx: &Connection, table: &str, map: &[(&str, &str)], filter: &str) -> Result<(), String> {
    let cols = columns(tx, table)?;
    let select: Vec<String> = cols.iter().map(|c| map.iter().find(|(m, _)| m == c).map(|(_, e)| e.to_string()).unwrap_or(format!("t.\"{c}\""))).collect();
    let list: Vec<String> = cols.iter().map(|c| format!("\"{c}\"")).collect();
    let sql = format!("INSERT INTO main.\"{table}\" ({}) SELECT {} FROM old.\"{table}\" t {filter}", list.join(", "), select.join(", "));
    tx.execute_batch(&sql).map_err(|e| format!("{table}: {e}"))
}

/// Put a saved catalog's library back. The current catalog is saved first.
pub fn put_back(db_path: &Path, copy: &Path, stamp: &str) -> Result<Done, String> {
    let dir = db_path.parent().ok_or("The catalog has no folder.")?;
    let named = copy.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".sqlite"));
    if copy.parent() != Some(dir) || !named || !copy.is_file() {
        return Err("That isn't a saved catalog.".into());
    }
    // A working copy, brought to this version's schema (the saved copy isn't changed).
    let marker = format!("{}{}", copier::TEMP_MARKER, &db::new_id()[..8]);
    let work = dir.join(format!("put-back.sqlite{marker}"));
    let cleanup = || {
        copier::remove_own_temp(&work, &marker);
    };
    let prepared = (|| -> Result<(), String> {
        let source = read_only(copy)?;
        source.execute("VACUUM INTO ?1", [work.display().to_string()]).map_err(|e| e.to_string())?;
        drop(source);
        let mut w = db::open(&work)?;
        db::migrate(&mut w, None)?;
        Ok(())
    })();
    if let Err(e) = prepared {
        cleanup();
        return Err(format!("That saved catalog can't be read ({e}), so nothing was changed."));
    }
    let result = (|| -> Result<Done, String> {
        let mut c = db::open(db_path)?;
        let saved = save_copy(&c, db_path, "put-back", stamp)?;
        c.execute("ATTACH DATABASE ?1 AS old", [work.display().to_string()]).map_err(|e| e.to_string())?;
        let applied = (|| -> Result<(), String> {
            let tx = c.transaction().map_err(|e| e.to_string())?;
            tx.execute_batch("PRAGMA defer_foreign_keys = ON;").map_err(|e| e.to_string())?;
            clear_library(&tx)?;
            in_bulk(&tx, || {
            // Drives: the same id, or the same drive (fingerprint) under a new id, or added.
            tx.execute_batch(
                "CREATE TEMP TABLE storage_map (old_id TEXT PRIMARY KEY, new_id TEXT NOT NULL);
                 INSERT INTO storage_map SELECT o.id, COALESCE(
                     (SELECT m.id FROM main.storage_locations m WHERE m.id = o.id),
                     (SELECT m.id FROM main.storage_locations m WHERE o.stable_id IS NOT NULL AND m.stable_id = o.stable_id),
                     o.id) FROM old.storage_locations o;",
            )
            .map_err(|e| e.to_string())?;
            copy_rows(&tx, "storage_locations", &[], "WHERE t.id NOT IN (SELECT id FROM main.storage_locations) AND (t.stable_id IS NULL OR t.stable_id NOT IN (SELECT stable_id FROM main.storage_locations WHERE stable_id IS NOT NULL))")?;
            let drive = ("storage_location_id", "(SELECT new_id FROM temp.storage_map WHERE old_id = t.storage_location_id)");
            copy_rows(&tx, "indexed_roots", &[drive], "")?;
            copy_rows(&tx, "games", &[], "")?;
            copy_rows(&tx, "game_releases", &[], "")?;
            copy_rows(&tx, "game_files", &[], "")?;
            copy_rows(&tx, "file_locations", &[drive], "")?;
            copy_rows(&tx, "game_media", &[drive], "")?;
            copy_rows(&tx, "game_details", &[], "")?;
            // Matches to the reference catalog, where its entries are still the same.
            copy_rows(&tx, "game_reference", &[], "WHERE t.entry_id IN (SELECT id FROM main.reference_entries)")?;
            copy_rows(&tx, "collections", &[], "")?;
            copy_rows(&tx, "collection_games", &[], "")?;
            Ok(())
            })?;
            copy_rows(&tx, "jobs", &[], "WHERE t.kind = 'library_scan' AND t.id NOT IN (SELECT id FROM main.jobs)")?;
            copy_rows(&tx, "job_events", &[], "WHERE t.job_id IN (SELECT id FROM old.jobs WHERE kind = 'library_scan') AND t.id NOT IN (SELECT id FROM main.job_events)")?;
            // Copy history: link its records to the library files again.
            tx.execute_batch(
                "UPDATE main.deployment_items SET game_file_id = (SELECT o.game_file_id FROM old.deployment_items o WHERE o.id = main.deployment_items.id)
                 WHERE game_file_id IS NULL
                   AND (SELECT o.game_file_id FROM old.deployment_items o WHERE o.id = main.deployment_items.id) IN (SELECT id FROM main.game_files);
                 DROP TABLE temp.storage_map;",
            )
            .map_err(|e| e.to_string())?;
            let violations = count(&tx, "SELECT COUNT(*) FROM pragma_foreign_key_check")?;
            if violations > 0 {
                return Err(format!("{violations} broken references"));
            }
            tx.commit().map_err(|e| e.to_string())
        })();
        let _ = c.execute("DETACH DATABASE old", []);
        applied.map_err(|e| format!("The saved catalog couldn't be put back ({e}), so nothing was changed. The current catalog was also saved as {}.", saved.name))?;
        Ok(Done { counts: counts(&c)?, saved })
    })();
    cleanup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    use crate::volumes::VolumeProbe;
    use std::path::PathBuf;

    fn local(_: &str) -> VolumeProbe {
        VolumeProbe { kind: "local", identity: None }
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
    fn table(c: &Connection, sql: &str) -> Vec<String> {
        let mut s = c.prepare(sql).unwrap();
        let n = s.column_count();
        s.query_map([], |r| Ok((0..n).map(|i| format!("{:?}", r.get_ref(i).unwrap())).collect::<Vec<_>>().join("|"))).unwrap().map(Result::unwrap).collect()
    }
    /// A catalog file with a scanned library, a collection, a left-out game, a system
    /// and a design. Returns (catalog folder, games folder, catalog path).
    fn setup() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let data = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        write(games.path(), "snes/Alpha (USA).sfc", b"alpha");
        write(games.path(), "nes/Beta (USA).nes", b"beta");
        write(games.path(), "psx/Disc (USA)/Disc (USA).cue", b"FILE \"Disc (USA).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n");
        write(games.path(), "psx/Disc (USA)/Disc (USA).bin", b"bin");
        let path = data.path().join("retrodrive.sqlite");
        let mut c = db::open(&path).unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, games.path(), &local).unwrap();
        let alpha: String = c.query_row("SELECT id FROM game_files WHERE filename LIKE 'Alpha%'", [], |r| r.get(0)).unwrap();
        c.execute("INSERT INTO collections (id, name) VALUES ('c1', 'Mine')", []).unwrap();
        c.execute("INSERT INTO collection_games (collection_id, game_file_id) VALUES ('c1', ?1)", [&alpha]).unwrap();
        c.execute("UPDATE game_files SET excluded_at = '2026-11-09', excluded_reason = 'user' WHERE filename LIKE 'Beta%'", []).unwrap();
        let storage: String = c.query_row("SELECT id FROM storage_locations", [], |r| r.get(0)).unwrap();
        c.execute("INSERT INTO installations (id, adapter_id, adapter_contract, storage_location_id, relative_path) VALUES ('i1', 'retrobat', 2, ?1, 'RetroBat')", [&storage]).unwrap();
        c.execute("INSERT INTO deployments (id, installation_id, adapter_id, adapter_contract, destination_root, plan_fingerprint, status) VALUES ('d1', 'i1', 'retrobat', 2, 'G:/RetroBat', 'f', 'completed')", []).unwrap();
        c.execute("INSERT INTO deployment_items (id, deployment_id, game_index, game_title, game_file_id, source_path, destination_path, size_bytes, planned_action, state) VALUES (7, 'd1', 0, 'Alpha', ?1, 's', 'd', 5, 'copy', 'verified')", [&alpha]).unwrap();
        c.execute("INSERT INTO designs (id, name) VALUES ('ds1', 'Look')", []).unwrap();
        (data, games, path)
    }
    const LIBRARY: &str = "SELECT g.canonical_title, gf.filename, gf.excluded_at, fl.relative_path FROM games g JOIN game_releases r ON r.game_id = g.id JOIN game_files gf ON gf.release_id = r.id JOIN file_locations fl ON fl.game_file_id = gf.id ORDER BY 2";

    #[test]
    fn reset_saves_a_copy_first_then_clears_only_the_library() {
        let (data, games, path) = setup();
        let before_files = snapshot(games.path());
        let c = db::open(&path).unwrap();
        let p = preview(&c).unwrap();
        assert_eq!((p.clears.games, p.clears.folders, p.clears.collections, p.systems), (3, 1, 1, 1));
        let library = table(&c, LIBRARY);
        let designs = table(&c, "SELECT * FROM designs");
        let triggers = table(&c, "SELECT name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name");
        assert!(triggers.len() >= 8);
        drop(c);

        let done = reset(&path, &p.clears, "2026-11-09 21.30.05").unwrap();
        assert_eq!(done.counts, Counts::default());
        assert_eq!(done.saved.name, "retrodrive-before-reset-2026-11-09 21.30.05.sqlite");
        assert_eq!(done.saved.counts.as_ref(), Some(&p.clears));
        let c = db::open(&path).unwrap();
        assert_eq!(c.query_row("SELECT COUNT(*) FROM games_fts", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(c.query_row("SELECT COUNT(*) FROM installations", [], |r| r.get::<_, i64>(0)).unwrap(), 1, "systems stay");
        assert_eq!(table(&c, "SELECT * FROM designs"), designs, "designs stay");
        let item: (String, Option<String>) = c.query_row("SELECT deployment_id, game_file_id FROM deployment_items WHERE id = 7", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(item, ("d1".to_string(), None), "copy history stays, unlinked");
        assert_eq!(c.query_row("SELECT COUNT(*) FROM storage_locations", [], |r| r.get::<_, i64>(0)).unwrap(), 1, "the system's drive stays");
        assert_eq!(table(&c, "SELECT name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name"), triggers, "every trigger is put back as it was");
        // The saved copy holds the whole library.
        let copy = read_only(Path::new(&done.saved.path)).unwrap();
        assert_eq!(table(&copy, LIBRARY), library);
        assert_eq!(snapshot(games.path()), before_files, "no game file is touched");
        assert!(!fs::read_dir(data.path()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(copier::TEMP_MARKER)));
        // Listed.
        let list = saved(&path);
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].before.as_str(), list[0].saved_at.as_str()), ("reset", "2026-11-09 21:30:05"));
    }

    #[test]
    fn putting_the_old_catalog_back_restores_the_library_and_saves_the_current_one() {
        let (_data, games, path) = setup();
        let c = db::open(&path).unwrap();
        let p = preview(&c).unwrap();
        let library = table(&c, LIBRARY);
        let collections = table(&c, "SELECT * FROM collection_games");
        let fts = table(&c, "SELECT game_id, canonical_title FROM games_fts ORDER BY 1");
        drop(c);
        let done = reset(&path, &p.clears, "2026-11-09 21.30.05").unwrap();

        // After the reset, a new scan of only one folder, and a new system's drive.
        let mut c = db::open(&path).unwrap();
        scanner::scan(&mut c, &games.path().join("snes"), &local).unwrap();
        drop(c);

        let back = put_back(&path, Path::new(&done.saved.path), "2026-11-09 22.00.00").unwrap();
        assert_eq!(back.counts, p.clears);
        assert_eq!(back.saved.before, "put-back");
        let c = db::open(&path).unwrap();
        assert_eq!(table(&c, LIBRARY), library);
        assert_eq!(table(&c, "SELECT * FROM collection_games"), collections);
        assert_eq!(table(&c, "SELECT game_id, canonical_title FROM games_fts ORDER BY 1"), fts, "search works again");
        let linked: Option<String> = c.query_row("SELECT game_file_id FROM deployment_items WHERE id = 7", [], |r| r.get(0)).unwrap();
        let alpha: String = c.query_row("SELECT id FROM game_files WHERE filename LIKE 'Alpha%'", [], |r| r.get(0)).unwrap();
        assert_eq!(linked, Some(alpha), "copy history is linked to the library again");
        // The catalog from before putting back (with the one-folder scan) was saved too.
        let list = saved(&path);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].before, "put-back");
        assert_eq!(list[0].counts.as_ref().map(|c| c.games), Some(1));
        // The saved copy itself is unchanged and no working file is left.
        assert_eq!(read_only(Path::new(&done.saved.path)).map(|c| counts(&c).unwrap()).unwrap(), p.clears);
        assert!(!fs::read_dir(path.parent().unwrap()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(copier::TEMP_MARKER)));
    }

    #[test]
    fn a_stale_preview_or_a_foreign_file_changes_nothing() {
        let (data, _games, path) = setup();
        let c = db::open(&path).unwrap();
        let mut p = preview(&c).unwrap().clears;
        drop(c);
        p.games += 1;
        assert!(reset(&path, &p, "2026-11-09 21.30.05").unwrap_err().contains("changed since"));
        assert!(saved(&path).is_empty(), "no copy made");
        let other = data.path().join("photos.sqlite");
        fs::write(&other, b"x").unwrap();
        assert!(put_back(&path, &other, "x").unwrap_err().contains("isn't a saved catalog"));
        let fake = data.path().join("retrodrive-before-reset-2026.sqlite");
        fs::write(&fake, b"not a database").unwrap();
        let c = db::open(&path).unwrap();
        let before = table(&c, LIBRARY);
        drop(c);
        assert!(put_back(&path, &fake, "2026-11-09 22.00.00").unwrap_err().contains("can't be read"));
        assert_eq!(table(&db::open(&path).unwrap(), LIBRARY), before);
        assert_eq!(saved(&path).iter().filter(|s| s.before == "put-back").count(), 0, "nothing saved when the copy can't be read");
    }

    #[test]
    fn a_copy_from_an_older_version_is_brought_up_to_date_first() {
        let (data, _games, path) = setup();
        let c = db::open(&path).unwrap();
        let library = table(&c, LIBRARY);
        // Pretend this copy was saved by a version without v28's indexes.
        let old = data.path().join("retrodrive-before-reset-2026-11-01 10.00.00.sqlite");
        c.execute("VACUUM INTO ?1", [old.display().to_string()]).unwrap();
        drop(c);
        let o = Connection::open(&old).unwrap();
        o.execute_batch("DROP INDEX idx_game_files_parent; DROP INDEX idx_deployment_items_file; PRAGMA user_version = 27;").unwrap();
        drop(o);
        let p = preview(&db::open(&path).unwrap()).unwrap().clears;
        reset(&path, &p, "2026-11-09 21.30.05").unwrap();
        put_back(&path, &old, "2026-11-09 22.00.00").unwrap();
        assert_eq!(table(&db::open(&path).unwrap(), LIBRARY), library);
        assert_eq!(db::user_version(&read_only(&old).unwrap()).unwrap(), 27, "the saved copy itself isn't changed");
    }
}
