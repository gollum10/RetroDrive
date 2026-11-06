//! Game art and details in RetroBat's game lists (owner request 2026-11-07: "apply
//! those [images] to RetroBat"; docs/18 §17).
//!
//! RetroBat's menus show a game's pictures and details from
//! `roms\<system>\gamelist.xml` (`<game><path>./Game.zip</path><name>…</name>
//! <image>./images/Game-image.png</image>…</game>`; RetroBat 8.3.1.1 creates these
//! lists itself with names and regions). For games RetroDrive copied to the drive,
//! this adds the user's own images (copied into `roms\<system>\images\` as new files)
//! and details from the user's reference catalog:
//!
//! - existing entries only get what they're missing (RetroBat's own art and details
//!   are kept; a name that is just the file name is replaced by the catalog title);
//! - games without an entry get a new one;
//! - every other entry and line stays as it is; each list is journaled and backed up
//!   (`customizer::write_file`, which also creates a missing list), and the whole
//!   step is undone at once (recorded like a design application).
//!
//! Refused while RetroBat runs: it saves its game lists when it closes.

use crate::copier::{self, TEMP_MARKER};
use crate::customizer::{self, ChangeRow, PresenceFn, RunningFn};
use crate::volumes::Presence;
use crate::{db, paths};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One game RetroDrive put on the drive, with what can be added for it.
#[derive(Debug, Clone)]
struct Target {
    system: String,
    /// Path inside the system folder, `/`-separated (e.g. `Game.zip`, `Saga/Saga.m3u`).
    rel: String,
    image: Option<PathBuf>,
    thumbnail: Option<PathBuf>,
    marquee: Option<PathBuf>,
    details: Details,
}

#[derive(Debug, Clone, Default)]
struct Details {
    title: Option<String>,
    year: Option<i64>,
    genre: Option<String>,
    developer: Option<String>,
    publisher: Option<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SystemArt {
    pub system: String,
    /// Games whose entry gets something (art or details).
    pub games_updated: usize,
    /// New entries for games the list didn't have.
    pub games_added: usize,
    pub images: usize,
    /// Games whose art RetroBat already shows (kept).
    pub kept: usize,
}

#[derive(Debug, Serialize)]
pub struct ArtPreview {
    pub systems: Vec<SystemArt>,
    pub images: usize,
    pub image_bytes: u64,
    pub games_updated: usize,
    /// Games RetroDrive copied whose art couldn't be read (drive with the images not connected).
    pub art_unavailable: usize,
    /// SHA-256 of each list as read (`none` when it doesn't exist yet).
    pub list_shas: BTreeMap<String, String>,
    pub nothing: bool,
}

// ---------- reading a gamelist ----------

#[derive(Debug, Clone)]
struct Entry {
    /// Normalised path (`game.zip`, lower case, no `./`).
    key: String,
    /// Child tag names present.
    tags: Vec<String>,
    /// The `<name>` value and its byte range, if any.
    name: Option<(String, usize, usize)>,
    /// Byte position of `</game>`.
    close: usize,
    /// Indentation of child lines.
    indent: String,
}

fn norm(path: &str) -> String {
    path.trim().trim_start_matches("./").replace('\\', "/").to_lowercase()
}

/// `<game>` entries with their children, from a gamelist's text.
fn parse(text: &str) -> Result<Vec<Entry>, String> {
    customizer::well_formed(text).map_err(|e| format!("gamelist.xml is {e}"))?;
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("<game") {
        let start = i + off;
        let after = text.as_bytes().get(start + 5).copied();
        if !matches!(after, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
            i = start + 5;
            continue;
        }
        let Some(open_end) = text[start..].find('>').map(|p| start + p + 1) else { break };
        if text[start..open_end].ends_with("/>") {
            i = open_end;
            continue;
        }
        let Some(close) = text[open_end..].find("</game>").map(|p| open_end + p) else { break };
        let body = &text[open_end..close];
        let mut tags = Vec::new();
        let mut path = None;
        let mut name = None;
        let mut indent = None;
        let mut j = 0;
        while let Some(o) = body[j..].find('<') {
            let t = j + o;
            if body[t..].starts_with("<!--") {
                j = body[t..].find("-->").map(|p| t + p + 3).unwrap_or(body.len());
                continue;
            }
            let Some(gt) = body[t..].find('>').map(|p| t + p) else { break };
            let tag: String = body[t + 1..gt].split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_string();
            if tag.is_empty() || tag.starts_with('/') {
                j = gt + 1;
                continue;
            }
            if indent.is_none() {
                let line_start = body[..t].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let pre = &body[line_start..t];
                if pre.trim().is_empty() && line_start > 0 {
                    indent = Some(pre.to_string());
                }
            }
            let close_tag = format!("</{tag}>");
            let (value, end) = if body[t..gt].ends_with('/') {
                (String::new(), gt + 1)
            } else if let Some(c) = body[gt + 1..].find(&close_tag).map(|p| gt + 1 + p) {
                (body[gt + 1..c].to_string(), c + close_tag.len())
            } else {
                (String::new(), gt + 1)
            };
            if tag == "path" {
                path = Some(customizer::unescape(value.trim()));
            }
            if tag == "name" {
                name = Some((customizer::unescape(value.trim()), open_end + gt + 1, open_end + gt + 1 + value.len()));
            }
            tags.push(tag);
            j = end;
        }
        if let Some(path) = path {
            out.push(Entry { key: norm(&path), tags, name, close, indent: indent.unwrap_or_else(|| "\t\t".into()) });
        }
        i = close + 7;
    }
    Ok(out)
}

fn has(entry: &Entry, tag: &str) -> bool {
    entry.tags.iter().any(|t| t == tag)
}

fn image_name(rel: &str, suffix: &str, source: &Path) -> String {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let safe: String = stem.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    format!("{safe}-{suffix}.{ext}")
}

// ---------- what RetroDrive knows ----------

/// A game's main picture, thumbnail and logo (when readable), and whether it has art that couldn't be read.
type Media = (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>, bool);

fn media_for(connection: &Connection, file_id: &str, presence: PresenceFn, online: &mut HashMap<String, bool>) -> Result<Media, String> {
    let mut statement = connection
        .prepare_cached(
            "SELECT gm.kind, gm.source, s.id, s.root_path, s.stable_id, gm.relative_path
             FROM game_media gm JOIN storage_locations s ON s.id = gm.storage_location_id WHERE gm.game_file_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String, String, Option<String>, String)> = statement
        .query_map([file_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let any = !rows.is_empty();
    let mut found: Vec<(i32, String, PathBuf)> = Vec::new();
    for (kind, source, storage, root, stable, relative) in rows {
        let up = *online.entry(storage).or_insert_with(|| presence(&root, stable.as_deref()) == Presence::Connected);
        if !up {
            continue;
        }
        let path = paths::join(&root, &relative);
        if !path.is_file() {
            continue;
        }
        // Lower is better for the main picture: the user's own choice, box art, then images.
        let rank = match (kind.as_str(), source.as_str()) {
            ("boxart", "user") => 0,
            ("boxart", _) => 1,
            ("image", _) => 2,
            ("thumbnail", _) => 3,
            _ => 9,
        };
        found.push((rank, kind, path));
    }
    found.sort_by_key(|f| f.0);
    let image = found.iter().find(|f| f.0 <= 3).map(|f| f.2.clone());
    let thumbnail = found.iter().find(|f| f.1 == "thumbnail").map(|f| f.2.clone()).filter(|t| Some(t) != image.as_ref());
    let marquee = found.iter().find(|f| f.1 == "marquee").map(|f| f.2.clone());
    let unavailable = any && image.is_none() && thumbnail.is_none() && marquee.is_none();
    Ok((image, thumbnail, marquee, unavailable))
}

fn details_for(connection: &Connection, file_id: &str) -> Result<Details, String> {
    let mut statement = connection
        .prepare_cached(
            "SELECT re.title, re.year, re.genre, re.developer, re.publisher
             FROM game_files gf JOIN game_releases r ON r.id = gf.release_id JOIN game_reference gr ON gr.game_id = r.game_id
             JOIN reference_entries re ON re.id = gr.entry_id WHERE gf.id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = statement.query([file_id]).map_err(|e| e.to_string())?;
    let Some(r) = rows.next().map_err(|e| e.to_string())? else { return Ok(Details::default()) };
    let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Ok(Details { title: clean(r.get(0).ok()), year: r.get(1).ok().flatten(), genre: clean(r.get(2).ok()), developer: clean(r.get(3).ok()), publisher: clean(r.get(4).ok()) })
}

/// Games RetroDrive copied (verified, identical or replaced primary files still on the drive).
fn targets(connection: &Connection, install: &customizer::Install, deployment: Option<&str>, presence: PresenceFn) -> Result<(Vec<Target>, usize), String> {
    let mut statement = connection
        .prepare(
            "SELECT i.destination_path, i.game_file_id FROM deployment_items i
             JOIN deployments d ON d.id = i.deployment_id JOIN game_files gf ON gf.id = i.game_file_id
             WHERE d.installation_id = ?1 AND (?2 IS NULL OR d.id = ?2) AND gf.file_role = 'primary'
               AND i.state IN ('verified', 'identical', 'replaced') ORDER BY i.id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = statement
        .query_map(params![install.id, deployment], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let roms = format!("{}\\roms\\", install.root_text.trim_end_matches(['\\', '/']));
    let mut seen: BTreeMap<String, Target> = BTreeMap::new();
    let mut online = HashMap::new();
    let mut unavailable = 0;
    for (destination, file_id) in rows {
        let d = destination.replace('/', "\\");
        if d.len() <= roms.len() || !d[..roms.len()].eq_ignore_ascii_case(&roms) || !Path::new(&destination).is_file() {
            continue;
        }
        let inside = d[roms.len()..].replace('\\', "/");
        let Some((system, rel)) = inside.split_once('/') else { continue };
        let (image, thumbnail, marquee, missing) = media_for(connection, &file_id, presence, &mut online)?;
        if missing {
            unavailable += 1;
        }
        let details = details_for(connection, &file_id)?;
        seen.insert(format!("{system}/{}", rel.to_lowercase()), Target { system: system.to_string(), rel: rel.to_string(), image, thumbnail, marquee, details });
    }
    Ok((seen.into_values().collect(), unavailable))
}

// ---------- planning edits ----------

struct SystemEdit {
    system: String,
    list_path: PathBuf,
    old_text: String,
    new_text: String,
    copies: Vec<(PathBuf, PathBuf)>,
    summary: SystemArt,
}

fn xml(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn plan_system(root: &Path, system: &str, targets: &[&Target]) -> Result<Option<SystemEdit>, String> {
    let list_path = root.join("roms").join(system).join("gamelist.xml");
    let exists = list_path.is_file();
    let old_text = if exists { customizer::read_text(&list_path)? } else { String::new() };
    let base = if exists { old_text.clone() } else { "<?xml version=\"1.0\"?>\n<gameList>\n</gameList>\n".to_string() };
    let entries = parse(&base)?;
    let newline = if base.contains("\r\n") { "\r\n" } else { "\n" };
    let by_key: HashMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
    let images_dir = root.join("roms").join(system).join("images");
    let mut summary = SystemArt { system: system.to_string(), games_updated: 0, games_added: 0, images: 0, kept: 0 };
    let mut copies = Vec::new();
    // (position, replace_end, text) edits; applied from the end.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut new_entries = String::new();

    for t in targets {
        let entry = by_key.get(norm(&t.rel).as_str()).copied();
        let mut lines: Vec<String> = Vec::new();
        let art_field = |tag: &str, suffix: &str, source: &Option<PathBuf>, lines: &mut Vec<String>, copies: &mut Vec<(PathBuf, PathBuf)>| -> bool {
            let Some(src) = source else { return false };
            if entry.is_some_and(|e| has(e, tag)) {
                return false;
            }
            let name = image_name(&t.rel, suffix, src);
            let target = images_dir.join(&name);
            match fs::symlink_metadata(&target) {
                Ok(_) if copier::sha256_of(&target).ok() != copier::sha256_of(src).ok() => return false, // someone else's file: leave it
                Ok(_) => {}
                Err(_) => copies.push((src.clone(), target)),
            }
            lines.push(format!("<{tag}>./images/{}</{tag}>", xml(&name)));
            true
        };
        let had_art = entry.is_some_and(|e| has(e, "image"));
        let mut added_images = 0;
        added_images += art_field("image", "image", &t.image, &mut lines, &mut copies) as usize;
        added_images += art_field("thumbnail", "thumb", &t.thumbnail, &mut lines, &mut copies) as usize;
        added_images += art_field("marquee", "marquee", &t.marquee, &mut lines, &mut copies) as usize;
        if had_art && t.image.is_some() {
            summary.kept += 1;
        }
        let d = &t.details;
        let add_text = |tag: &str, value: &Option<String>, lines: &mut Vec<String>| {
            if let Some(v) = value {
                if !entry.is_some_and(|e| has(e, tag)) {
                    lines.push(format!("<{tag}>{}</{tag}>", xml(v)));
                }
            }
        };
        add_text("genre", &d.genre, &mut lines);
        add_text("developer", &d.developer, &mut lines);
        add_text("publisher", &d.publisher, &mut lines);
        if let Some(year) = d.year.filter(|y| (1950..2100).contains(y)) {
            if !entry.is_some_and(|e| has(e, "releasedate")) {
                lines.push(format!("<releasedate>{year}0101T000000</releasedate>"));
            }
        }
        let file = t.rel.rsplit('/').next().unwrap_or(&t.rel);
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        match entry {
            Some(e) => {
                let mut changed = !lines.is_empty();
                // A name that's just the file name is replaced by the catalog title.
                if let (Some(title), Some((name, start, end))) = (&d.title, &e.name) {
                    if name.eq_ignore_ascii_case(stem) && name != title {
                        edits.push((*start, *end, xml(title)));
                        changed = true;
                    }
                } else if let (Some(title), None) = (&d.title, &e.name) {
                    lines.push(format!("<name>{}</name>", xml(title)));
                    changed = true;
                }
                if !lines.is_empty() {
                    // Insert before </game>, on its own lines with the entry's indentation.
                    let line_start = base[..e.close].rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let own_line = base[line_start..e.close].trim().is_empty();
                    let at = if own_line { line_start } else { e.close };
                    let mut block = String::new();
                    for l in &lines {
                        block.push_str(&format!("{}{l}{newline}", e.indent));
                    }
                    if !own_line {
                        block = format!("{newline}{block}");
                    }
                    edits.push((at, at, block));
                }
                if changed {
                    summary.games_updated += 1;
                    summary.images += added_images;
                }
            }
            None => {
                if lines.is_empty() && d.title.is_none() {
                    continue;
                }
                new_entries.push_str(&format!("\t<game>{newline}\t\t<path>./{}</path>{newline}\t\t<name>{}</name>{newline}", xml(&t.rel), xml(d.title.as_deref().unwrap_or(stem))));
                for l in &lines {
                    new_entries.push_str(&format!("\t\t{l}{newline}"));
                }
                new_entries.push_str(&format!("\t</game>{newline}"));
                summary.games_added += 1;
                summary.images += added_images;
            }
        }
    }
    if edits.is_empty() && new_entries.is_empty() {
        return Ok(None);
    }
    if !new_entries.is_empty() {
        let close = base.rfind("</gameList>").ok_or("gamelist.xml has no </gameList>, so RetroDrive won't edit it.")?;
        let line_start = base[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let own_line = base[line_start..close].trim().is_empty();
        let at = if own_line { line_start } else { close };
        edits.push((at, at, if own_line { new_entries } else { format!("{newline}{new_entries}") }));
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let mut new_text = base.clone();
    for (start, end, text) in edits {
        new_text.replace_range(start..end, &text);
    }
    // Check: still valid, every old entry still there, the right number of new ones.
    let after = parse(&new_text).map_err(|e| format!("Editing {system}'s game list would make it invalid ({e}); nothing was changed."))?;
    if after.len() != entries.len() + summary.games_added || entries.iter().any(|e| !after.iter().any(|a| a.key == e.key)) {
        return Err(format!("Editing {system}'s game list would have lost entries; nothing was changed."));
    }
    copies.sort();
    copies.dedup();
    Ok(Some(SystemEdit { system: system.to_string(), list_path, old_text, new_text, copies, summary }))
}

fn plan(connection: &Connection, installation_id: &str, deployment: Option<&str>, presence: PresenceFn) -> Result<(customizer::Install, Vec<SystemEdit>, usize), String> {
    let install = customizer::install(connection, installation_id, presence)?;
    let (all, unavailable) = targets(connection, &install, deployment, presence)?;
    let mut by_system: BTreeMap<String, Vec<&Target>> = BTreeMap::new();
    for t in &all {
        by_system.entry(t.system.clone()).or_default().push(t);
    }
    let mut edits = Vec::new();
    for (system, list) in by_system {
        if system.contains(['.', '\\', ':']) {
            continue;
        }
        if let Some(edit) = plan_system(&install.root, &system, &list)? {
            edits.push(edit);
        }
    }
    Ok((install, edits, unavailable))
}

fn sha_or_none(edit: &SystemEdit) -> String {
    if edit.list_path.is_file() {
        customizer::hex_sha(edit.old_text.as_bytes())
    } else {
        "none".into()
    }
}

/// What adding art and details would do (reads only). `deployment`: only games from that copy.
pub fn preview(connection: &Connection, installation_id: &str, deployment: Option<&str>, presence: PresenceFn) -> Result<ArtPreview, String> {
    let (_, edits, unavailable) = plan(connection, installation_id, deployment, presence)?;
    let images = edits.iter().map(|e| e.copies.len()).sum();
    let image_bytes = edits.iter().flat_map(|e| e.copies.iter()).filter_map(|(s, _)| fs::metadata(s).ok()).map(|m| m.len()).sum();
    Ok(ArtPreview {
        games_updated: edits.iter().map(|e| e.summary.games_updated + e.summary.games_added).sum(),
        list_shas: edits.iter().map(|e| (e.system.clone(), sha_or_none(e))).collect(),
        systems: edits.into_iter().map(|e| e.summary).collect(),
        images,
        image_bytes,
        art_unavailable: unavailable,
        nothing: images == 0 && false,
    })
    .map(|mut p| {
        p.nothing = p.systems.is_empty();
        p
    })
}

/// Copy one image to a new file (temporary name, checked, renamed without replacing).
fn copy_image(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
    }
    let marker = format!("{TEMP_MARKER}{}", &db::new_id()[..8]);
    let temp = PathBuf::from(format!("{}{marker}", target.display()));
    let copied = (|| -> std::io::Result<()> {
        let mut input = fs::File::open(source)?;
        let mut out = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        std::io::copy(&mut input, &mut out)?;
        out.flush()?;
        out.sync_all()
    })();
    let fail = |m: String| -> Result<(), String> {
        copier::remove_own_temp(&temp, &marker);
        Err(m)
    };
    if let Err(e) = copied {
        return fail(format!("Couldn't copy {}: {e}", source.display()));
    }
    if copier::sha256_of(&temp).ok() != copier::sha256_of(source).ok() {
        return fail(format!("The copy of {} didn't match, so it wasn't used.", source.display()));
    }
    copier::rename_no_replace(&temp, target).or_else(|e| fail(format!("Couldn't name {}: {e}", target.display())).map(|_| ()))
}

/// Add art and details. `expected`: the list checksums from the preview (`None`: no review, e.g. right after a copy).
pub fn apply(connection: &Connection, installation_id: &str, deployment: Option<&str>, expected: Option<&BTreeMap<String, String>>, presence: PresenceFn, running: RunningFn, progress: &dyn Fn(usize, usize)) -> Result<String, String> {
    if let Some(reason) = customizer::running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let (install, edits, _) = plan(connection, installation_id, deployment, presence)?;
    if edits.is_empty() {
        return Err("There's no art or detail to add: the games RetroDrive copied already have them, or their images aren't available.".into());
    }
    if let Some(expected) = expected {
        let now: BTreeMap<String, String> = edits.iter().map(|e| (e.system.clone(), sha_or_none(e))).collect();
        if &now != expected {
            return Err("RetroBat's game lists changed since you reviewed this, so nothing was changed. Review it again.".into());
        }
    }
    // 1. Images (new files only).
    let total: usize = edits.iter().map(|e| e.copies.len()).sum();
    let mut done = 0;
    let mut added = Vec::new();
    for e in &edits {
        for (src, dst) in &e.copies {
            if fs::symlink_metadata(dst).is_err() {
                copy_image(src, dst)?;
            }
            done += 1;
            if done % 25 == 0 || done == total {
                progress(done, total);
            }
        }
        if !e.copies.is_empty() {
            added.push(format!("{} image(s) in roms/{}/images", e.copies.len(), e.system));
        }
    }
    // 2. Game lists, journaled; if one fails, the ones already written are undone.
    let mut ids = Vec::new();
    for e in &edits {
        let relative = format!("roms/{}/gamelist.xml", e.system);
        let s = &e.summary;
        let rows = vec![ChangeRow { system: e.system.clone(), key: "gamelist".into(), before: None, after: Some(format!("{} game(s) updated, {} added, {} image(s)", s.games_updated, s.games_added, s.images)) }];
        match customizer::write_file(connection, &install, &relative, &e.list_path, &e.old_text, &e.new_text, rows) {
            Ok((_, id)) => ids.push(id),
            Err(err) => {
                let all = ids.iter().rev().all(|id| customizer::undo(connection, *id, presence, running).is_ok());
                return Err(format!("{err} {}", if all { "The game lists already changed were put back." } else { "Some game lists were changed; undo them from Changes made." }));
            }
        }
    }
    let games: usize = edits.iter().map(|e| e.summary.games_updated + e.summary.games_added).sum();
    connection
        .execute(
            "INSERT INTO design_applications (design_id, design_name, installation_id, settings_change_ids, added_json, state) VALUES (NULL, 'Game art and details', ?1, ?2, ?3, 'applied')",
            params![installation_id, serde_json::to_string(&ids).unwrap_or_default(), serde_json::to_string(&added).unwrap_or_default()],
        )
        .map_err(|e| e.to_string())?;
    Ok(format!("Art and details added for {games} game(s) in {} system(s) ({total} image(s) copied). Undo it under Applied designs.", edits.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;
    use crate::{installations, scanner};

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn closed() -> bool {
        false
    }
    fn open() -> bool {
        true
    }
    fn quiet(_: usize, _: usize) {}

    const LIST: &str = "<?xml version=\"1.0\"?>\r\n<gameList>\r\n\t<game>\r\n\t\t<path>./Kid.zip</path>\r\n\t\t<name>Kid</name>\r\n\t\t<region>us</region>\r\n\t</game>\r\n\t<game>\r\n\t\t<path>./Scraped.zip</path>\r\n\t\t<name>Scraped Game</name>\r\n\t\t<image>./images/theirs.png</image>\r\n\t</game>\r\n\t<game>\r\n\t\t<path>./Other.zip</path>\r\n\t\t<name>Other</name>\r\n\t</game>\r\n</gameList>\r\n";

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
    }

    /// A library with art (gamelist images) for three NES games, copied to a RetroBat.
    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("library/nes");
        fs::create_dir_all(lib.join("media/images")).unwrap();
        fs::create_dir_all(lib.join("media/wheel")).unwrap();
        for g in ["Kid", "Scraped", "New Game"] {
            fs::write(lib.join(format!("{g}.zip")), g.as_bytes()).unwrap();
            fs::write(lib.join(format!("media/images/{g}.png")), format!("art of {g}")).unwrap();
        }
        fs::write(lib.join("media/wheel/Kid.png"), b"kid logo").unwrap();
        fs::write(
            lib.join("gamelist.xml"),
            "<gameList><game><path>./Kid.zip</path><image>./media/images/Kid.png</image><marquee>./media/wheel/Kid.png</marquee></game><game><path>./Scraped.zip</path><image>./media/images/Scraped.png</image></game><game><path>./New Game.zip</path><image>./media/images/New Game.png</image></game></gameList>",
        )
        .unwrap();
        let root = dir.path().join("RetroBat");
        for d in ["emulationstation/.emulationstation", "roms/nes", "bios"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("emulationstation/.emulationstation/es_settings.cfg"), "<config></config>").unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &dir.path().join("library"), &|_: &str| VolumeProbe::unknown()).unwrap();
        let install = installations::add(&catalog, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let roomy = |_: &Path| Some(crate::volumes::VolumeSpace { free_bytes: 500 << 30, total_bytes: 1 << 40, filesystem: Some("NTFS".into()), cluster_bytes: 4096 });
        let choices = crate::deployment::Choices::new();
        let plan = crate::deployment::plan_with(&catalog, &install, None, None, &choices, &connected, &roomy).unwrap();
        struct Quiet;
        impl copier::DeployObserver for Quiet {
            fn workers(&self) -> usize {
                1
            }
        }
        copier::deploy(&catalog, None, &copier::DeployRequest { installation_id: &install, systems: None, fingerprint: &plan.fingerprint, choices: &choices, collection: None }, &connected, &roomy, &mut Quiet).unwrap();
        // The catalog knows Kid's proper title and details.
        catalog.execute("INSERT INTO reference_catalogs (id, name, file_name, entries, genres_added) VALUES ('c', 'test', 't.csv', 1, 0)", []).unwrap();
        catalog
            .execute("INSERT INTO reference_entries (id, catalog_id, entry_key, title, platform, platform_label, year, genre, publisher, developer) VALUES (1, 'c', 'NES-kid', 'Kid Icarus', 'NES', 'NES', 1986, 'Platform', 'Nintendo', 'Nintendo R&D1')", [])
            .unwrap();
        catalog.execute("INSERT INTO game_reference (game_id, entry_id, method) SELECT g.id, 1, 'name' FROM games g WHERE g.canonical_title = 'Kid'", []).unwrap();
        let root = crate::paths::normalize(&root).unwrap();
        fs::write(root.join("roms/nes/gamelist.xml"), LIST).unwrap();
        Setup { _dir: dir, root, catalog, install }
    }

    fn list(s: &Setup) -> String {
        fs::read_to_string(s.root.join("roms/nes/gamelist.xml")).unwrap()
    }

    #[test]
    fn art_and_details_fill_gaps_and_keep_retrobats_own() {
        let s = setup();
        let p = preview(&s.catalog, &s.install, None, &connected).unwrap();
        assert_eq!(p.systems.len(), 1);
        let nes = &p.systems[0];
        assert_eq!((nes.games_updated, nes.games_added, nes.kept), (1, 1, 1), "Kid updated, New Game added, Scraped kept");
        assert_eq!(p.images, 3, "Kid's image and logo, New Game's image");
        assert_eq!(list(&s), LIST, "previewing changes nothing");

        assert!(apply(&s.catalog, &s.install, None, Some(&p.list_shas), &connected, &open, &quiet).unwrap_err().contains("running"));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), &connected, &closed, &quiet).unwrap();
        let after = list(&s);
        assert!(after.contains("\t\t<name>Kid Icarus</name>\r\n"), "a file-name title becomes the catalog title");
        assert!(after.contains("\t\t<image>./images/Kid-image.png</image>\r\n\t\t<marquee>./images/Kid-marquee.png</marquee>\r\n"));
        assert!(after.contains("<genre>Platform</genre>") && after.contains("<developer>Nintendo R&amp;D1</developer>") && after.contains("<releasedate>19860101T000000</releasedate>"));
        assert!(after.contains("<image>./images/theirs.png</image>") && !after.contains("Scraped-image"), "RetroBat's own art is kept");
        assert!(after.contains("<path>./New Game.zip</path>") && after.contains("./images/New Game-image.png"));
        assert!(after.contains("\t</game>\r\n\t<game>\r\n\t\t<path>./New Game.zip</path>"), "new entries follow without blank lines");
        assert!(after.ends_with("\t</game>\r\n</gameList>\r\n"));
        assert!(after.contains("<path>./Other.zip</path>\r\n\t\t<name>Other</name>\r\n\t</game>"), "unrelated entries untouched");
        assert_eq!(fs::read(s.root.join("roms/nes/images/Kid-image.png")).unwrap(), b"art of Kid");
        assert_eq!(fs::read(s.root.join("roms/nes/images/Kid-marquee.png")).unwrap(), b"kid logo");

        // Nothing left to add; undo restores the list exactly (images stay).
        assert!(preview(&s.catalog, &s.install, None, &connected).unwrap().nothing);
        let app = crate::designs::applications(&s.catalog, &s.install).unwrap()[0].id;
        crate::designs::undo(&s.catalog, app, &connected, &closed).unwrap();
        assert_eq!(list(&s), LIST);
        assert!(s.root.join("roms/nes/images/Kid-image.png").exists(), "copied images are kept");
    }

    #[test]
    fn a_missing_list_is_created_and_a_stale_review_is_refused() {
        let s = setup();
        fs::remove_file(s.root.join("roms/nes/gamelist.xml")).unwrap();
        let p = preview(&s.catalog, &s.install, None, &connected).unwrap();
        assert_eq!(p.list_shas.get("nes").map(String::as_str), Some("none"));
        assert_eq!(p.systems[0].games_added, 3);
        let mut stale = p.list_shas.clone();
        stale.insert("nes".into(), "different".into());
        assert!(apply(&s.catalog, &s.install, None, Some(&stale), &connected, &closed, &quiet).unwrap_err().contains("changed since"));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), &connected, &closed, &quiet).unwrap();
        let created = list(&s);
        assert!(created.starts_with("<?xml") && created.matches("<game>").count() == 3);
        // An image name taken by someone else's different file is left alone.
        let s2 = setup();
        fs::create_dir_all(s2.root.join("roms/nes/images")).unwrap();
        fs::write(s2.root.join("roms/nes/images/Kid-image.png"), b"someone else's").unwrap();
        apply(&s2.catalog, &s2.install, None, None, &connected, &closed, &quiet).unwrap();
        assert_eq!(fs::read(s2.root.join("roms/nes/images/Kid-image.png")).unwrap(), b"someone else's");
        assert!(!list(&s2).contains("Kid-image.png"));
    }

    #[test]
    fn gamelists_are_read_with_their_layout() {
        let e = parse(LIST).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].key, "kid.zip");
        assert!(has(&e[1], "image") && !has(&e[0], "image"));
        assert_eq!(e[0].indent, "\t\t");
        assert!(parse("<gameList><game><path>x</gameList>").is_err());
        assert_eq!(image_name("Saga/Saga.m3u", "image", Path::new("a.JPG")), "Saga-image.jpg");
    }
}
