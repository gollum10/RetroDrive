//! Reference catalog (owner's game catalog import; docs/07 "Reference catalog").
//!
//! The user imports a spreadsheet (`.xlsx`) or `.csv` that lists games with their
//! details: title, system, year, genre, publisher, developer and, ideally, the
//! exact No-Intro or MAME name. RetroDrive links library games to those entries so
//! it can show clean titles and details. Rules:
//! - Only names and details are imported. **Columns about art are never read**,
//!   and no images are imported (owner decision; artwork rights, docs/16 §7).
//! - The user's files are only read; everything lives in the local catalog.
//! - Reference data is kept separate from the library's own titles, so removing
//!   the catalog (or importing a new one) never changes the library itself.
//! - Matching: first the exact No-Intro/MAME name (and MAME clone sets), then the
//!   title within the same system. Ambiguous titles are left unmatched.
//! - A file with only `Game ID` and `Genre` columns is a genre supplement: it fills
//!   genres that the imported catalog left empty.

use crate::db;
use calamine::{open_workbook, Data, Reader, Xlsx};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// One row of a reference catalog, after column mapping.
#[derive(Debug, Default, Clone, PartialEq)]
struct Entry {
    entry_key: Option<String>,
    title: String,
    platform_label: String,
    year: Option<i64>,
    genre: Option<String>,
    publisher: Option<String>,
    developer: Option<String>,
    source_id: Option<String>,
    source_name: Option<String>,
    variants: Option<String>,
    regions: Option<String>,
}

#[derive(Debug, Serialize, Default, Clone, PartialEq, Eq)]
pub struct MatchCounts {
    /// Exact No-Intro / MAME name.
    pub by_name: i64,
    /// MAME clone or regional set listed as a variant.
    pub by_variant: i64,
    /// Same title on the same system.
    pub by_title: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlatformMatch {
    pub platform: String,
    pub games: i64,
    pub matched: i64,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    /// `catalog` or `genres`.
    pub kind: &'static str,
    pub file_name: String,
    pub entries: i64,
    pub genres_added: i64,
    /// Rows skipped because they had no title or system.
    pub skipped_rows: i64,
    /// Columns that were present but deliberately not imported (e.g. art).
    pub ignored_columns: Vec<String>,
    pub matched: MatchCounts,
    pub platforms: Vec<PlatformMatch>,
}

#[derive(Debug, Serialize)]
pub struct CatalogStatus {
    pub name: String,
    pub file_name: String,
    pub imported_at: String,
    pub entries: i64,
    pub genres_added: i64,
    pub matched: MatchCounts,
    pub platforms: Vec<PlatformMatch>,
}

/// Catalog system names that RetroDrive calls something else.
fn retrodrive_platform(label: &str) -> String {
    match label.trim() {
        "Game Boy Advance" => "GBA",
        "Sega Genesis" | "Sega Mega Drive" | "Mega Drive" => "Genesis",
        "Nintendo 64" => "N64",
        "Sega Game Gear" => "Game Gear",
        "Sega Master System" => "Master System",
        "Nintendo Entertainment System" | "Famicom" => "NES",
        "Super Nintendo" | "Super Nintendo Entertainment System" | "Super Famicom" => "SNES",
        "Sony PlayStation" | "PS1" => "PlayStation",
        "Sony PlayStation 2" | "PS2" => "PlayStation 2",
        other => other,
    }
    .to_string()
}

/// Lower-case, single-spaced exact key (for No-Intro / MAME names).
fn name_key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Loose title key: no tags in brackets, "Title, The" == "The Title", letters and
/// digits only. Used only within one system.
fn title_key(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let mut lower = out.trim().to_lowercase();
    for article in ["the", "a", "an"] {
        let suffix = format!(", {article}");
        if lower.ends_with(&suffix) {
            lower = format!("{article} {}", &lower[..lower.len() - suffix.len()]);
            break;
        }
    }
    lower.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Which input column feeds which field. `None` = not present.
#[derive(Debug, Default)]
struct Columns {
    entry_key: Option<usize>,
    title: Option<usize>,
    platform: Option<usize>,
    year: Option<usize>,
    genre: Option<usize>,
    publisher: Option<usize>,
    developer: Option<usize>,
    source_id: Option<usize>,
    source_name: Option<usize>,
    variants: Option<usize>,
    regions: Option<usize>,
    ignored: Vec<String>,
}

fn map_columns(headers: &[String]) -> Columns {
    let mut c = Columns::default();
    for (i, raw) in headers.iter().enumerate() {
        let h = raw.trim().to_lowercase();
        let set = |slot: &mut Option<usize>| {
            if slot.is_none() {
                *slot = Some(i);
            }
        };
        // Art columns are never imported (owner decision).
        if h == "art" || h.starts_with("art ") || h.contains("box art") || h.contains("boxart") || h.contains("image") || h.contains("thumbnail") {
            c.ignored.push(raw.trim().to_string());
        } else if h == "game id" || h == "id" {
            set(&mut c.entry_key);
        } else if h == "title" || h == "name" || h == "game" {
            set(&mut c.title);
        } else if h.contains("platform") || h == "system" || h == "console" {
            set(&mut c.platform);
        } else if h == "year" || h.contains("release year") {
            set(&mut c.year);
        } else if h == "genre" || h.starts_with("genre") {
            set(&mut c.genre);
        } else if h.starts_with("publisher") {
            set(&mut c.publisher);
        } else if h.starts_with("developer") {
            set(&mut c.developer);
        } else if h.starts_with("source id") || (h.contains("no-intro name") && !h.contains("full")) || h.contains("rom name") {
            set(&mut c.source_id);
        } else if h.starts_with("source full name") || h.contains("description") {
            set(&mut c.source_name);
        } else if h.starts_with("variant") && !h.contains("count") {
            set(&mut c.variants);
        } else if h.starts_with("region") {
            set(&mut c.regions);
        }
    }
    c
}

fn cell_text(row: &[String], index: Option<usize>) -> Option<String> {
    let value = row.get(index?)?.trim();
    (!value.is_empty() && !value.eq_ignore_ascii_case("none")).then(|| value.to_string())
}

/// Read the first sheet (xlsx) or the file (csv) as header + rows of text.
fn read_table(path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let friendly = |e: std::io::Error| -> String {
        if e.raw_os_error() == Some(32) || e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("{} is open in another program (such as Excel). Close it there and try again.", path.display())
        } else {
            format!("Couldn't open {}: {e}", path.display())
        }
    };
    // Open first, so a locked file gets a clear message instead of a parser error.
    std::fs::File::open(path).map_err(friendly)?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "xlsx" | "xlsm" => {
            let mut workbook: Xlsx<_> = open_workbook(path).map_err(|e| format!("Couldn't read the spreadsheet: {e}"))?;
            let names = workbook.sheet_names().to_vec();
            // Prefer the first sheet that has a title or genre column.
            for name in names {
                let Ok(range) = workbook.worksheet_range(&name) else { continue };
                let mut rows = range.rows().map(|r| r.iter().map(cell_to_string).collect::<Vec<_>>());
                let Some(header) = rows.next() else { continue };
                let columns = map_columns(&header);
                if columns.title.is_some() || columns.genre.is_some() {
                    return Ok((header, rows.collect()));
                }
            }
            Err("No sheet in this workbook has a Title column.".into())
        }
        "csv" => {
            let mut reader = csv::ReaderBuilder::new().flexible(true).from_path(path).map_err(|e| format!("Couldn't read the CSV file: {e}"))?;
            let header = reader.headers().map_err(|e| e.to_string())?.iter().map(|h| h.trim_start_matches('\u{feff}').to_string()).collect();
            let rows = reader.records().filter_map(Result::ok).map(|r| r.iter().map(str::to_string).collect()).collect();
            Ok((header, rows))
        }
        other => Err(format!("RetroDrive can import .xlsx or .csv catalogs, not .{other} files.")),
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        Data::Int(i) => i.to_string(),
        other => other.to_string(),
    }
}

/// Import a catalog (replacing any previous one) or a genre supplement, then match.
pub fn import(connection: &Connection, path: &Path) -> Result<ImportSummary, String> {
    let (header, rows) = read_table(path)?;
    let columns = map_columns(&header);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("catalog").to_string();

    // A genre supplement: Game ID + Genre, no titles.
    if columns.title.is_none() {
        let (Some(key_col), Some(genre_col)) = (columns.entry_key, columns.genre) else {
            return Err("This file needs Title and Platform columns (a catalog), or Game ID and Genre columns (a genre supplement).".into());
        };
        let catalog: Option<String> = connection
            .query_row("SELECT id FROM reference_catalogs ORDER BY imported_at DESC LIMIT 1", [], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(catalog) = catalog else {
            return Err("Import the main catalog first; this file only adds genres to it.".into());
        };
        let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut added = 0i64;
        {
            let mut update = tx
                .prepare("UPDATE reference_entries SET genre = ?3 WHERE catalog_id = ?1 AND entry_key = ?2 AND (genre IS NULL OR genre = '')")
                .map_err(|e| e.to_string())?;
            for row in &rows {
                if let (Some(key), Some(genre)) = (cell_text(row, Some(key_col)), cell_text(row, Some(genre_col))) {
                    added += update.execute(params![catalog, key, genre]).map_err(|e| e.to_string())? as i64;
                }
            }
        }
        tx.execute("UPDATE reference_catalogs SET genres_added = genres_added + ?2 WHERE id = ?1", params![catalog, added]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        let status = status(connection)?;
        return Ok(ImportSummary {
            kind: "genres",
            file_name,
            entries: status.as_ref().map(|s| s.entries).unwrap_or(0),
            genres_added: added,
            skipped_rows: 0,
            ignored_columns: columns.ignored,
            matched: status.as_ref().map(|s| s.matched.clone()).unwrap_or_default(),
            platforms: status.map(|s| s.platforms).unwrap_or_default(),
        });
    }

    if columns.platform.is_none() {
        return Err("The catalog needs a Platform (or System) column so games can be matched per system.".into());
    }
    let mut entries = Vec::with_capacity(rows.len());
    let mut skipped = 0i64;
    for row in &rows {
        let (Some(title), Some(platform_label)) = (cell_text(row, columns.title), cell_text(row, columns.platform)) else {
            skipped += 1;
            continue;
        };
        entries.push(Entry {
            entry_key: cell_text(row, columns.entry_key),
            title,
            platform_label,
            year: cell_text(row, columns.year).and_then(|y| y.parse::<i64>().ok()).filter(|y| (1950..=2100).contains(y)),
            genre: cell_text(row, columns.genre),
            publisher: cell_text(row, columns.publisher),
            developer: cell_text(row, columns.developer),
            source_id: cell_text(row, columns.source_id),
            source_name: cell_text(row, columns.source_name),
            variants: cell_text(row, columns.variants),
            regions: cell_text(row, columns.regions),
        });
    }
    if entries.is_empty() {
        return Err("No rows with both a title and a system were found.".into());
    }

    // One catalog at a time: a new import replaces the previous one (links included).
    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM reference_catalogs", []).map_err(|e| e.to_string())?;
    let catalog = db::new_id();
    let name = Path::new(&file_name).file_stem().and_then(|s| s.to_str()).unwrap_or(&file_name).replace('_', " ");
    tx.execute(
        "INSERT INTO reference_catalogs (id, name, file_name, entries) VALUES (?1, ?2, ?3, ?4)",
        params![catalog, name, file_name, entries.len() as i64],
    )
    .map_err(|e| e.to_string())?;
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO reference_entries (catalog_id, entry_key, title, platform, platform_label, year, genre, publisher, developer, source_id, source_name, variants, regions)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .map_err(|e| e.to_string())?;
        for e in &entries {
            insert
                .execute(params![
                    catalog,
                    e.entry_key,
                    e.title,
                    retrodrive_platform(&e.platform_label),
                    e.platform_label,
                    e.year,
                    e.genre,
                    e.publisher,
                    e.developer,
                    e.source_id,
                    e.source_name,
                    e.variants,
                    e.regions
                ])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    match_games(connection, true)?;
    let status = status(connection)?.ok_or("The catalog wasn't saved")?;
    Ok(ImportSummary {
        kind: "catalog",
        file_name,
        entries: status.entries,
        genres_added: 0,
        skipped_rows: skipped,
        ignored_columns: columns.ignored,
        matched: status.matched,
        platforms: status.platforms,
    })
}

/// Link library games to catalog entries. `all` rematches everything; otherwise
/// only games without a link (e.g. after a scan). Returns how many links were added.
pub fn match_games(connection: &Connection, all: bool) -> Result<i64, String> {
    let has_catalog: bool = connection.query_row("SELECT EXISTS (SELECT 1 FROM reference_entries)", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !has_catalog {
        return Ok(0);
    }
    // Index the catalog in memory: exact names, variants and (system, title).
    let mut by_name: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    let mut by_variant: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    let mut by_title: HashMap<(String, String), Vec<i64>> = HashMap::new();
    {
        let mut statement = connection
            .prepare("SELECT id, platform, title, source_id, source_name, variants FROM reference_entries")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, platform, title, source_id, source_name, variants) = row.map_err(|e| e.to_string())?;
            for name in [source_id, source_name].into_iter().flatten() {
                let list = by_name.entry(name_key(&name)).or_default();
                if !list.iter().any(|(e, _)| *e == id) {
                    list.push((id, platform.clone()));
                }
            }
            // Clone / regional set names are only meaningful for arcade sets.
            if platform == "Arcade" {
                for variant in variants.iter().flat_map(|v| v.split(',')).map(str::trim).filter(|v| !v.is_empty() && !v.starts_with('(')) {
                    by_variant.entry(name_key(variant)).or_default().push((id, platform.clone()));
                }
            }
            by_title.entry((platform, title_key(&title))).or_default().push(id);
        }
    }

    let games: Vec<(String, String, String)> = {
        let filter = if all { "" } else { " AND NOT EXISTS (SELECT 1 FROM game_reference gr WHERE gr.game_id = g.id)" };
        let mut statement = connection
            .prepare(&format!(
                "SELECT g.id, g.platform, gf.filename FROM games g JOIN game_releases r ON r.game_id = g.id
                 JOIN game_files gf ON gf.release_id = r.id AND gf.file_role = 'primary' WHERE 1 = 1{filter}"
            ))
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };

    // Prefer an entry on the game's own system; an unclassified game accepts a
    // unique match on any system.
    let pick = |candidates: Option<&Vec<(i64, String)>>, platform: &str| -> Option<i64> {
        let list = candidates?;
        let same: Vec<i64> = list.iter().filter(|(_, p)| p == platform).map(|(id, _)| *id).collect();
        match (same.len(), platform == "Unclassified", list.len()) {
            (1, _, _) => Some(same[0]),
            (0, true, 1) => Some(list[0].0),
            _ => None,
        }
    };

    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    if all {
        tx.execute("DELETE FROM game_reference", []).map_err(|e| e.to_string())?;
    }
    let mut added = 0i64;
    {
        let mut insert = tx
            .prepare("INSERT OR REPLACE INTO game_reference (game_id, entry_id, method) VALUES (?1, ?2, ?3)")
            .map_err(|e| e.to_string())?;
        let mut seen = std::collections::HashSet::new();
        for (game, platform, filename) in &games {
            if !seen.insert(game.clone()) {
                continue; // several primary files of one game: the first decides
            }
            let stem = filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename);
            let key = name_key(stem);
            let found = pick(by_name.get(&key), platform)
                .map(|id| (id, "name"))
                .or_else(|| pick(by_variant.get(&key), platform).map(|id| (id, "variant")))
                .or_else(|| match by_title.get(&(platform.clone(), title_key(stem))) {
                    Some(ids) if ids.len() == 1 => Some((ids[0], "title")),
                    _ => None,
                });
            if let Some((entry, method)) = found {
                insert.execute(params![game, entry, method]).map_err(|e| e.to_string())?;
                added += 1;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(added)
}

/// The imported catalog and how much of the library it matches, or `None`.
pub fn status(connection: &Connection) -> Result<Option<CatalogStatus>, String> {
    let row: Option<(String, String, String, i64, i64)> = connection
        .query_row("SELECT name, file_name, imported_at, entries, genres_added FROM reference_catalogs ORDER BY imported_at DESC LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((name, file_name, imported_at, entries, genres_added)) = row else { return Ok(None) };
    let mut matched = MatchCounts::default();
    {
        let mut statement = connection.prepare("SELECT method, COUNT(*) FROM game_reference GROUP BY method").map_err(|e| e.to_string())?;
        let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).map_err(|e| e.to_string())?;
        for row in rows {
            let (method, n) = row.map_err(|e| e.to_string())?;
            match method.as_str() {
                "name" => matched.by_name = n,
                "variant" => matched.by_variant = n,
                _ => matched.by_title = n,
            }
        }
    }
    let mut per: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    {
        let mut statement = connection
            .prepare("SELECT g.platform, COUNT(*), COUNT(gr.game_id) FROM games g LEFT JOIN game_reference gr ON gr.game_id = g.id GROUP BY g.platform")
            .map_err(|e| e.to_string())?;
        let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))).map_err(|e| e.to_string())?;
        for row in rows {
            let (platform, games, linked) = row.map_err(|e| e.to_string())?;
            per.insert(platform, (games, linked));
        }
    }
    let mut platforms: Vec<PlatformMatch> = per.into_iter().map(|(platform, (games, matched))| PlatformMatch { platform, games, matched }).collect();
    platforms.sort_by_key(|p| std::cmp::Reverse(p.games));
    Ok(Some(CatalogStatus { name, file_name, imported_at, entries, genres_added, matched, platforms }))
}

/// Remove the reference catalog and all links. The library itself is unchanged.
pub fn remove(connection: &Connection) -> Result<bool, String> {
    connection.execute("DELETE FROM reference_catalogs", []).map(|n| n > 0).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    use crate::volumes::VolumeProbe;
    use std::fs;

    fn unknown(_: &str) -> VolumeProbe {
        VolumeProbe::unknown()
    }

    fn library(files: &[&str]) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        for f in files {
            let p = dir.path().join("library").join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, f.as_bytes()).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &dir.path().join("library"), &unknown).unwrap();
        (dir, c)
    }

    const CSV: &str = "\u{feff}Game ID,Title,Console or arcade (Platform),Year,Genre,Publisher / Manufacturer,Developer,Source ID (MAME set / No-Intro name),Source full name (MAME description / No-Intro name),Variants (clone / regional sets),Art file,Art source\n\
ARC-88games,'88 Games,Arcade,1988,Sports,Konami,,88games,'88 Games,\"hypsptsp, konami88\",art/ARC/ARC-88games.jpg,thumbs\n\
NES-kid,Kid Icarus,NES,1986,Platform,Nintendo,Nintendo,Kid Icarus (USA),Kid Icarus (USA),,art/NES/kid.jpg,thumbs\n\
SNES-sm,\"Super Metroid\",SNES,1994,,Nintendo,Nintendo,,,,,\n\
GEN-mono1,Monopoly,Sega Genesis,1992,Board,Parker,,,,,,\n\
GEN-mono2,Monopoly,Sega Genesis,1994,Board,Parker,,,,,,\n\
GBA-x,Golden Sun,Game Boy Advance,2001,RPG,Nintendo,Camelot,\"Golden Sun (USA, Europe)\",,,,\n\
,,NES,,,,,,,,,\n";

    fn write_csv(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    fn link(c: &Connection, file: &str) -> Option<(String, String, Option<String>)> {
        c.query_row(
            "SELECT re.title, gr.method, re.genre FROM game_files gf JOIN game_releases r ON r.id = gf.release_id
             JOIN game_reference gr ON gr.game_id = r.game_id JOIN reference_entries re ON re.id = gr.entry_id WHERE gf.filename = ?1",
            [file],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .unwrap()
    }

    #[test]
    fn imports_a_catalog_and_matches_by_name_variant_and_title() {
        let (dir, c) = library(&[
            "arcade/88games.zip",
            "arcade/konami88.zip",
            "nes/Kid Icarus (USA).zip",
            "snes/Super Metroid (Japan, USA) (En).sfc",
            "megadrive/Monopoly (USA).md",
            "gba/Golden Sun (USA, Europe).gba",
            "gba/Unknown Homebrew.gba",
        ]);
        let before: Vec<String> = c.prepare("SELECT canonical_title FROM games ORDER BY 1").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        let summary = import(&c, &write_csv(dir.path(), "uncorked_games_catalog.csv", CSV)).unwrap();
        assert_eq!(summary.kind, "catalog");
        assert_eq!(summary.entries, 6);
        assert_eq!(summary.skipped_rows, 1, "a row without title or system");
        assert_eq!(summary.ignored_columns, ["Art file", "Art source"], "art columns are never imported");

        assert_eq!(link(&c, "88games.zip").map(|l| (l.0, l.1)), Some(("'88 Games".into(), "name".into())));
        assert_eq!(link(&c, "konami88.zip").map(|l| l.1), Some("variant".into()), "a MAME clone set");
        assert_eq!(link(&c, "Kid Icarus (USA).zip").map(|l| l.1), Some("name".into()));
        assert_eq!(link(&c, "Super Metroid (Japan, USA) (En).sfc").map(|l| l.1), Some("title".into()), "same title on the same system");
        assert_eq!(link(&c, "Golden Sun (USA, Europe).gba").map(|l| (l.1, l.2)), Some(("name".into(), Some("RPG".into()))), "system names are mapped (Game Boy Advance = GBA)");
        assert_eq!(link(&c, "Monopoly (USA).md"), None, "two Monopoly entries on Genesis: ambiguous, so not guessed");
        assert_eq!(link(&c, "Unknown Homebrew.gba"), None);
        assert_eq!(summary.matched, MatchCounts { by_name: 3, by_variant: 1, by_title: 1 });

        let art: i64 = c.query_row("SELECT COUNT(*) FROM pragma_table_info('reference_entries') WHERE name LIKE '%art%'", [], |r| r.get(0)).unwrap();
        assert_eq!(art, 0, "the schema has nowhere to store art");
        let after: Vec<String> = c.prepare("SELECT canonical_title FROM games ORDER BY 1").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(before, after, "the library's own titles are never changed");

        // Genre supplement fills empty genres only.
        let genres = write_csv(dir.path(), "game_genres_supplement.csv", "\u{feff}Game ID,Genre,Source\nSNES-sm,Action-adventure,x\nNES-kid,Overwrite attempt,x\n");
        let g = import(&c, &genres).unwrap();
        assert_eq!((g.kind, g.genres_added), ("genres", 1));
        assert_eq!(link(&c, "Super Metroid (Japan, USA) (En).sfc").unwrap().2.as_deref(), Some("Action-adventure"));
        assert_eq!(link(&c, "Kid Icarus (USA).zip").unwrap().2.as_deref(), Some("Platform"), "existing genres are kept");

        // New games are matched after a scan; removing the catalog removes only links.
        fs::write(dir.path().join("library/nes/Kid Icarus (USA) (Rev 1).zip"), b"rev").unwrap();
        let mut c = c;
        scanner::scan(&mut c, &dir.path().join("library"), &unknown).unwrap();
        assert_eq!(match_games(&c, false).unwrap(), 1, "only the new game is matched");
        assert_eq!(link(&c, "Kid Icarus (USA) (Rev 1).zip").map(|l| l.1), Some("title".into()));
        assert!(remove(&c).unwrap());
        assert_eq!(status(&c).unwrap().map(|s| s.entries), None);
        let links: i64 = c.query_row("SELECT COUNT(*) FROM game_reference", [], |r| r.get(0)).unwrap();
        assert_eq!(links, 0);
    }

    #[test]
    fn unhelpful_files_get_clear_errors() {
        let (dir, c) = library(&["nes/Kid.nes"]);
        let genres_first = write_csv(dir.path(), "g.csv", "Game ID,Genre\nX,Y\n");
        assert!(import(&c, &genres_first).unwrap_err().contains("main catalog first"));
        let no_platform = write_csv(dir.path(), "p.csv", "Title,Year\nKid,1986\n");
        assert!(import(&c, &no_platform).unwrap_err().contains("Platform"));
        let nothing = write_csv(dir.path(), "n.csv", "Foo,Bar\n1,2\n");
        assert!(import(&c, &nothing).unwrap_err().contains("Title and Platform"));
        let txt = write_csv(dir.path(), "x.txt", "hi");
        assert!(import(&c, &txt).unwrap_err().contains(".xlsx or .csv"));
    }

    #[test]
    fn reads_xlsx_workbooks() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/reference_small.xlsx");
        let (_dir, c) = library(&["arcade/88games.zip", "nes/Kid Icarus (USA).zip"]);
        let summary = import(&c, &path).unwrap();
        assert_eq!(summary.entries, 2);
        assert_eq!(summary.matched.by_name, 2);
        assert!(summary.ignored_columns.iter().any(|c| c == "Art file"));
        let year: Option<i64> = c.query_row("SELECT year FROM reference_entries WHERE title = '''88 Games'", [], |r| r.get(0)).unwrap();
        assert_eq!(year, Some(1988), "numeric cells come through as whole numbers");
    }

    #[test]
    fn title_keys_ignore_tags_articles_and_punctuation() {
        assert_eq!(title_key("Pyramid, The (USA) [!]"), "the pyramid");
        assert_eq!(title_key("Jeopardy! 25th Anniversary Edition (USA)"), "jeopardy 25th anniversary edition");
        assert_eq!(title_key("Super Metroid"), title_key("Super Metroid (Japan, USA) (En)"));
        assert_eq!(name_key("  Kid  Icarus (USA) "), "kid icarus (usa)");
    }
}
