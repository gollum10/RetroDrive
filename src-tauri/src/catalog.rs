//! Catalog queries for the UI (RDS-004: paged, filtered, indexed).
//!
//! Availability is derived from the last scan result plus one presence check per
//! storage location, never by touching individual game files, so queries stay fast
//! on large or disconnected libraries. Duplicate candidates are files with the same
//! platform, file name (case-insensitive) and non-zero size; they are informational
//! only and nothing is ever deleted (docs/07 "Duplicate categories").
//!
//! Only each game's primary file is listed: discs of an `.m3u` game and tracks of
//! a `.cue` sheet are shown through `disc_count`, `file_size` (the game's total)
//! and `incomplete` (RDS-005).

use crate::paths;
use crate::volumes::Presence;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 500;
/// Filtered result counts stop here (shown as "10,000+"); counting every match of a
/// very broad search costs far more than fetching the page itself.
pub const COUNT_CAP: i64 = 10_000;

/// One row per physical file location, joined back to its game.
#[derive(Debug, Serialize, Clone)]
pub struct GameRow {
    pub id: String,
    pub game_id: String,
    pub title: String,
    pub platform: String,
    pub path: String,
    /// Total size of the game: the primary file plus its discs and tracks.
    pub file_size: i64,
    /// Discs listed by the game's `.m3u` playlist (0 for single-file games).
    pub disc_count: i64,
    /// True when a disc or track is missing, or a referenced file was never found.
    pub incomplete: bool,
    /// From the reference catalog, when this game is linked to an entry.
    pub reference: Option<ReferenceInfo>,
    /// The primary game file's id (used to ask for its image).
    pub file_id: String,
    /// `kind|source|added_at` of the best linked image, or `None`.
    pub art: Option<String>,
    pub storage_name: String,
    /// `online`: file was present at the last scan and its volume is attached.
    /// `offline`: the volume (drive / share) is not attached, or its drive letter
    /// now belongs to a different volume.
    /// `missing`: a complete scan of its folder did not find the file.
    pub availability: String,
    /// Number of files sharing this file's duplicate key (1 = no duplicates).
    pub duplicate_count: i64,
}

/// Library filters sent by the UI. Every field is optional.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct GameQuery {
    /// Matches inside titles and platform names, case-insensitively.
    pub text: Option<String>,
    pub platform: Option<String>,
    /// `online`, `offline` or `missing`.
    pub availability: Option<String>,
    pub storage_id: Option<String>,
    pub duplicates_only: bool,
    /// Show only files the user left out of the library (normally they are hidden).
    pub left_out: bool,
    /// Only titles starting with this letter (`A`-`Z`), or `#` for digits and symbols.
    pub letter: Option<String>,
    /// `title` (default), `platform` or `size`.
    pub sort: Option<String>,
    pub offset: u32,
    pub limit: Option<u32>,
}

/// Details from the user's reference catalog (`reference.rs`).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ReferenceInfo {
    pub title: String,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub publisher: Option<String>,
    pub developer: Option<String>,
    /// `name`, `variant` or `title`.
    pub method: String,
}

#[derive(Debug, Serialize)]
pub struct GamePage {
    pub rows: Vec<GameRow>,
    /// Rows matching the filters, across all pages (at most `COUNT_CAP + 1` when filtered).
    pub total: i64,
    /// True when `total` stopped at the cap and the real number is larger.
    pub total_capped: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PlatformCount {
    pub platform: String,
    pub files: i64,
}

/// One card on the library's Systems page.
#[derive(Debug, Serialize, PartialEq)]
pub struct SystemSummary {
    pub platform: String,
    /// Games on this platform (multi-disc games count once).
    pub games: i64,
    /// Total size of those games, including discs and tracks.
    pub bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct LibraryStats {
    pub files: i64,
    pub online: i64,
    pub offline: i64,
    pub missing: i64,
    /// Files that belong to a duplicate-candidate group.
    pub duplicate_files: i64,
    /// Files the user left out of the library (not counted above).
    pub left_out: i64,
    pub platforms: Vec<PlatformCount>,
}

type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;

/// JSON array of storage-location ids whose volume is not attached right now.
fn offline_storage_ids(connection: &Connection, presence: PresenceFn) -> Result<String, String> {
    let mut statement = connection
        .prepare("SELECT id, root_path, stable_id FROM storage_locations")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))
        .map_err(|e| e.to_string())?;
    let mut offline = Vec::new();
    for row in rows {
        let (id, root, stable_id) = row.map_err(|e| e.to_string())?;
        if presence(&root, stable_id.as_deref()) != Presence::Connected {
            offline.push(id);
        }
    }
    serde_json::to_string(&offline).map_err(|e| e.to_string())
}

/// Plain join used for counting and for loading page details.
const FROM: &str = "
    FROM games g
    JOIN game_releases r ON r.game_id = g.id
    JOIN game_files gf ON gf.release_id = r.id
    JOIN file_locations fl ON fl.game_file_id = gf.id";

/// Page lookups fix the join order with CROSS JOIN (SQLite never reorders it) so
/// the outer loop walks an index already in the requested order and stops after
/// one page, instead of sorting every row in the library (measured: docs/15).
const FROM_BY_GAME: &str = "
    FROM games g
    CROSS JOIN game_releases r ON r.game_id = g.id
    CROSS JOIN game_files gf ON gf.release_id = r.id
    CROSS JOIN file_locations fl ON fl.game_file_id = gf.id";
const FROM_BY_FILE: &str = "
    FROM game_files gf
    CROSS JOIN game_releases r ON r.id = gf.release_id
    CROSS JOIN games g ON g.id = r.game_id
    CROSS JOIN file_locations fl ON fl.game_file_id = gf.id";

/// True when another file shares this file's duplicate key (uses idx_game_files_duplicate_key).
const HAS_DUPLICATE: &str = "gf.size_bytes > 0 AND EXISTS (
    SELECT 1 FROM game_files d
    WHERE d.size_bytes = gf.size_bytes AND d.filename = gf.filename COLLATE NOCASE
      AND d.platform_hint = gf.platform_hint AND d.file_role = 'primary' AND d.id <> gf.id AND d.excluded_at IS NULL)";

/// Escape a user search string as a single FTS5 phrase.
fn fts_phrase(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// Escape LIKE wildcards; used with `ESCAPE '\'`.
fn like_pattern(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

/// Run a page of the library with filters applied.
///
/// Two phases: (1) find the ids for the requested page using only indexed columns,
/// (2) load full details, availability and duplicate counts for just those rows.
pub fn query_games(connection: &Connection, query: &GameQuery, presence: PresenceFn) -> Result<GamePage, String> {
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let offline = offline_storage_ids(connection, presence)?;
    // Always list primary files only. The unary `+` stops SQLite from choosing the
    // role index for this term, which would scan every primary file instead of
    // walking the title index or the search results.
    let mut conditions: Vec<&str> = vec!["+gf.file_role = 'primary'"];
    let mut values: Vec<Value> = Vec::new();

    if let Some(text) = query.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        // Trigram search needs at least 3 characters; shorter searches use LIKE.
        if text.chars().count() >= 3 {
            conditions.push("g.id IN (SELECT game_id FROM games_fts WHERE games_fts MATCH ?)");
            values.push(Value::Text(fts_phrase(text)));
        } else {
            conditions.push("(g.canonical_title LIKE ? ESCAPE '\\' OR g.platform LIKE ? ESCAPE '\\')");
            values.push(Value::Text(like_pattern(text)));
            values.push(Value::Text(like_pattern(text)));
        }
    }
    if let Some(platform) = query.platform.as_deref().filter(|p| !p.is_empty()) {
        conditions.push("g.platform = ?");
        values.push(Value::Text(platform.to_string()));
    }
    match query.availability.as_deref().filter(|a| !a.is_empty()) {
        None => {}
        Some("offline") => {
            conditions.push("fl.storage_location_id IN (SELECT value FROM json_each(?))");
            values.push(Value::Text(offline.clone()));
        }
        Some(state @ ("online" | "missing")) => {
            conditions.push(if state == "online" {
                "fl.present_status = 'present' AND fl.storage_location_id NOT IN (SELECT value FROM json_each(?))"
            } else {
                "fl.present_status = 'missing' AND fl.storage_location_id NOT IN (SELECT value FROM json_each(?))"
            });
            values.push(Value::Text(offline.clone()));
        }
        Some(other) => return Err(format!("Unknown availability filter: {other}")),
    }
    if let Some(storage) = query.storage_id.as_deref().filter(|s| !s.is_empty()) {
        conditions.push("fl.storage_location_id = ?");
        values.push(Value::Text(storage.to_string()));
    }
    if query.duplicates_only {
        conditions.push(HAS_DUPLICATE);
    }
    match query.letter.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        None => {}
        Some("#") => conditions.push("NOT (lower(substr(g.canonical_title, 1, 1)) BETWEEN 'a' AND 'z')"),
        Some(letter) if letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()) => {
            // A range on the title keeps the title index usable.
            let low = letter.to_ascii_lowercase();
            let high = ((low.as_bytes()[0]) + 1) as char;
            conditions.push("g.canonical_title >= ? COLLATE NOCASE AND g.canonical_title < ? COLLATE NOCASE");
            values.push(Value::Text(low));
            values.push(Value::Text(high.to_string()));
        }
        Some(other) => return Err(format!("Unknown letter filter: {other}")),
    }
    let filtered = conditions.len() > 1 || query.left_out;
    // Left-out files are hidden unless asked for; this alone doesn't make a query "filtered".
    conditions.push(if query.left_out { "gf.excluded_at IS NOT NULL" } else { "gf.excluded_at IS NULL" });
    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    let (page_from, order) = match query.sort.as_deref() {
        Some("platform") => (FROM_BY_GAME, "g.platform, g.canonical_title COLLATE NOCASE, g.rowid, fl.id"),
        Some("size") => (FROM_BY_FILE, "gf.file_role DESC, gf.payload_bytes DESC, fl.id"),
        Some("title") | None => (FROM_BY_GAME, "g.canonical_title COLLATE NOCASE, g.rowid, fl.id"),
        Some(other) => return Err(format!("Unknown sort: {other}")),
    };

    // Every game_files row has exactly one file_locations row (the scanner creates
    // them in pairs), so primary files can be counted straight from the role index.
    let total: i64 = if !filtered {
        connection.query_row("SELECT COUNT(*) FROM game_files WHERE file_role = 'primary' AND excluded_at IS NULL", [], |r| r.get(0))
    } else {
        connection.query_row(
            &format!("SELECT COUNT(*) FROM (SELECT 1 {FROM} {where_clause} LIMIT {})", COUNT_CAP + 1),
            params_from_iter(values.iter()),
            |r| r.get(0),
        )
    }
    .map_err(|e| e.to_string())?;
    let total_capped = filtered && total > COUNT_CAP;

    // The size sort walks the (file_role, payload_bytes) index, which needs an indexable role term.
    let page_extra = if query.sort.as_deref() == Some("size") { " AND gf.file_role = 'primary'" } else { "" };

    // Phase 1: ids for this page.
    let mut page_values = values.clone();
    page_values.push(Value::Integer(i64::from(limit)));
    page_values.push(Value::Integer(i64::from(query.offset)));
    let ids: Vec<String> = {
        let mut statement = connection
            .prepare(&format!("SELECT fl.id {page_from} {where_clause}{page_extra} ORDER BY {order} LIMIT ? OFFSET ?"))
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map(params_from_iter(page_values.iter()), |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    if ids.is_empty() {
        return Ok(GamePage { rows: Vec::new(), total, total_capped, offset: query.offset, limit });
    }

    // Phase 2: details for those rows only.
    let ids_json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT fl.id, g.id, g.canonical_title, g.platform, s.root_path, fl.relative_path, gf.payload_bytes, s.display_name,
                    CASE WHEN fl.storage_location_id IN (SELECT value FROM json_each(?2)) THEN 'offline'
                         WHEN fl.present_status = 'missing' THEN 'missing' ELSE 'online' END,
                    CASE WHEN gf.size_bytes > 0 THEN (
                        SELECT COUNT(*) FROM game_files d
                        WHERE d.size_bytes = gf.size_bytes AND d.filename = gf.filename COLLATE NOCASE
                          AND d.platform_hint = gf.platform_hint AND d.file_role = 'primary' AND d.excluded_at IS NULL)
                    ELSE 1 END,
                    (SELECT COUNT(*) FROM game_files m WHERE m.release_id = gf.release_id AND m.file_role = 'disc'),
                    EXISTS (SELECT 1 FROM game_files m WHERE m.release_id = gf.release_id AND m.missing_parts > 0)
                    OR EXISTS (SELECT 1 FROM game_files m JOIN file_locations ml ON ml.game_file_id = m.id
                               WHERE m.release_id = gf.release_id AND m.file_role <> 'primary' AND ml.present_status = 'missing'),
                    re.title, re.year, re.genre, re.publisher, re.developer, gr.method, gf.id, {art}
             {FROM} JOIN storage_locations s ON s.id = fl.storage_location_id
             LEFT JOIN game_reference gr ON gr.game_id = g.id LEFT JOIN reference_entries re ON re.id = gr.entry_id
             WHERE fl.id IN (SELECT value FROM json_each(?1))",
            art = crate::media::best_media_sql("gf.id")
        ))
        .map_err(|e| e.to_string())?;
    let mut by_id: std::collections::HashMap<String, GameRow> = statement
        .query_map([&ids_json, &offline], |r| {
            let root: String = r.get(4)?;
            let relative: String = r.get(5)?;
            Ok(GameRow {
                id: r.get(0)?,
                game_id: r.get(1)?,
                title: r.get(2)?,
                platform: r.get(3)?,
                path: paths::join(&root, &relative).display().to_string(),
                file_size: r.get(6)?,
                storage_name: r.get(7)?,
                availability: r.get(8)?,
                duplicate_count: r.get(9)?,
                disc_count: r.get(10)?,
                incomplete: r.get(11)?,
                reference: match r.get::<_, Option<String>>(12)? {
                    Some(title) => Some(ReferenceInfo { title, year: r.get(13)?, genre: r.get(14)?, publisher: r.get(15)?, developer: r.get(16)?, method: r.get(17)? }),
                    None => None,
                },
                file_id: r.get(18)?,
                art: r.get(19)?,
            })
        })
        .map_err(|e| e.to_string())?
        .map(|row| row.map(|r| (r.id.clone(), r)))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let rows = ids.iter().filter_map(|id| by_id.remove(id)).collect();
    Ok(GamePage { rows, total, total_capped, offset: query.offset, limit })
}

/// Totals for the dashboard and filter menus. Each figure uses an index.
/// Games and total size per platform, for the library's Systems page. Kept out of
/// `library_stats` (which runs on every refresh) because summing sizes walks every game.
pub fn system_summary(connection: &Connection) -> Result<Vec<SystemSummary>, String> {
    // Walk primary files through the role index, then look up release and game by key.
    let mut statement = connection
        .prepare(
            "SELECT g.platform, COUNT(DISTINCT g.id), COALESCE(SUM(gf.payload_bytes), 0)
             FROM game_files gf CROSS JOIN game_releases r CROSS JOIN games g
             WHERE gf.file_role = 'primary' AND gf.excluded_at IS NULL AND r.id = gf.release_id AND g.id = r.game_id
             GROUP BY g.platform ORDER BY g.platform",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| Ok(SystemSummary { platform: r.get(0)?, games: r.get(1)?, bytes: r.get(2)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn library_stats(connection: &Connection, presence: PresenceFn) -> Result<LibraryStats, String> {
    let offline_ids = offline_storage_ids(connection, presence)?;
    let count = |sql: &str, bind_offline: bool| -> Result<i64, String> {
        if bind_offline {
            connection.query_row(sql, [&offline_ids], |r| r.get(0))
        } else {
            connection.query_row(sql, [], |r| r.get(0))
        }
        .map_err(|e| e.to_string())
    };
    const PRIMARY: &str = "FROM file_locations fl JOIN game_files gf ON gf.id = fl.game_file_id WHERE +gf.file_role = 'primary' AND gf.excluded_at IS NULL";
    let files = count("SELECT COUNT(*) FROM game_files WHERE file_role = 'primary' AND excluded_at IS NULL", false)?;
    let left_out = count("SELECT COUNT(*) FROM game_files WHERE file_role = 'primary' AND excluded_at IS NOT NULL", false)?;
    let offline = if offline_ids == "[]" {
        0
    } else {
        count(&format!("SELECT COUNT(*) {PRIMARY} AND fl.storage_location_id IN (SELECT value FROM json_each(?1))"), true)?
    };
    let missing = count(
        &format!("SELECT COUNT(*) {PRIMARY} AND fl.present_status = 'missing' AND fl.storage_location_id NOT IN (SELECT value FROM json_each(?1))"),
        true,
    )?;
    let duplicate_files = count(
        "SELECT COALESCE(SUM(copies), 0) FROM (
             SELECT COUNT(*) AS copies FROM game_files WHERE size_bytes > 0 AND file_role = 'primary' AND excluded_at IS NULL
             GROUP BY size_bytes, filename COLLATE NOCASE, platform_hint HAVING COUNT(*) > 1)",
        false,
    )?;
    let mut statement = connection
        .prepare("SELECT platform, COUNT(*) FROM games GROUP BY platform ORDER BY platform")
        .map_err(|e| e.to_string())?;
    let platforms = statement
        .query_map([], |r| Ok(PlatformCount { platform: r.get(0)?, files: r.get(1)? }))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(LibraryStats { files, online: files - offline - missing, offline, missing, duplicate_files, left_out, platforms })
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

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    fn library(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        for (relative, contents) in files {
            let path = dir.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| crate::volumes::VolumeProbe::unknown()).unwrap();
        (dir, c)
    }

    fn run(c: &Connection, query: GameQuery) -> GamePage {
        query_games(c, &query, &connected).unwrap()
    }

    fn titles(page: &GamePage) -> Vec<&str> {
        page.rows.iter().map(|r| r.title.as_str()).collect()
    }

    fn text(t: &str) -> GameQuery {
        GameQuery { text: Some(t.into()), ..GameQuery::default() }
    }

    const SAMPLE: &[(&str, &[u8])] = &[
        ("snes/Super Puzzle Quest.sfc", b"aaa"),
        ("nes/Puzzle Kid.nes", b"bb"),
        ("gba/Racing Hero.gba", b"c"),
    ];

    #[test]
    fn availability_states() {
        let (dir, c) = library(&[("snes/A.sfc", b"a"), ("snes/B.sfc", b"b")]);
        fs::remove_file(dir.path().join("snes/B.sfc")).unwrap();
        let mut c = c;
        scanner::scan(&mut c, dir.path(), &|_| crate::volumes::VolumeProbe::unknown()).unwrap();

        let page = run(&c, GameQuery::default());
        let state = |t: &str| page.rows.iter().find(|r| r.title == t).unwrap().availability.clone();
        assert_eq!(state("A"), "online");
        assert_eq!(state("B"), "missing");

        // Unplugged drive, or its letter now holds another drive: rows stay, shown offline.
        for p in [Presence::NotConnected, Presence::OtherVolume] {
            let page = query_games(&c, &GameQuery::default(), &|_, _| p).unwrap();
            assert_eq!(page.total, 2);
            assert!(page.rows.iter().all(|r| r.availability == "offline"));
            assert!(page.rows.iter().all(|r| Path::new(&r.path).is_absolute()));
        }

        let drives = list_storage_locations(&c, &|_, _| Presence::OtherVolume).unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].file_count, 2);
        assert_eq!(drives[0].missing_count, 1);
        assert_eq!(drives[0].presence, Presence::OtherVolume);
    }

    #[test]
    fn text_search_matches_inside_titles_and_platforms() {
        let (_dir, c) = library(SAMPLE);
        assert_eq!(run(&c, text("puzzle")).total, 2);
        assert_eq!(run(&c, text("PUZ")).total, 2, "case-insensitive, 3 chars uses the index");
        assert_eq!(titles(&run(&c, text("ra"))), ["Racing Hero"], "short searches fall back to LIKE");
        assert_eq!(titles(&run(&c, text("snes"))), ["Super Puzzle Quest"], "platform names match too");
        assert_eq!(run(&c, text("   ")).total, 3, "blank search means no filter");
        for awkward in ["a\"b", "100%", "x_y", "\\", "\"\"\"", "*", "NEAR(a b)"] {
            assert_eq!(run(&c, text(awkward)).total, 0, "{awkward} is searched literally");
        }
    }

    #[test]
    fn filters_combine() {
        let (dir, mut c) = library(SAMPLE);
        assert_eq!(titles(&run(&c, GameQuery { platform: Some("NES".into()), ..GameQuery::default() })), ["Puzzle Kid"]);
        fs::remove_file(dir.path().join("nes/Puzzle Kid.nes")).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| crate::volumes::VolumeProbe::unknown()).unwrap();
        let missing = run(&c, GameQuery { availability: Some("missing".into()), ..GameQuery::default() });
        assert_eq!(titles(&missing), ["Puzzle Kid"]);
        let combined = run(&c, GameQuery { text: Some("puzzle".into()), availability: Some("online".into()), ..GameQuery::default() });
        assert_eq!(titles(&combined), ["Super Puzzle Quest"]);
        let storage: String = c.query_row("SELECT id FROM storage_locations", [], |r| r.get(0)).unwrap();
        assert_eq!(run(&c, GameQuery { storage_id: Some(storage), ..GameQuery::default() }).total, 3);
        assert_eq!(run(&c, GameQuery { storage_id: Some("nope".into()), ..GameQuery::default() }).total, 0);
        assert!(query_games(&c, &GameQuery { availability: Some("deleted".into()), ..GameQuery::default() }, &connected).is_err());
        assert!(query_games(&c, &GameQuery { sort: Some("random".into()), ..GameQuery::default() }, &connected).is_err());
    }

    #[test]
    fn duplicate_candidates_need_same_platform_name_and_size() {
        let (_dir, c) = library(&[
            ("drive1/snes/Copy Game.sfc", b"same"),
            ("drive2/snes/COPY GAME.sfc", b"same"),       // same name (case-insensitive) + size
            ("drive3/snes/Copy Game.sfc", b"different"),  // different size
            ("drive1/nes/Copy Game.nes", b"same"),        // different platform and name
            ("drive1/snes/Empty.sfc", b""),               // zero-byte files are ignored
            ("drive2/snes/Empty.sfc", b""),
        ]);
        let dups = run(&c, GameQuery { duplicates_only: true, ..GameQuery::default() });
        assert_eq!(dups.total, 2, "{:?}", titles(&dups));
        assert!(dups.rows.iter().all(|r| r.duplicate_count == 2 && r.platform == "SNES"));
        let stats = library_stats(&c, &connected).unwrap();
        assert_eq!(stats.duplicate_files, 2);
        assert_eq!(stats.files, 6);
    }

    #[test]
    fn pages_and_sorting() {
        let files: Vec<(String, Vec<u8>)> = (0..7).map(|i| (format!("snes/Game {i}.sfc"), vec![b'x'; i + 1])).collect();
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
        let (_dir, c) = library(&refs);
        let page = run(&c, GameQuery { offset: 3, limit: Some(3), ..GameQuery::default() });
        assert_eq!(page.total, 7);
        assert_eq!(titles(&page), ["Game 3", "Game 4", "Game 5"]);
        let past_end = run(&c, GameQuery { offset: 50, limit: Some(3), ..GameQuery::default() });
        assert!(past_end.rows.is_empty());
        assert_eq!(past_end.total, 7);
        let by_size = run(&c, GameQuery { sort: Some("size".into()), limit: Some(1), ..GameQuery::default() });
        assert_eq!(titles(&by_size), ["Game 6"]);
        assert_eq!(run(&c, GameQuery { limit: Some(10_000), ..GameQuery::default() }).limit, MAX_PAGE_SIZE);
    }

    #[test]
    fn filtered_counts_are_capped_but_unfiltered_are_exact() {
        let (_dir, c) = library(SAMPLE);
        let page = run(&c, text("puzzle"));
        assert_eq!((page.total, page.total_capped), (2, false));
        assert!(!run(&c, GameQuery::default()).total_capped);
    }

    #[test]
    fn multi_file_games_are_listed_once_with_their_total_size() {
        let (dir, mut c) = library(&[
            ("psx/Saga/Saga.m3u", b"Saga (Disc 1).cue\nSaga (Disc 2).cue\n"),
            ("psx/Saga/Saga (Disc 1).cue", b"FILE \"d1.bin\" BINARY\n"),
            ("psx/Saga/Saga (Disc 2).cue", b"FILE \"d2.bin\" BINARY\n"),
            ("psx/Saga/d1.bin", &[0u8; 4000]),
            ("psx/Saga/d2.bin", &[0u8; 6000]),
            ("snes/Plain.sfc", b"rom"),
        ]);
        let page = run(&c, GameQuery::default());
        assert_eq!(titles(&page), ["Plain", "Saga"], "discs and tracks are not separate rows");
        let saga = &page.rows[1];
        assert_eq!(saga.disc_count, 2);
        assert!(saga.file_size > 10_000, "{}", saga.file_size);
        assert!(!saga.incomplete);
        assert_eq!(library_stats(&c, &connected).unwrap().files, 2);
        let systems = system_summary(&c).unwrap();
        let psx = systems.iter().find(|p| p.platform == "PlayStation").unwrap();
        assert_eq!(psx.games, 1, "a multi-disc game counts once per system");
        assert_eq!(psx.bytes, saga.file_size, "system totals include discs and tracks");
        assert_eq!(systems.iter().map(|s| s.games).sum::<i64>(), 2);
        let by_size = run(&c, GameQuery { sort: Some("size".into()), limit: Some(1), ..GameQuery::default() });
        assert_eq!(titles(&by_size), ["Saga"], "size sort uses the game's total size");

        fs::remove_file(dir.path().join("psx/Saga/d2.bin")).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| crate::volumes::VolumeProbe::unknown()).unwrap();
        let saga = run(&c, text("saga")).rows.remove(0);
        assert!(saga.incomplete, "a missing track marks the game incomplete");
        assert_eq!(saga.availability, "online", "the game's main file is still there");
    }

    #[test]
    fn stats_summarize_the_library() {
        let (dir, mut c) = library(SAMPLE);
        fs::remove_file(dir.path().join("gba/Racing Hero.gba")).unwrap();
        scanner::scan(&mut c, dir.path(), &|_| crate::volumes::VolumeProbe::unknown()).unwrap();
        let stats = library_stats(&c, &connected).unwrap();
        assert_eq!((stats.files, stats.online, stats.missing, stats.offline), (3, 2, 1, 0));
        let platforms: Vec<&str> = stats.platforms.iter().map(|p| p.platform.as_str()).collect();
        assert_eq!(platforms, ["GBA", "NES", "SNES"]);
        let offline = library_stats(&c, &|_, _| Presence::NotConnected).unwrap();
        assert_eq!((offline.online, offline.offline), (0, 3));
    }

    /// Performance baseline (docs/15). Run with:
    /// `cargo test --release benchmark_100k -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn benchmark_100k() {
        use std::time::Instant;
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let started = Instant::now();
        let tx = c.transaction().unwrap();
        let storage = db::upsert_storage_location(&tx, "C:\\", "local").unwrap();
        let platforms = ["NES", "SNES", "Genesis", "GBA", "N64", "PlayStation"];
        for i in 0..100_000u32 {
            // Every 100th file is a twin of the previous one: same platform, name and size.
            let key = if i % 100 == 1 { i - 1 } else { i };
            let platform = platforms[(key % 6) as usize];
            let name = format!("Game {key:06}.rom");
            let title = format!("Benchmark Adventure {i:06} {}", if i % 7 == 0 { "Puzzle" } else { "Quest" });
            let file = db::insert_game_file(&tx, &title, platform, &name, "rom", 1024 + i64::from(key), None, "folder").unwrap();
            tx.execute(
                "INSERT INTO file_locations (id, game_file_id, storage_location_id, relative_path) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![db::new_id(), file, storage, format!("roms/{platform}/{i}/{name}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        eprintln!("insert 100k rows: {:?}", started.elapsed());

        let cases: Vec<(&str, GameQuery)> = vec![
            ("first page, no filter", GameQuery::default()),
            ("page 500 (offset 50,000)", GameQuery { offset: 50_000, ..GameQuery::default() }),
            ("text 'puzzle'", text("puzzle")),
            ("text 'adventure 0999'", text("adventure 0999")),
            ("text 'ad' (LIKE)", text("ad")),
            ("platform SNES", GameQuery { platform: Some("SNES".into()), ..GameQuery::default() }),
            ("duplicates only", GameQuery { duplicates_only: true, ..GameQuery::default() }),
            ("sort by size", GameQuery { sort: Some("size".into()), ..GameQuery::default() }),
        ];
        for (name, query) in cases {
            let t = Instant::now();
            let page = query_games(&c, &query, &connected).unwrap();
            eprintln!("{name:<28} total={:>6}{} {:?}", page.total, if page.total_capped { "+" } else { " " }, t.elapsed());
        }
        let t = Instant::now();
        let stats = library_stats(&c, &connected).unwrap();
        eprintln!("{:<28} files={:>6}  {:?}", "library_stats", stats.files, t.elapsed());
        let t = std::time::Instant::now();
        let systems = system_summary(&c).unwrap();
        eprintln!("{:<28} systems={:>4}  {:?}", "system_summary", systems.len(), t.elapsed());
        assert_eq!(stats.files, 100_000);
        assert_eq!(stats.duplicate_files, 2_000);
    }

    #[test]
    fn letters_filter_titles_and_hash_means_digits_or_symbols() {
        let (_dir, c) = library(&[("nes/Metroid (USA).nes", b"m"), ("nes/mario (USA).nes", b"m2"), ("nes/Zelda (USA).nes", b"z"), ("nes/1942 (USA).nes", b"1"), ("nes/'89 Uranai (Japan).nes", b"8")]);
        let pick = |l: &str| -> Vec<String> { titles(&run(&c, GameQuery { letter: Some(l.into()), ..GameQuery::default() })).into_iter().map(|t| t.to_string()).collect() };
        assert_eq!(pick("M"), ["mario (USA)", "Metroid (USA)"], "case doesn't matter");
        assert_eq!(pick("z"), ["Zelda (USA)"]);
        assert_eq!(pick("#"), ["'89 Uranai (Japan)", "1942 (USA)"]);
        assert!(query_games(&c, &GameQuery { letter: Some("AB".into()), ..GameQuery::default() }, &connected).is_err());
    }
}
