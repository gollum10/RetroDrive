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
//!
//! Batocera (2026-11-07) uses the same `roms/<system>/gamelist.xml` and `./images/`
//! convention, so the same step works on a Batocera game drive or share. Batocera runs
//! on its own machine (no "running" check here); a running Batocera shows the new art
//! after Start → Game Settings → Update Gamelists.
//!
//! RetroPie (2026-11-07, docs/18 §27): its EmulationStation uses a `gamelist.xml` in
//! the system's rom folder *instead of* the one in its home folder
//! (`SystemData::getGamelistPath`), which holds any scraped art, favorites and play
//! counts and which Windows can't see. So on RetroPie only lists already in the rom
//! folder are filled in; creating one is an explicit opt-in (`create_lists`), never
//! done after a copy. RetroPie's roms share has no `roms` level: system folders are
//! found under `games_dir`.
//!
//! ES-DE (2026-11-07, docs/18 §32) keeps its lists in its data folder
//! (`ES-DE\gamelists\<system>\gamelist.xml`) and finds media by file name in
//! `downloaded_media\<system>\<type>\<game path without extension>.png|jpg` (user
//! guide), with no image tags in the list. So for ES-DE the details go into its list
//! and the pictures are copied as `covers` (box art or main image) and `marquees`
//! (logos) named after each game; a picture ES-DE already has (any extension) is kept.
//! ES-DE saves its lists when it closes, so changes are refused while it runs.
//!
//! Recalbox (2026-11-07) keeps `gamelist.xml` in each system's rom folder with
//! images referenced from it (Recalbox wiki, internal scraper), like Batocera, so it
//! uses the in-folder layout. It runs on its own machine (no running check).

use crate::copier::{self, TEMP_MARKER};

/// Platforms whose game lists RetroDrive fills in (same format).
pub const GAMELIST_PLATFORMS: &[&str] = &["retrobat", "batocera", "retropie", "esde", "recalbox"];

/// Where a platform keeps its game lists and pictures.
enum Layout {
    /// `<games>\<system>\gamelist.xml`, pictures in `<system>\images` with tags in the list.
    InFolder,
    /// ES-DE: lists under `<data>\gamelists`, pictures under `<media>\<system>\<type>` by name.
    EsDe { data: PathBuf, media: PathBuf },
}

/// ES-DE's data folder for a ROMs folder: `ES-DE` next to a portable `ES-DE.exe`, or
/// `%USERPROFILE%\ES-DE` when its settings point at this ROMs folder (or at the default one).
pub(crate) fn esde_data_dir(roms: &Path) -> Option<PathBuf> {
    if let Some(parent) = roms.parent().filter(|p| p.join("ES-DE.exe").is_file()) {
        return Some(parent.join("ES-DE")).filter(|d| d.is_dir());
    }
    let home = PathBuf::from(std::env::var_os("USERPROFILE")?);
    let data = home.join("ES-DE");
    let setting = esde_setting(&data, "ROMDirectory").unwrap_or_default();
    let target = if setting.is_empty() { home.join("ROMs") } else { PathBuf::from(setting.replace("~", &home.display().to_string())) };
    let same = |a: &Path, b: &Path| a.display().to_string().trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.display().to_string().trim_end_matches(['\\', '/']));
    (data.is_dir() && same(&target, roms)).then_some(data)
}

/// A string setting from ES-DE's `settings\es_settings.xml`.
fn esde_setting(data: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(data.join("settings").join("es_settings.xml")).ok()?;
    let values = customizer::values(&customizer::entries(&text).ok()?).ok()?;
    values.get(key).map(|v| v.trim().to_string())
}

/// ES-DE's media folder: the `MediaDirectory` setting, or `<data>\downloaded_media`.
fn esde_media_dir(data: &Path) -> PathBuf {
    esde_setting(data, "MediaDirectory").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| data.join("downloaded_media"))
}

/// Whether ES-DE already has a picture of this type for a game (`.png`, `.jpg` or `.jpeg`).
fn esde_has_media(dir: &Path, stem: &str) -> bool {
    ["png", "jpg", "jpeg"].iter().any(|ext| dir.join(format!("{stem}.{ext}")).is_file())
}

/// Where the system folders are: `<root>\roms`, or the root itself (RetroPie's roms share).
fn games_dir(root: &Path) -> (PathBuf, &'static str) {
    if root.join("roms").is_dir() {
        (root.join("roms"), "roms/")
    } else {
        (root.to_path_buf(), "")
    }
}
use crate::customizer::{self, ChangeRow, PresenceFn, RunningFn};
use crate::volumes::Presence;
use crate::{db, paths};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Progress: `(phase, done, total)`; phases are `checking` (games) and `copying` (images).
pub type ProgressFn<'a> = &'a (dyn Fn(&str, usize, usize) + Sync);

/// Files are checked this many at a time: on a network share each check mostly waits on
/// the network (measured 2026-11-07 on the owner's NAS: 4,000 games took 10 minutes one by one).
const CHECK_THREADS: usize = 16;

/// Runs `work` on every item, `CHECK_THREADS` at a time, keeping the order.
fn parallel_map<T: Sync, R: Send>(items: &[T], phase: &str, work: impl Fn(&T) -> R + Sync, progress: ProgressFn) -> Vec<R> {
    let total = items.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let (work, next, done) = (&work, &next, &done);
    let mut results: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..CHECK_THREADS.min(total))
            .map(|_| {
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= total {
                            break mine;
                        }
                        mine.push((i, work(&items[i])));
                        let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                        if d % 50 == 0 || d == total {
                            progress(phase, d, total);
                        }
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p))).collect()
    });
    results.sort_by_key(|r| r.0);
    results.into_iter().map(|r| r.1).collect()
}

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
    /// RetroPie: systems with games but no game list in their rom folder (not created
    /// unless asked: RetroPie would then use it instead of its home-folder list).
    pub lists_not_created: Vec<String>,
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

/// A game's known art on connected drives as (rank, kind, path), and whether it has any art at all.
type Candidates = (Vec<(i32, String, PathBuf)>, bool);

/// From the catalog only (no file checks): art recorded for a game file on drives that are connected.
fn media_for(connection: &Connection, file_id: &str, presence: PresenceFn, online: &mut HashMap<String, bool>) -> Result<Candidates, String> {
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
    Ok((found, any))
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

/// A game found in the catalog, before its files are checked.
struct Pending {
    system: String,
    rel: String,
    destination: PathBuf,
    media: Vec<(i32, String, PathBuf)>,
    any_media: bool,
    details: Details,
}

/// Image sizes seen while checking (so nothing is looked at twice).
type Sizes = HashMap<PathBuf, u64>;

/// Games RetroDrive copied (verified, identical or replaced primary files still on the drive).
fn targets(connection: &Connection, install: &customizer::Install, deployment: Option<&str>, presence: PresenceFn, progress: ProgressFn) -> Result<(Vec<Target>, usize, Sizes), String> {
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
    let roms = format!("{}\\", games_dir(&install.root).0.display().to_string().trim_end_matches(['\\', '/']));
    // 1. The catalog (fast): one entry per game on the drive; a later copy of the same file wins.
    let mut seen: BTreeMap<String, Pending> = BTreeMap::new();
    let mut online = HashMap::new();
    for (destination, file_id) in rows {
        let d = destination.replace('/', "\\");
        if d.len() <= roms.len() || !d[..roms.len()].eq_ignore_ascii_case(&roms) {
            continue;
        }
        let inside = d[roms.len()..].replace('\\', "/");
        let Some((system, rel)) = inside.split_once('/') else { continue };
        let (media, any_media) = media_for(connection, &file_id, presence, &mut online)?;
        let details = details_for(connection, &file_id)?;
        seen.insert(format!("{system}/{}", rel.to_lowercase()), Pending { system: system.to_string(), rel: rel.to_string(), destination: PathBuf::from(&destination), media, any_media, details });
    }
    // 2. The files (slow on a network share): checked in parallel.
    let pending: Vec<Pending> = seen.into_values().collect();
    let checked = parallel_map(
        &pending,
        "checking",
        |p| {
            if !p.destination.is_file() {
                return None;
            }
            Some(p.media.iter().filter_map(|(rank, kind, path)| fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| (*rank, kind.clone(), path.clone(), m.len()))).collect::<Vec<_>>())
        },
        progress,
    );
    let mut out = Vec::new();
    let mut sizes = Sizes::new();
    let mut unavailable = 0;
    for (p, found) in pending.into_iter().zip(checked) {
        let Some(found) = found else { continue };
        for (_, _, path, size) in &found {
            sizes.insert(path.clone(), *size);
        }
        let image = found.iter().find(|f| f.0 <= 3).map(|f| f.2.clone());
        let thumbnail = found.iter().find(|f| f.1 == "thumbnail").map(|f| f.2.clone()).filter(|t| Some(t) != image.as_ref());
        let marquee = found.iter().find(|f| f.1 == "marquee").map(|f| f.2.clone());
        if p.any_media && image.is_none() && thumbnail.is_none() && marquee.is_none() {
            unavailable += 1;
        }
        out.push(Target { system: p.system, rel: p.rel, image, thumbnail, marquee, details: p.details });
    }
    Ok((out, unavailable, sizes))
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

fn plan_system(games: &Path, layout: &Layout, system: &str, targets: &[&Target], sizes: &Sizes) -> Result<Option<SystemEdit>, String> {
    let list_path = match layout {
        Layout::InFolder => games.join(system).join("gamelist.xml"),
        Layout::EsDe { data, .. } => data.join("gamelists").join(system).join("gamelist.xml"),
    };
    let exists = list_path.is_file();
    let old_text = if exists { customizer::read_text(&list_path)? } else { String::new() };
    let base = if exists { old_text.clone() } else { "<?xml version=\"1.0\"?>\n<gameList>\n</gameList>\n".to_string() };
    let entries = parse(&base)?;
    let newline = if base.contains("\r\n") { "\r\n" } else { "\n" };
    let by_key: HashMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
    let images_dir = games.join(system).join("images");
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
                // A file is already there: use it only if it's the same image (sizes first, then contents); otherwise it's someone else's: leave it.
                Ok(m) => {
                    if sizes.get(src) != Some(&m.len()) || copier::sha256_of(&target).ok() != copier::sha256_of(src).ok() {
                        return false;
                    }
                }
                Err(_) => copies.push((src.clone(), target)),
            }
            lines.push(format!("<{tag}>./images/{}</{tag}>", xml(&name)));
            true
        };
        let mut added_images = 0;
        match layout {
            Layout::InFolder => {
                let had_art = entry.is_some_and(|e| has(e, "image"));
                added_images += art_field("image", "image", &t.image, &mut lines, &mut copies) as usize;
                added_images += art_field("thumbnail", "thumb", &t.thumbnail, &mut lines, &mut copies) as usize;
                added_images += art_field("marquee", "marquee", &t.marquee, &mut lines, &mut copies) as usize;
                if had_art && t.image.is_some() {
                    summary.kept += 1;
                }
            }
            Layout::EsDe { media, .. } => {
                // Pictures named after the game (its path without the extension), no tags.
                let stem_path = t.rel.rsplit_once('.').map(|(s, _)| s).unwrap_or(&t.rel);
                if stem_path.split('/').any(|p| p.is_empty() || p == "..") {
                    continue;
                }
                let mut had = false;
                for (kind, source) in [("covers", t.image.as_ref().or(t.thumbnail.as_ref())), ("marquees", t.marquee.as_ref())] {
                    let Some(src) = source else { continue };
                    let dir = media.join(system).join(kind);
                    let (sub, stem) = stem_path.rsplit_once('/').map(|(d, s)| (Some(d), s)).unwrap_or((None, stem_path));
                    let dir = sub.map(|d| dir.join(d)).unwrap_or(dir);
                    if esde_has_media(&dir, stem) {
                        had = true;
                        continue;
                    }
                    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                    copies.push((src.clone(), dir.join(format!("{stem}.{ext}"))));
                    added_images += 1;
                }
                if had {
                    summary.kept += 1;
                }
            }
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
                if changed || added_images > 0 {
                    summary.games_updated += 1;
                    summary.images += added_images;
                }
            }
            None => {
                if lines.is_empty() && d.title.is_none() {
                    // ES-DE: a picture alone needs no list entry (it's found by name).
                    if added_images > 0 {
                        summary.games_updated += 1;
                        summary.images += added_images;
                    }
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
    if edits.is_empty() && new_entries.is_empty() && copies.is_empty() {
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

type Planned = (customizer::Install, Vec<SystemEdit>, usize, Sizes, Vec<String>);

/// `create_lists`: on RetroPie, also create lists for systems without one (opt-in).
fn plan(connection: &Connection, installation_id: &str, deployment: Option<&str>, create_lists: bool, presence: PresenceFn, progress: ProgressFn) -> Result<Planned, String> {
    let install = customizer::install_for(connection, installation_id, presence, GAMELIST_PLATFORMS, "Game art and details are available for RetroBat, Batocera, RetroPie, ES-DE and Recalbox.")?;
    let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [installation_id], |r| r.get(0)).map_err(|e| e.to_string())?;
    let (games, _) = games_dir(&install.root);
    let layout = if adapter == "esde" {
        let data = esde_data_dir(&install.root).ok_or("RetroDrive couldn't find ES-DE's settings folder for this ROMs folder (ES-DE next to its ES-DE.exe, or in your user folder), so it can't add art for ES-DE.")?;
        let media = esde_media_dir(&data);
        Layout::EsDe { data, media }
    } else {
        Layout::InFolder
    };
    let mut not_created = Vec::new();
    let (all, unavailable, sizes) = targets(connection, &install, deployment, presence, progress)?;
    let mut by_system: BTreeMap<String, Vec<&Target>> = BTreeMap::new();
    for t in &all {
        by_system.entry(t.system.clone()).or_default().push(t);
    }
    let mut edits = Vec::new();
    for (system, list) in by_system {
        if system.contains(['.', '\\', ':']) {
            continue;
        }
        if adapter == "retropie" && !create_lists && !games.join(&system).join("gamelist.xml").is_file() {
            not_created.push(system);
            continue;
        }
        if let Some(edit) = plan_system(&games, &layout, &system, &list, &sizes)? {
            edits.push(edit);
        }
    }
    Ok((install, edits, unavailable, sizes, not_created))
}

fn sha_or_none(edit: &SystemEdit) -> String {
    if edit.list_path.is_file() {
        customizer::hex_sha(edit.old_text.as_bytes())
    } else {
        "none".into()
    }
}

/// What adding art and details would do (reads only). `deployment`: only games from that copy.
pub fn preview(connection: &Connection, installation_id: &str, deployment: Option<&str>, create_lists: bool, presence: PresenceFn, progress: ProgressFn) -> Result<ArtPreview, String> {
    let (_, edits, unavailable, sizes, lists_not_created) = plan(connection, installation_id, deployment, create_lists, presence, progress)?;
    let images = edits.iter().map(|e| e.copies.len()).sum();
    let image_bytes = edits.iter().flat_map(|e| e.copies.iter()).filter_map(|(s, _)| sizes.get(s)).sum();
    Ok(ArtPreview {
        games_updated: edits.iter().map(|e| e.summary.games_updated + e.summary.games_added).sum(),
        list_shas: edits.iter().map(|e| (e.system.clone(), sha_or_none(e))).collect(),
        systems: edits.into_iter().map(|e| e.summary).collect(),
        images,
        image_bytes,
        art_unavailable: unavailable,
        nothing: false,
        lists_not_created,
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
#[allow(clippy::too_many_arguments)]
pub fn apply(connection: &Connection, installation_id: &str, deployment: Option<&str>, expected: Option<&BTreeMap<String, String>>, create_lists: bool, presence: PresenceFn, running: RunningFn, progress: ProgressFn) -> Result<String, String> {
    if let Some(reason) = customizer::running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let (install, edits, _, _, _) = plan(connection, installation_id, deployment, create_lists, presence, progress)?;
    let (_, prefix) = games_dir(&install.root);
    // ES-DE's lists live in its data folder: journal and back them up there.
    let esde = connection.query_row("SELECT adapter_id = 'esde' FROM installations WHERE id = ?1", [installation_id], |r| r.get::<_, bool>(0)).unwrap_or(false);
    let list_install = match (esde, esde_data_dir(&install.root)) {
        (true, Some(data)) => customizer::Install { id: install.id.clone(), root_text: data.display().to_string(), root: data, stable: None },
        _ => customizer::Install { id: install.id.clone(), root: install.root.clone(), root_text: install.root_text.clone(), stable: install.stable.clone() },
    };
    if edits.is_empty() {
        return Err("There's no art or detail to add: the games RetroDrive copied already have them, or their images aren't available.".into());
    }
    if let Some(expected) = expected {
        let now: BTreeMap<String, String> = edits.iter().map(|e| (e.system.clone(), sha_or_none(e))).collect();
        if &now != expected {
            return Err("The game lists changed since you reviewed this, so nothing was changed. Review it again.".into());
        }
    }
    // 1. Images (new files only), several at a time; any failure stops before the lists are touched.
    let all_copies: Vec<&(PathBuf, PathBuf)> = edits.iter().flat_map(|e| e.copies.iter()).collect();
    let total = all_copies.len();
    let results = parallel_map(&all_copies, "copying", |(src, dst)| if fs::symlink_metadata(dst).is_err() { copy_image(src, dst) } else { Ok(()) }, progress);
    if let Some(Err(e)) = results.into_iter().find(Result::is_err) {
        return Err(format!("{e} No game list was changed; images already copied are new files nothing uses yet."));
    }
    let added: Vec<String> = edits.iter().filter(|e| !e.copies.is_empty()).map(|e| if esde { format!("{} image(s) in ES-DE's downloaded_media/{}", e.copies.len(), e.system) } else { format!("{} image(s) in {prefix}{}/images", e.copies.len(), e.system) }).collect();
    // 2. Game lists, journaled; if one fails, the ones already written are undone.
    let mut ids = Vec::new();
    for e in &edits {
        if e.new_text == e.old_text {
            continue; // ES-DE: pictures only, the list already had everything.
        }
        let relative = if esde { format!("gamelists/{}/gamelist.xml", e.system) } else { format!("{prefix}{}/gamelist.xml", e.system) };
        if esde {
            copier::ensure_folders(&list_install.root, &relative).map_err(|e| format!("{e} No game list was changed."))?;
        }
        let s = &e.summary;
        let rows = vec![ChangeRow { system: e.system.clone(), key: "gamelist".into(), before: None, after: Some(format!("{} game(s) updated, {} added, {} image(s)", s.games_updated, s.games_added, s.images)) }];
        match customizer::write_file(connection, &list_install, &relative, &e.list_path, &e.old_text, &e.new_text, rows) {
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
    fn quiet(_: &str, _: usize, _: usize) {}

    #[test]
    fn checks_run_in_parallel_keep_order_and_report_progress() {
        let items: Vec<usize> = (0..1000).collect();
        let last = std::sync::Mutex::new((String::new(), 0, 0));
        let threads = std::sync::Mutex::new(std::collections::HashSet::new());
        let out = parallel_map(&items, "checking", |i| {
            threads.lock().unwrap().insert(std::thread::current().id());
            std::thread::sleep(std::time::Duration::from_micros(200));
            i * 2
        }, &|phase, done, total| {
            let mut l = last.lock().unwrap();
            if done >= l.1 { *l = (phase.to_string(), done, total); }
        });
        assert_eq!(out, items.iter().map(|i| i * 2).collect::<Vec<_>>(), "results keep the input order");
        assert_eq!(*last.lock().unwrap(), ("checking".to_string(), 1000, 1000));
        assert!(threads.lock().unwrap().len() > 1, "work is spread over several threads");
        assert!(parallel_map(&Vec::<usize>::new(), "checking", |i| *i, &quiet).is_empty());
    }

    const LIST: &str = "<?xml version=\"1.0\"?>\r\n<gameList>\r\n\t<game>\r\n\t\t<path>./Kid.zip</path>\r\n\t\t<name>Kid</name>\r\n\t\t<region>us</region>\r\n\t</game>\r\n\t<game>\r\n\t\t<path>./Scraped.zip</path>\r\n\t\t<name>Scraped Game</name>\r\n\t\t<image>./images/theirs.png</image>\r\n\t</game>\r\n\t<game>\r\n\t\t<path>./Other.zip</path>\r\n\t\t<name>Other</name>\r\n\t</game>\r\n</gameList>\r\n";

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
    }

    /// A library with art (gamelist images) for three NES games, copied to a RetroBat.
    fn setup() -> Setup {
        setup_for("retrobat")
    }

    /// The same, copied to `adapter` ("retrobat", or "batocera": a Batocera game folder).
    fn setup_for(adapter: &str) -> Setup {
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
        let root = if adapter == "recalbox" {
            let share = dir.path().join("recalbox");
            for d in ["roms/nes", "bios", "saves", "system"] {
                fs::create_dir_all(share.join(d)).unwrap();
            }
            fs::write(share.join("system/recalbox.conf"), b"").unwrap();
            share
        } else if adapter == "esde" {
            // A portable ES-DE like the owner's: ES-DE.exe, ES-DE\ (data), ROMs\.
            let program = dir.path().join("ES-DE");
            for d in ["ROMs/nes", "ES-DE/gamelists", "ES-DE/downloaded_media", "ES-DE/settings"] {
                fs::create_dir_all(program.join(d)).unwrap();
            }
            fs::write(program.join("ES-DE.exe"), b"").unwrap();
            fs::write(program.join("ROMs/systems.txt"), b"nes: Nintendo Entertainment System\n").unwrap();
            program.join("ROMs")
        } else if adapter == "retropie" {
            // RetroPie's roms share: the system folders are at the top.
            let roms = dir.path().join("roms");
            for d in ["nes", "snes", "ports"] {
                fs::create_dir_all(roms.join(d)).unwrap();
            }
            roms
        } else if adapter == "batocera" {
            let share = dir.path().join("batocera");
            for d in ["roms/nes", "bios", "system"] {
                fs::create_dir_all(share.join(d)).unwrap();
            }
            fs::write(share.join("system/batocera.conf"), b"").unwrap();
            share
        } else {
            let root = dir.path().join("RetroBat");
            for d in ["emulationstation/.emulationstation", "roms/nes", "bios"] {
                fs::create_dir_all(root.join(d)).unwrap();
            }
            fs::write(root.join("RetroBat.exe"), b"").unwrap();
            fs::write(root.join("emulationstation/.emulationstation/es_settings.cfg"), "<config></config>").unwrap();
            root
        };
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &dir.path().join("library"), &|_: &str| VolumeProbe::unknown()).unwrap();
        let install = installations::add(&catalog, &root, adapter, &|_: &str| VolumeProbe::unknown()).unwrap();
        let roomy = |_: &Path| Some(crate::volumes::VolumeSpace { free_bytes: 500 << 30, total_bytes: 1 << 40, filesystem: Some("NTFS".into()), cluster_bytes: 4096 });
        let choices = crate::deployment::Choices::new();
        let plan = crate::deployment::plan_with(&catalog, &install, None, None, &choices, &connected, &roomy).unwrap();
        struct Quiet;
        impl copier::DeployObserver for Quiet {
            fn workers(&self) -> usize {
                1
            }
        }
        copier::deploy(&catalog, None, &copier::DeployRequest { installation_id: &install, systems: None, fingerprint: &plan.fingerprint, choices: &choices, collection: None, sync: false }, &connected, &roomy, &mut Quiet).unwrap();
        // The catalog knows Kid's proper title and details.
        catalog.execute("INSERT INTO reference_catalogs (id, name, file_name, entries, genres_added) VALUES ('c', 'test', 't.csv', 1, 0)", []).unwrap();
        catalog
            .execute("INSERT INTO reference_entries (id, catalog_id, entry_key, title, platform, platform_label, year, genre, publisher, developer) VALUES (1, 'c', 'NES-kid', 'Kid Icarus', 'NES', 'NES', 1986, 'Platform', 'Nintendo', 'Nintendo R&D1')", [])
            .unwrap();
        catalog.execute("INSERT INTO game_reference (game_id, entry_id, method) SELECT g.id, 1, 'name' FROM games g WHERE g.canonical_title = 'Kid'", []).unwrap();
        let root = crate::paths::normalize(&root).unwrap();
        fs::write(games_dir(&root).0.join("nes/gamelist.xml"), LIST).unwrap();
        Setup { _dir: dir, root, catalog, install }
    }

    fn list(s: &Setup) -> String {
        fs::read_to_string(games_dir(&s.root).0.join("nes/gamelist.xml")).unwrap()
    }

    #[test]
    fn art_and_details_fill_gaps_and_keep_retrobats_own() {
        let s = setup();
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert_eq!(p.systems.len(), 1);
        let nes = &p.systems[0];
        assert_eq!((nes.games_updated, nes.games_added, nes.kept), (1, 1, 1), "Kid updated, New Game added, Scraped kept");
        assert_eq!(p.images, 3, "Kid's image and logo, New Game's image");
        assert_eq!(list(&s), LIST, "previewing changes nothing");

        assert!(apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &open, &quiet).unwrap_err().contains("running"));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
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
        assert!(preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap().nothing);
        let app = crate::designs::applications(&s.catalog, &s.install).unwrap()[0].id;
        crate::designs::undo(&s.catalog, app, &connected, &closed).unwrap();
        assert_eq!(list(&s), LIST);
        assert!(s.root.join("roms/nes/images/Kid-image.png").exists(), "copied images are kept");
    }

    #[test]
    fn batocera_game_lists_get_the_same_art_and_details() {
        // Batocera uses the same gamelist.xml and ./images/ convention (2026-11-07).
        let s = setup_for("batocera");
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert_eq!((p.systems[0].games_updated, p.systems[0].games_added, p.images), (1, 1, 3));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
        let after = list(&s);
        assert!(after.contains("<name>Kid Icarus</name>") && after.contains("<image>./images/Kid-image.png</image>"));
        assert!(after.contains("<image>./images/theirs.png</image>"), "art Batocera already shows is kept");
        assert_eq!(fs::read(s.root.join("roms/nes/images/Kid-image.png")).unwrap(), b"art of Kid");
        let app = crate::designs::applications(&s.catalog, &s.install).unwrap()[0].id;
        crate::designs::undo(&s.catalog, app, &connected, &closed).unwrap();
        assert_eq!(list(&s), LIST, "undo restores Batocera's list exactly");
        // Emulator settings stay RetroBat-only.
        assert!(crate::customizer::install(&s.catalog, &s.install, &connected).is_err());
    }

    #[test]
    fn retropie_lists_are_filled_in_and_only_created_when_asked() {
        // RetroPie uses a rom-folder gamelist.xml instead of its home-folder list, so a
        // missing one is only created on request (docs/18 §27).
        let s = setup_for("retropie");
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert_eq!((p.systems[0].games_updated, p.systems[0].games_added, p.images), (1, 1, 3), "an existing list is filled in");
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
        assert!(list(&s).contains("<name>Kid Icarus</name>") && list(&s).contains("<image>./images/Kid-image.png</image>"));
        assert!(s.root.join("nes/images/Kid-image.png").exists(), "images next to the list, no roms level");
        // Without a list: nothing is created unless asked.
        let s = setup_for("retropie");
        fs::remove_file(s.root.join("nes/gamelist.xml")).unwrap();
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert!(p.nothing && p.lists_not_created == ["nes"], "{:?}", p.lists_not_created);
        assert!(apply(&s.catalog, &s.install, None, None, false, &connected, &closed, &quiet).is_err());
        assert!(!s.root.join("nes/gamelist.xml").exists(), "never created without asking");
        let asked = preview(&s.catalog, &s.install, None, true, &connected, &quiet).unwrap();
        assert_eq!((asked.systems[0].games_added, asked.lists_not_created.len()), (3, 0));
        apply(&s.catalog, &s.install, None, Some(&asked.list_shas), true, &connected, &closed, &quiet).unwrap();
        assert!(list(&s).contains("<path>./Kid.zip</path>"));
    }

    #[test]
    fn esde_gets_details_in_its_own_list_and_pictures_named_after_each_game() {
        let s = setup_for("esde");
        let data = s.root.parent().unwrap().join("ES-DE");
        assert_eq!(esde_data_dir(&s.root).as_deref(), Some(data.as_path()));
        // ES-DE already has a cover for Scraped (as .jpg): it's kept.
        fs::create_dir_all(data.join("downloaded_media/nes/covers")).unwrap();
        fs::write(data.join("downloaded_media/nes/covers/Scraped.jpg"), b"theirs").unwrap();
        let rom_list = fs::read_to_string(s.root.join("nes/gamelist.xml")).unwrap();
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        let nes = &p.systems[0];
        assert_eq!((nes.games_added, nes.games_updated, nes.images, nes.kept), (1, 1, 3, 1), "Kid listed with details; New Game's cover only; Scraped's cover kept");
        assert!(apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &open, &quiet).unwrap_err().contains("running"), "not while ES-DE runs");
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
        let list = fs::read_to_string(data.join("gamelists/nes/gamelist.xml")).unwrap();
        assert!(list.contains("<path>./Kid.zip</path>") && list.contains("<name>Kid Icarus</name>") && list.contains("<genre>Platform</genre>"));
        assert!(!list.contains("<image>"), "ES-DE finds pictures by name, not by tags");
        assert_eq!(fs::read(data.join("downloaded_media/nes/covers/Kid.png")).unwrap(), b"art of Kid");
        assert_eq!(fs::read(data.join("downloaded_media/nes/marquees/Kid.png")).unwrap(), b"kid logo");
        assert_eq!(fs::read(data.join("downloaded_media/nes/covers/New Game.png")).unwrap(), b"art of New Game");
        assert_eq!(fs::read(data.join("downloaded_media/nes/covers/Scraped.jpg")).unwrap(), b"theirs");
        assert_eq!(fs::read_to_string(s.root.join("nes/gamelist.xml")).unwrap(), rom_list, "nothing written in the ROMs folder");
        // Undo puts ES-DE's list back (it didn't exist); pictures stay.
        let app = crate::designs::applications(&s.catalog, &s.install).unwrap()[0].id;
        crate::designs::undo(&s.catalog, app, &connected, &closed).unwrap();
        assert!(!data.join("gamelists/nes/gamelist.xml").exists());
        assert!(data.join("downloaded_media/nes/covers/Kid.png").exists());
    }

    #[test]
    fn recalbox_game_lists_get_the_same_art_and_details() {
        let s = setup_for("recalbox");
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert_eq!((p.systems[0].games_updated, p.systems[0].games_added, p.images), (1, 1, 3));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
        assert!(list(&s).contains("<name>Kid Icarus</name>") && list(&s).contains("<image>./images/Kid-image.png</image>"));
        assert!(s.root.join("roms/nes/images/Kid-image.png").exists());
        let app = crate::designs::applications(&s.catalog, &s.install).unwrap()[0].id;
        crate::designs::undo(&s.catalog, app, &connected, &closed).unwrap();
        assert_eq!(list(&s), LIST);
    }

    #[test]
    fn a_missing_list_is_created_and_a_stale_review_is_refused() {
        let s = setup();
        fs::remove_file(s.root.join("roms/nes/gamelist.xml")).unwrap();
        let p = preview(&s.catalog, &s.install, None, false, &connected, &quiet).unwrap();
        assert_eq!(p.list_shas.get("nes").map(String::as_str), Some("none"));
        assert_eq!(p.systems[0].games_added, 3);
        let mut stale = p.list_shas.clone();
        stale.insert("nes".into(), "different".into());
        assert!(apply(&s.catalog, &s.install, None, Some(&stale), false, &connected, &closed, &quiet).unwrap_err().contains("changed since"));
        apply(&s.catalog, &s.install, None, Some(&p.list_shas), false, &connected, &closed, &quiet).unwrap();
        let created = list(&s);
        assert!(created.starts_with("<?xml") && created.matches("<game>").count() == 3);
        // An image name taken by someone else's different file is left alone.
        let s2 = setup();
        fs::create_dir_all(s2.root.join("roms/nes/images")).unwrap();
        fs::write(s2.root.join("roms/nes/images/Kid-image.png"), b"someone else's").unwrap();
        apply(&s2.catalog, &s2.install, None, None, false, &connected, &closed, &quiet).unwrap();
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
