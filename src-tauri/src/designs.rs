//! Designs: named RetroBat customizations made in a studio, then applied as one
//! reviewed, undoable step (owner request 2026-11-06: "design, then pick finish
//! design or abandon … then it becomes selectable in another screen"; docs/18 §15).
//!
//! A design is kept in RetroDrive's catalog only. Editing never touches RetroBat:
//! edits go to the design's draft; *Finish* makes the draft the design, *Abandon*
//! throws the draft away (a design that was never finished disappears). Applying a
//! finished design to an installation:
//!
//! 1. checks everything first (RetroBat closed, both settings files unchanged since
//!    the preview, every value one the install offers, every file to add valid);
//! 2. adds files: the remixed theme (a new folder, see `themes.rs`), the user's own
//!    boot video and music (new files; an identical file already there is reused);
//! 3. writes es_settings.cfg, then retrobat.ini, each journaled and backed up
//!    (`customizer::write_file`); if the second write fails the first is undone;
//! 4. records the application, which is undone in one step (both files, newest first).
//!
//! Settings a design doesn't mention are left as they are; `null` means AUTO.

use crate::copier;
use crate::customizer::{self, Change, ChangeRow, IniChange, PresenceFn, RunningFn};
use crate::{db, themes};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Music settings in es_settings.cfg (names found in RetroBat's emulationstation.exe
/// and in batocera-emulationstation's Settings.cpp/AudioManager.cpp, 2026-11-06),
/// with their element type and allowed values.
pub const MUSIC_SETTINGS: [(&str, &str, &str); 8] = [
    ("audio.bgmusic", "bool", "Frontend music (play music in the menus)"),
    ("audio.persystem", "bool", "Only play system-specific music folder"),
    ("audio.thememusics", "bool", "Play system-specific music (from the theme)"),
    ("audio.display_titles", "bool", "Show song titles"),
    ("audio.display_titles_time", "int", "How long titles show (seconds)"),
    ("MusicVolume", "int", "Music volume (%)"),
    ("StopMusicOnScreenSaver", "bool", "Stop music in the screensaver"),
    ("VideoLowersMusic", "bool", "Lower music while game videos play"),
];

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Use a theme already on the drive.
    Existing { folder: String },
    /// Remix a theme into a new one (created when the design is applied).
    Remix {
        base: String,
        name: String,
        #[serde(default)]
        colors: BTreeMap<String, String>,
        #[serde(default)]
        images: BTreeMap<String, String>,
        #[serde(default)]
        fonts: BTreeMap<String, String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OwnMusic {
    pub path: String,
    /// `None`: the main music folder; `Some(system)`: `music\<system>` (with "music per system").
    #[serde(default)]
    pub system: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct DesignSpec {
    #[serde(default)]
    pub theme: Option<ThemeChoice>,
    /// retrobat.ini `Section.Key` -> value (boot video and behavior).
    #[serde(default)]
    pub ini: BTreeMap<String, String>,
    /// The user's own boot video (copied when applied; becomes `SplashScreen.FileName`).
    #[serde(default)]
    pub own_video: Option<String>,
    /// es_settings.cfg music settings -> value.
    #[serde(default)]
    pub music: BTreeMap<String, String>,
    #[serde(default)]
    pub own_music: Vec<OwnMusic>,
    /// Per-system settings `<system>.<key>` -> value (`None` = AUTO).
    #[serde(default)]
    pub systems: BTreeMap<String, Option<String>>,
    /// Only these tracks play (relative to the music folder, e.g. `snes/Song.ogg`),
    /// through RetroBat's favorites playlist. `None`: leave as is.
    #[serde(default)]
    pub play_tracks: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct Design {
    pub id: String,
    pub name: String,
    pub platform: String,
    /// The finished design (`None` until finished once).
    pub spec: Option<DesignSpec>,
    /// Unfinished edits (`None` when not being edited).
    pub draft: Option<DesignSpec>,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

fn parse(json: Option<String>) -> Option<DesignSpec> {
    json.and_then(|j| serde_json::from_str(&j).ok())
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 60 {
        return Err("Give the design a name (up to 60 characters).".into());
    }
    Ok(name)
}

pub fn list(connection: &Connection) -> Result<Vec<Design>, String> {
    let mut statement = connection
        .prepare("SELECT id, name, platform, spec_json, draft_json, updated_at, finished_at FROM designs ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok(Design { id: r.get(0)?, name: r.get(1)?, platform: r.get(2)?, spec: parse(r.get(3)?), draft: parse(r.get(4)?), updated_at: r.get(5)?, finished_at: r.get(6)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn name_taken(connection: &Connection, name: &str, except: Option<&str>) -> Result<bool, String> {
    connection
        .query_row("SELECT EXISTS (SELECT 1 FROM designs WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2)", params![name, except], |r| r.get(0))
        .map_err(|e| e.to_string())
}

/// Start a new design (a draft). `seed` is a starting point, e.g. an installation's current setup.
pub fn create(connection: &Connection, name: &str, seed: DesignSpec) -> Result<String, String> {
    let name = clean_name(name)?;
    if name_taken(connection, &name, None)? {
        return Err(format!("You already have a design called \"{name}\"."));
    }
    let id = db::new_id();
    connection
        .execute("INSERT INTO designs (id, name, platform, draft_json) VALUES (?1, ?2, 'retrobat', ?3)", params![id, name, serde_json::to_string(&seed).map_err(|e| e.to_string())?])
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Save edits to the draft (starting one from the finished design if needed). Never touches RetroBat.
pub fn save_draft(connection: &Connection, id: &str, name: &str, draft: &DesignSpec) -> Result<(), String> {
    let name = clean_name(name)?;
    if name_taken(connection, &name, Some(id))? {
        return Err(format!("You already have a design called \"{name}\"."));
    }
    let n = connection
        .execute(
            "UPDATE designs SET name = ?2, draft_json = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, name, serde_json::to_string(draft).map_err(|e| e.to_string())?],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("That design wasn't found.".into());
    }
    Ok(())
}

/// Checks that don't need an installation (names, file types, value shapes).
fn check_shape(spec: &DesignSpec) -> Result<(), String> {
    if let Some(ThemeChoice::Remix { name, colors, .. }) = &spec.theme {
        themes::folder_for(name)?;
        if colors.values().any(|v| !((v.len() == 6 || v.len() == 8) && v.chars().all(|c| c.is_ascii_hexdigit()))) {
            return Err("Theme colors must be RRGGBB or RRGGBBAA.".into());
        }
    }
    if let Some(tracks) = &spec.play_tracks {
        if tracks.is_empty() {
            return Err("Choose at least one track to play, or turn menu music off.".into());
        }
        if tracks.iter().any(|t| t.contains("..") || t.contains(['\\', ':', ';', '\n', '\r']) || t.starts_with('/')) {
            return Err("A track name isn't valid.".into());
        }
    }
    if let Some(video) = &spec.own_video {
        if !video.to_lowercase().ends_with(".mp4") {
            return Err("The boot video must be an .mp4 file.".into());
        }
    }
    for m in &spec.own_music {
        let l = m.path.to_lowercase();
        if !(l.ends_with(".ogg") || l.ends_with(".mp3")) {
            return Err(format!("{} isn't .ogg or .mp3.", m.path));
        }
    }
    for (key, value) in &spec.music {
        let (_, kind, _) = MUSIC_SETTINGS.iter().find(|(k, _, _)| k == key).ok_or(format!("RetroDrive doesn't change {key}."))?;
        let ok = match *kind {
            "bool" => value == "true" || value == "false",
            _ => value.parse::<u32>().is_ok_and(|n| n <= if key == "MusicVolume" { 100 } else { 60 }),
        };
        if !ok {
            return Err(format!("{value} isn't valid for {key}."));
        }
    }
    Ok(())
}

/// Make the draft the design.
pub fn finish(connection: &Connection, id: &str) -> Result<(), String> {
    let draft: Option<String> = connection.query_row("SELECT draft_json FROM designs WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?.ok_or("That design wasn't found.")?;
    let spec = parse(draft).ok_or("There's nothing to finish.")?;
    check_shape(&spec)?;
    connection
        .execute("UPDATE designs SET spec_json = draft_json, draft_json = NULL, finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Throw away the draft. A design that was never finished is removed. Returns whether the design still exists.
pub fn abandon(connection: &Connection, id: &str) -> Result<bool, String> {
    let finished: Option<Option<String>> = connection.query_row("SELECT spec_json FROM designs WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    match finished {
        None => Err("That design wasn't found.".into()),
        Some(None) => {
            connection.execute("DELETE FROM designs WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
            Ok(false)
        }
        Some(Some(_)) => {
            connection.execute("UPDATE designs SET draft_json = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
            Ok(true)
        }
    }
}

/// Delete a design from RetroDrive (nothing on any drive changes).
pub fn delete(connection: &Connection, id: &str) -> Result<bool, String> {
    connection.execute("DELETE FROM designs WHERE id = ?1", [id]).map(|n| n > 0).map_err(|e| e.to_string())
}

/// A starting point from an installation's current setup (reads only).
pub fn seed_from(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<DesignSpec, String> {
    let install = customizer::install(connection, installation_id, presence)?;
    let es = customizer::values(&customizer::entries(&customizer::read_text(&install.root.join(customizer::SETTINGS))?)?)?;
    let ini = customizer::ini_entries(&customizer::read_text(&install.root.join(customizer::INI))?);
    let on_drive = customizer::theme_folders(&install.root);
    let theme = es.get("ThemeSet").filter(|f| on_drive.contains(f)).map(|f| ThemeChoice::Existing { folder: f.clone() });
    let mut spec = DesignSpec { theme, ..DesignSpec::default() };
    for o in customizer::ini_options() {
        let key = format!("{}.{}", o.section, o.key);
        if let Some((value, _, _)) = ini.get(&key) {
            spec.ini.insert(key, value.clone());
        }
    }
    for (key, _, _) in MUSIC_SETTINGS {
        if let Some(v) = es.get(key) {
            spec.music.insert(key.to_string(), v.clone());
        }
    }
    if es.get("audio.useFavoriteMusic").map(String::as_str) == Some("true") {
        spec.play_tracks = customizer::favorite_tracks(&install.root, &install.root_text).filter(|t| !t.is_empty());
    }
    for (name, value) in &es {
        if let Some((_, key)) = name.rsplit_once('.') {
            if customizer::KEYS.contains(&key) {
                spec.systems.insert(name.clone(), Some(value.clone()));
            }
        }
    }
    Ok(spec)
}

#[derive(Debug, Serialize)]
pub struct MusicSetting {
    pub key: &'static str,
    pub kind: &'static str,
    pub label: &'static str,
    /// The value on this RetroBat (`None`: RetroBat's default).
    pub current: Option<String>,
}

/// The music settings offered, with this RetroBat's current values (reads only).
pub fn music_settings(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Vec<MusicSetting>, String> {
    let install = customizer::install(connection, installation_id, presence)?;
    let es = customizer::values(&customizer::entries(&customizer::read_text(&install.root.join(customizer::SETTINGS))?)?)?;
    let mut list: Vec<MusicSetting> = MUSIC_SETTINGS.iter().map(|(key, kind, label)| MusicSetting { key, kind, label, current: es.get(*key).cloned() }).collect();
    list.push(MusicSetting { key: "audio.useFavoriteMusic", kind: "bool", label: "Play only songs from your favorites playlist", current: es.get("audio.useFavoriteMusic").cloned() });
    Ok(list)
}

// ---------- applying ----------

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct FilePlan {
    pub name: String,
    /// Where it goes, relative to the installation.
    pub folder: String,
    /// `add` or `already_there` (an identical file is reused).
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct DesignPreview {
    pub design: String,
    /// `create` (a new remixed theme), `reuse` (made by this design before) or `existing`.
    pub theme_action: Option<String>,
    pub theme_folder: Option<String>,
    pub theme_bytes: i64,
    pub files: Vec<FilePlan>,
    pub es_changes: Vec<ChangeRow>,
    pub ini_changes: Vec<ChangeRow>,
    pub es_sha: String,
    pub ini_sha: String,
    /// Tracks that will play / tracks left out, when the design picks tracks.
    pub tracks_playing: Option<usize>,
    pub tracks_left_out: Option<usize>,
    /// True when applying would change nothing at all.
    pub nothing: bool,
}

struct Plan {
    preview: DesignPreview,
    remix: Option<themes::Remix>,
    copies: Vec<(PathBuf, String, &'static [&'static str], &'static str)>,
    es_wanted: BTreeMap<String, Option<String>>,
    es_tags: BTreeMap<String, &'static str>,
    es_current: BTreeMap<String, String>,
    es_text: String,
    ini_wanted: BTreeMap<String, String>,
    ini_rows: Vec<ChangeRow>,
    ini_text: String,
    /// New favorites.m3u text and the current one ("" when there's none).
    favorites: Option<(String, String)>,
}

fn finished_spec(connection: &Connection, id: &str) -> Result<(String, DesignSpec), String> {
    let (name, spec): (String, Option<String>) = connection.query_row("SELECT name, spec_json FROM designs WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|e| e.to_string())?.ok_or("That design wasn't found.")?;
    Ok((name, parse(spec).ok_or("Finish the design before applying it.")?))
}

fn file_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("file").chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect()
}

/// `add`, `already_there` (identical file), or an error when a different file has that name.
fn file_action(source: &Path, target: &Path) -> Result<&'static str, String> {
    if !source.is_file() {
        return Err(format!("{} wasn't found.", source.display()));
    }
    match fs::symlink_metadata(target) {
        Err(_) => Ok("add"),
        Ok(_) if copier::sha256_of(source).ok() == copier::sha256_of(target).ok() => Ok("already_there"),
        Ok(_) => Err(format!("A different file called {} is already in {}; rename yours in the design.", file_name(source), target.parent().map(|p| p.display().to_string()).unwrap_or_default())),
    }
}

fn plan(connection: &Connection, design_id: &str, installation_id: &str, presence: PresenceFn) -> Result<Plan, String> {
    let (design_name, spec) = finished_spec(connection, design_id)?;
    check_shape(&spec)?;
    let install = customizer::install(connection, installation_id, presence)?;
    let es_text = customizer::read_text(&install.root.join(customizer::SETTINGS))?;
    let es_current = customizer::values(&customizer::entries(&es_text)?)?;
    let ini_text = customizer::read_text(&install.root.join(customizer::INI))?;
    let offer = customizer::offer(&install.root)?;
    let mut es_wanted: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut es_tags: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut files = Vec::new();
    let mut copies = Vec::new();

    // Theme.
    let (mut theme_action, mut theme_folder, mut theme_bytes, mut remix) = (None, None, 0, None);
    let themes_on_drive = customizer::theme_folders(&install.root);
    match &spec.theme {
        None => {}
        Some(ThemeChoice::Existing { folder }) => {
            if !themes_on_drive.contains(folder) {
                return Err(format!("The theme {folder} isn't on this RetroBat."));
            }
            theme_action = Some("existing".to_string());
            theme_folder = Some(folder.clone());
        }
        Some(ThemeChoice::Remix { base, name, colors, images, fonts }) => {
            let r = themes::Remix { base: base.clone(), name: name.clone(), colors: colors.clone(), images: images.clone(), fonts: fonts.clone(), design: Some(design_id.to_string()) };
            let folder = themes::folder_for(name)?;
            if !themes_on_drive.contains(base) {
                return Err(format!("The design remixes {base}, which isn't on this RetroBat."));
            }
            let existing = themes_on_drive.iter().find(|t| t.eq_ignore_ascii_case(&folder)).cloned();
            match existing {
                Some(found) => {
                    let dir = install.root.join("emulationstation/.emulationstation/themes").join(&found);
                    match themes::made_by(&dir) {
                        Some((d, recipe)) if d == design_id && recipe == r.recipe() => theme_action = Some("reuse".to_string()),
                        _ => return Err(format!("A different theme called {found} is already on this RetroBat; rename the theme in the design.")),
                    }
                    theme_folder = Some(found);
                }
                None => {
                    theme_action = Some("create".to_string());
                    theme_bytes = themes::theme_bytes(&install.root.join("emulationstation/.emulationstation/themes").join(base));
                    theme_folder = Some(folder);
                    remix = Some(r);
                }
            }
        }
    }
    if let Some(folder) = &theme_folder {
        es_wanted.insert("ThemeSet".into(), Some(folder.clone()));
    }

    // Per-system settings.
    let changes: Vec<Change> = spec
        .systems
        .iter()
        // Values that already match this RetroBat aren't changes, so they aren't re-checked.
        .filter(|(k, v)| es_current.get(*k) != v.as_ref())
        .filter_map(|(k, v)| k.rsplit_once('.').map(|(system, key)| Change { system: system.to_string(), key: key.to_string(), value: v.clone() }))
        .collect();
    if !changes.is_empty() {
        es_wanted.extend(customizer::validate_systems(&changes, &offer, &es_current)?);
    }

    // Music settings (typed) and own music.
    for (key, value) in &spec.music {
        let kind = MUSIC_SETTINGS.iter().find(|(k, _, _)| k == key).map(|m| m.1).unwrap_or("string");
        es_tags.insert(key.clone(), if kind == "bool" { "bool" } else { "int" });
        es_wanted.insert(key.clone(), Some(value.clone()));
    }
    for m in &spec.own_music {
        let folder = match &m.system {
            None => customizer::MUSIC.to_string(),
            Some(system) if offer.systems.iter().any(|(s, _, _)| s == system) => format!("{}/{system}", customizer::MUSIC),
            Some(system) => return Err(format!("{system} isn't a system in this RetroBat.")),
        };
        let source = PathBuf::from(&m.path);
        let name = file_name(&source);
        let action = file_action(&source, &crate::paths::join(&install.root_text, &format!("{folder}/{name}")))?;
        files.push(FilePlan { name: name.clone(), folder: folder.clone(), action: action.into() });
        if action == "add" {
            copies.push((source, folder, &["ogg", "mp3"][..], "Music must be .ogg or .mp3 files."));
        }
    }
    // Which tracks play: RetroBat's favorites playlist, plus "play only favorites".
    let (mut tracks_playing, mut tracks_left_out, mut favorites) = (None, None, None);
    if let Some(chosen) = &spec.play_tracks {
        let mut available = customizer::music_tracks(&install.root);
        for f in files.iter().filter(|f| f.folder.starts_with(customizer::MUSIC)) {
            let sub = f.folder[customizer::MUSIC.len()..].trim_start_matches('/');
            available.push(if sub.is_empty() { f.name.clone() } else { format!("{sub}/{}", f.name) });
        }
        if let Some(missing) = chosen.iter().find(|t| !available.iter().any(|a| a.eq_ignore_ascii_case(t))) {
            return Err(format!("The design plays {missing}, which isn't on this RetroBat."));
        }
        let text = customizer::favorites_text(&install.root_text, chosen);
        let current = fs::read_to_string(install.root.join(customizer::FAVORITES)).unwrap_or_default();
        tracks_playing = Some(chosen.len());
        tracks_left_out = Some(available.len().saturating_sub(chosen.len()));
        if text != current {
            favorites = Some((text, current));
        }
        es_tags.insert("audio.useFavoriteMusic".into(), "bool");
        es_wanted.insert("audio.useFavoriteMusic".into(), Some("true".into()));
    }
    es_wanted.retain(|k, v| es_current.get(k) != v.as_ref());

    // retrobat.ini: options, plus the own boot video.
    let ini_now = customizer::ini_entries(&ini_text);
    let mut ini_changes: Vec<IniChange> = spec
        .ini
        .iter()
        .filter(|(k, v)| ini_now.get(*k).map(|e| &e.0) != Some(*v))
        .filter_map(|(k, v)| k.split_once('.').map(|(section, key)| IniChange { section: section.into(), key: key.into(), value: v.clone() }))
        .collect();
    let mut extra_videos = Vec::new();
    if let Some(video) = &spec.own_video {
        let source = PathBuf::from(video);
        let name = file_name(&source);
        let action = file_action(&source, &crate::paths::join(&install.root_text, &format!("{}/{name}", customizer::VIDEOS)))?;
        files.push(FilePlan { name: name.clone(), folder: customizer::VIDEOS.into(), action: action.into() });
        if action == "add" {
            copies.push((source, customizer::VIDEOS.to_string(), &["mp4"][..], "Boot videos must be .mp4 files."));
        }
        extra_videos.push(name.clone());
        ini_changes.retain(|c| !(c.section == "SplashScreen" && c.key == "FileName"));
        ini_changes.push(IniChange { section: "SplashScreen".into(), key: "FileName".into(), value: name });
    }
    let (ini_wanted, ini_rows) = customizer::ini_validate(&install, &ini_text, &ini_changes, &extra_videos)?;

    // Check both edits now, so nothing is written unless all of it can be.
    customizer::edit_typed(&es_text, &es_wanted, &es_tags)?;
    if !ini_wanted.is_empty() {
        customizer::ini_edit(&ini_text, &ini_wanted)?;
    }
    let es_changes = customizer::rows(&es_wanted, &es_current);
    let nothing = es_changes.is_empty() && ini_rows.is_empty() && copies.is_empty() && remix.is_none() && favorites.is_none();
    Ok(Plan {
        preview: DesignPreview {
            design: design_name,
            theme_action,
            theme_folder,
            theme_bytes,
            files,
            es_changes,
            ini_changes: ini_rows.clone(),
            es_sha: customizer::hex_sha(es_text.as_bytes()),
            ini_sha: customizer::hex_sha(ini_text.as_bytes()),
            tracks_playing,
            tracks_left_out,
            nothing,
        },
        remix,
        copies,
        es_wanted,
        es_tags,
        es_current,
        es_text,
        ini_wanted,
        ini_rows,
        ini_text,
        favorites,
    })
}

/// Everything applying the design would do (reads only).
pub fn preview(connection: &Connection, design_id: &str, installation_id: &str, presence: PresenceFn) -> Result<DesignPreview, String> {
    plan(connection, design_id, installation_id, presence).map(|p| p.preview)
}

/// Apply a finished design to an installation (see the module docs for the order).
#[allow(clippy::too_many_arguments)]
pub fn apply(connection: &Connection, design_id: &str, installation_id: &str, es_sha: &str, ini_sha: &str, presence: PresenceFn, running: RunningFn, free_space: themes::SpaceFn) -> Result<String, String> {
    if let Some(reason) = customizer::running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let p = plan(connection, design_id, installation_id, presence)?;
    if p.preview.es_sha != es_sha || p.preview.ini_sha != ini_sha {
        return Err("RetroBat's settings changed since you reviewed the design, so nothing was changed. Review it again.".into());
    }
    if p.preview.nothing {
        return Err("This RetroBat already looks like this design.".into());
    }
    let install = customizer::install(connection, installation_id, presence)?;
    let mut added: Vec<String> = Vec::new();
    // 1. Files (additions only).
    if let Some(remix) = &p.remix {
        let folder = themes::create(connection, installation_id, remix, presence, free_space)?;
        added.push(format!("theme {folder}"));
    }
    for (source, folder, exts, wrong) in &p.copies {
        let name = customizer::add_file(connection, installation_id, &source.display().to_string(), presence, folder, exts, wrong)?;
        added.push(format!("{folder}/{name}"));
    }
    // 2. Settings files, es_settings.cfg first; undo it if retrobat.ini can't follow.
    let mut ids = Vec::new();
    if !p.es_wanted.is_empty() {
        let new_text = customizer::edit_typed(&p.es_text, &p.es_wanted, &p.es_tags)?;
        let (_, id) = customizer::write_file(connection, &install, customizer::SETTINGS, &install.root.join(customizer::SETTINGS.replace('/', "\\")), &p.es_text, &new_text, customizer::rows(&p.es_wanted, &p.es_current))?;
        ids.push(id);
    }
    let roll_back = |ids: &[i64], e: String| -> String {
        let all = ids.iter().rev().all(|id| customizer::undo(connection, *id, presence, running).is_ok());
        format!("{e} {}", if all { "The settings were put back as they were." } else { "Some settings were changed; undo them from Changes made." })
    };
    if !p.ini_wanted.is_empty() {
        let new_text = customizer::ini_edit(&p.ini_text, &p.ini_wanted)?;
        match customizer::write_file(connection, &install, customizer::INI, &install.root.join(customizer::INI), &p.ini_text, &new_text, p.ini_rows.clone()) {
            Ok((_, id)) => ids.push(id),
            Err(e) => return Err(roll_back(&ids, e)),
        }
    }
    if let Some((text, current)) = &p.favorites {
        let path = install.root.join(customizer::FAVORITES.replace('/', "\\"));
        let rows = vec![ChangeRow { system: String::new(), key: "favorites".into(), before: None, after: Some(format!("{} track(s)", p.preview.tracks_playing.unwrap_or(0))) }];
        match customizer::write_file(connection, &install, customizer::FAVORITES, &path, current, text, rows) {
            Ok((_, id)) => ids.push(id),
            Err(e) => return Err(roll_back(&ids, e)),
        }
    }
    connection
        .execute(
            "INSERT INTO design_applications (design_id, design_name, installation_id, settings_change_ids, added_json, state) VALUES (?1, ?2, ?3, ?4, ?5, 'applied')",
            params![design_id, p.preview.design, installation_id, serde_json::to_string(&ids).unwrap_or_default(), serde_json::to_string(&added).unwrap_or_default()],
        )
        .map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" applied.{}", p.preview.design, if added.is_empty() { String::new() } else { format!(" Added: {}.", added.join(", ")) }))
}

#[derive(Debug, Serialize)]
pub struct Application {
    pub id: i64,
    pub design_id: Option<String>,
    pub design_name: String,
    pub state: String,
    pub added: Vec<String>,
    pub when: String,
}

pub fn applications(connection: &Connection, installation_id: &str) -> Result<Vec<Application>, String> {
    let mut statement = connection
        .prepare("SELECT id, design_id, design_name, state, added_json, created_at FROM design_applications WHERE installation_id = ?1 ORDER BY id DESC")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([installation_id], |r| {
            Ok(Application { id: r.get(0)?, design_id: r.get(1)?, design_name: r.get(2)?, state: r.get(3)?, added: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(), when: r.get(5)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Undo a design application: its settings changes, newest first. Added files stay (never deleted).
pub fn undo(connection: &Connection, application_id: i64, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let (ids, state, name): (String, String, String) = connection
        .query_row("SELECT settings_change_ids, state, design_name FROM design_applications WHERE id = ?1", [application_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("That application wasn't found.")?;
    if state == "undone" {
        return Err("Already undone.".into());
    }
    let ids: Vec<i64> = serde_json::from_str(&ids).unwrap_or_default();
    let mut done = 0;
    for id in ids.iter().rev() {
        let st: String = connection.query_row("SELECT state FROM settings_changes WHERE id = ?1", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
        if st == "undone" {
            done += 1;
            continue;
        }
        if let Err(e) = customizer::undo(connection, *id, presence, running) {
            connection.execute("UPDATE design_applications SET state = ?2 WHERE id = ?1", params![application_id, if done > 0 { "partial" } else { "applied" }]).map_err(|e| e.to_string())?;
            return Err(e);
        }
        done += 1;
    }
    connection.execute("UPDATE design_applications SET state = 'undone' WHERE id = ?1", [application_id]).map_err(|e| e.to_string())?;
    Ok(format!("\"{name}\" undone: RetroBat's previous settings are back. Added theme and media files were kept on the drive."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::installations;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn closed() -> bool {
        false
    }
    fn open() -> bool {
        true
    }
    fn roomy(_: &Path) -> Option<u64> {
        Some(1 << 40)
    }

    const ES: &str = "<?xml version=\"1.0\"?>\n<config>\n\t<bool name=\"FavoritesFirst\" value=\"true\" />\n\t<string name=\"ThemeSet\" value=\"es-theme-test\" />\n\t<string name=\"nes.shaderset\" value=\"vhs\" />\n</config>\n";
    const INI: &str = "[SplashScreen]\nEnableIntro=1\nFileName=\"retrobat-neon.mp4\"\nFilePath=\"default\"\nRandomVideo=1\n\n[EmulationStation]\nInterfaceMode=0\n";
    const SYSTEMS: &str = "<systemList><system><name>snes</name><fullname>SNES</fullname><emulators><emulator name=\"libretro\"><cores><core>snes9x</core><core>bsnes</core></cores></emulator></emulators></system><system><name>nes</name><fullname>NES</fullname><emulators><emulator name=\"libretro\"><cores><core>fceumm</core></cores></emulator></emulators></system></systemList>";
    const THEME: &str = "<?xml version=\"1.0\"?>\n<!-- credits -->\n<theme>\n  <variables>\n    <baseColor>8b0000</baseColor>\n  </variables>\n</theme>\n";

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
        mine: PathBuf,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        let es = root.join("emulationstation/.emulationstation");
        for d in [es.join("themes/es-theme-test"), es.join("video"), es.join("music"), root.join("roms/snes"), root.join("system/shaders/configs/vhs")] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(es.join("es_settings.cfg"), ES).unwrap();
        fs::write(es.join("es_systems.cfg"), SYSTEMS).unwrap();
        fs::write(es.join("es_features.cfg"), "<features/>").unwrap();
        fs::write(root.join("retrobat.ini"), INI).unwrap();
        fs::write(es.join("themes/es-theme-test/theme.xml"), THEME).unwrap();
        fs::write(es.join("video/retrobat-neon.mp4"), b"neon").unwrap();
        let mine = dir.path().join("mine");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("Intro.mp4"), b"my intro").unwrap();
        fs::write(mine.join("Boss.ogg"), b"boss").unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let install = installations::add(&catalog, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let root = crate::paths::normalize(&root).unwrap();
        Setup { _dir: dir, root, catalog, install, mine }
    }

    fn read(s: &Setup, rel: &str) -> String {
        fs::read_to_string(s.root.join(rel)).unwrap()
    }

    #[test]
    fn drafts_finish_or_are_abandoned_without_touching_retrobat() {
        let s = setup();
        let es_before = read(&s, customizer::SETTINGS);
        let id = create(&s.catalog, "Neon Nights", DesignSpec::default()).unwrap();
        assert!(create(&s.catalog, "neon nights", DesignSpec::default()).is_err(), "names are unique");
        let d = &list(&s.catalog).unwrap()[0];
        assert!(d.spec.is_none() && d.draft.is_some(), "a new design starts as a draft");
        assert!(preview(&s.catalog, &id, &s.install, &connected).unwrap_err().contains("Finish"));
        assert!(!abandon(&s.catalog, &id).unwrap(), "an unfinished design disappears");
        assert!(list(&s.catalog).unwrap().is_empty());

        let id = create(&s.catalog, "Neon Nights", DesignSpec::default()).unwrap();
        let spec = DesignSpec { music: [("audio.bgmusic".to_string(), "false".to_string())].into_iter().collect(), ..DesignSpec::default() };
        save_draft(&s.catalog, &id, "Neon Nights", &spec).unwrap();
        finish(&s.catalog, &id).unwrap();
        let edited = DesignSpec { music: [("audio.bgmusic".to_string(), "true".to_string())].into_iter().collect(), ..DesignSpec::default() };
        save_draft(&s.catalog, &id, "Neon Nights", &edited).unwrap();
        assert!(abandon(&s.catalog, &id).unwrap(), "abandoning edits keeps the finished design");
        let d = &list(&s.catalog).unwrap()[0];
        assert_eq!((d.spec.as_ref(), d.draft.as_ref()), (Some(&spec), None));
        let bad = DesignSpec { music: [("MusicVolume".to_string(), "999".to_string())].into_iter().collect(), ..DesignSpec::default() };
        save_draft(&s.catalog, &id, "Neon Nights", &bad).unwrap();
        assert!(finish(&s.catalog, &id).is_err(), "bad values can't be finished");
        assert_eq!(read(&s, customizer::SETTINGS), es_before, "designing never touches RetroBat");
    }

    #[test]
    fn applying_a_design_is_previewed_done_together_and_undone_together() {
        let s = setup();
        let es_before = read(&s, customizer::SETTINGS);
        let ini_before = read(&s, customizer::INI);
        let spec = DesignSpec {
            theme: Some(ThemeChoice::Remix { base: "es-theme-test".into(), name: "Neon Nights".into(), colors: [("baseColor".to_string(), "ff00ff".to_string())].into_iter().collect(), images: BTreeMap::new(), fonts: BTreeMap::new() }),
            ini: [("SplashScreen.RandomVideo".to_string(), "0".to_string()), ("EmulationStation.InterfaceMode".to_string(), "1".to_string())].into_iter().collect(),
            own_video: Some(s.mine.join("Intro.mp4").display().to_string()),
            music: [("audio.persystem".to_string(), "true".to_string()), ("MusicVolume".to_string(), "100".to_string())].into_iter().collect(),
            own_music: vec![OwnMusic { path: s.mine.join("Boss.ogg").display().to_string(), system: Some("snes".into()) }],
            systems: [("snes.emulator".to_string(), Some("libretro".to_string())), ("snes.core".to_string(), Some("bsnes".to_string())), ("nes.shaderset".to_string(), None)].into_iter().collect(),
            play_tracks: None,
        };
        let id = create(&s.catalog, "Neon Nights", spec).unwrap();
        finish(&s.catalog, &id).unwrap();

        let p = preview(&s.catalog, &id, &s.install, &connected).unwrap();
        assert_eq!((p.theme_action.as_deref(), p.theme_folder.as_deref()), (Some("create"), Some("Neon Nights")));
        assert_eq!(p.files.len(), 2);
        assert!(p.es_changes.iter().any(|c| c.key == "ThemeSet" && c.after.as_deref() == Some("Neon Nights")));
        assert!(p.es_changes.iter().any(|c| c.system == "nes" && c.key == "shaderset" && c.after.is_none()), "AUTO removes the setting");
        assert!(p.ini_changes.iter().any(|c| c.key == "FileName" && c.after.as_deref() == Some("Intro.mp4")));
        assert_eq!(read(&s, customizer::SETTINGS), es_before, "previewing changes nothing");

        assert!(apply(&s.catalog, &id, &s.install, &p.es_sha, &p.ini_sha, &connected, &open, &roomy).unwrap_err().contains("running"));
        assert!(apply(&s.catalog, &id, &s.install, "stale", &p.ini_sha, &connected, &closed, &roomy).is_err());
        apply(&s.catalog, &id, &s.install, &p.es_sha, &p.ini_sha, &connected, &closed, &roomy).unwrap();
        let es = read(&s, customizer::SETTINGS);
        assert!(es.contains("<string name=\"ThemeSet\" value=\"Neon Nights\" />"));
        assert!(es.contains("<bool name=\"audio.persystem\" value=\"true\" />"), "bool settings are written as <bool>");
        assert!(es.contains("<int name=\"MusicVolume\" value=\"100\" />"));
        assert!(es.contains("snes.core") && !es.contains("nes.shaderset"));
        let ini = read(&s, customizer::INI);
        assert!(ini.contains("FileName=\"Intro.mp4\"") && ini.contains("RandomVideo=0") && ini.contains("InterfaceMode=1"));
        assert!(s.root.join("emulationstation/.emulationstation/themes/Neon Nights/theme.xml").exists());
        assert_eq!(fs::read(s.root.join("emulationstation/.emulationstation/video/Intro.mp4")).unwrap(), b"my intro");
        assert_eq!(fs::read(s.root.join("emulationstation/.emulationstation/music/snes/Boss.ogg")).unwrap(), b"boss");

        // Applying again: the theme and files are recognised; nothing left to change.
        let again = preview(&s.catalog, &id, &s.install, &connected).unwrap();
        assert_eq!(again.theme_action.as_deref(), Some("reuse"));
        assert!(again.files.iter().all(|f| f.action == "already_there"));
        assert!(again.nothing);

        let app = applications(&s.catalog, &s.install).unwrap();
        assert_eq!((app.len(), app[0].state.as_str()), (1, "applied"));
        undo(&s.catalog, app[0].id, &connected, &closed).unwrap();
        assert_eq!(read(&s, customizer::SETTINGS), es_before, "both files back as they were");
        assert_eq!(read(&s, customizer::INI), ini_before);
        assert!(s.root.join("emulationstation/.emulationstation/video/Intro.mp4").exists(), "added files are kept");
        assert_eq!(applications(&s.catalog, &s.install).unwrap()[0].state, "undone");
    }

    #[test]
    fn a_design_that_cant_fully_apply_changes_nothing() {
        let s = setup();
        let ini_before = read(&s, customizer::INI);
        let es_before = read(&s, customizer::SETTINGS);
        // A different file with the same name as the own video is already on the drive.
        fs::write(s.root.join("emulationstation/.emulationstation/video/Intro.mp4"), b"someone else's").unwrap();
        let spec = DesignSpec { own_video: Some(s.mine.join("Intro.mp4").display().to_string()), music: [("audio.bgmusic".to_string(), "false".to_string())].into_iter().collect(), ..DesignSpec::default() };
        let id = create(&s.catalog, "Clash", spec).unwrap();
        finish(&s.catalog, &id).unwrap();
        assert!(preview(&s.catalog, &id, &s.install, &connected).unwrap_err().contains("already"));
        // A value this RetroBat doesn't offer.
        let bad = DesignSpec { systems: [("snes.core".to_string(), Some("nope".to_string()))].into_iter().collect(), ..DesignSpec::default() };
        let id2 = create(&s.catalog, "Bad", bad).unwrap();
        finish(&s.catalog, &id2).unwrap();
        assert!(preview(&s.catalog, &id2, &s.install, &connected).is_err());
        assert_eq!((read(&s, customizer::INI), read(&s, customizer::SETTINGS)), (ini_before, es_before));
    }

    #[test]
    fn a_design_chooses_which_tracks_play() {
        let s = setup();
        let music = s.root.join("emulationstation/.emulationstation/music");
        fs::create_dir_all(music.join("snes")).unwrap();
        fs::write(music.join("Liked.ogg"), b"a").unwrap();
        fs::write(music.join("Disliked.ogg"), b"b").unwrap();
        fs::write(music.join("snes/Snes.ogg"), b"c").unwrap();
        let es_before = read(&s, customizer::SETTINGS);
        let spec = DesignSpec {
            own_music: vec![OwnMusic { path: s.mine.join("Boss.ogg").display().to_string(), system: None }],
            play_tracks: Some(vec!["Liked.ogg".into(), "snes/Snes.ogg".into(), "Boss.ogg".into()]),
            ..DesignSpec::default()
        };
        let id = create(&s.catalog, "Only the good ones", spec).unwrap();
        finish(&s.catalog, &id).unwrap();
        let p = preview(&s.catalog, &id, &s.install, &connected).unwrap();
        assert_eq!((p.tracks_playing, p.tracks_left_out), (Some(3), Some(1)));
        assert!(p.es_changes.iter().any(|c| c.system == "audio" && c.key == "useFavoriteMusic"));
        apply(&s.catalog, &id, &s.install, &p.es_sha, &p.ini_sha, &connected, &closed, &roomy).unwrap();
        let fav = read(&s, customizer::FAVORITES);
        assert!(fav.contains("/music/Liked.ogg;Liked\r\n") && fav.contains("/music/snes/Snes.ogg;Snes\r\n") && fav.contains("/music/Boss.ogg;Boss\r\n"));
        assert!(!fav.contains("Disliked"), "the disliked track won't play");
        assert!(read(&s, customizer::SETTINGS).contains("<bool name=\"audio.useFavoriteMusic\" value=\"true\" />"));
        assert!(music.join("Disliked.ogg").exists(), "nothing is deleted");
        // Undo: favorites.m3u (created) moves aside, settings come back.
        let app = applications(&s.catalog, &s.install).unwrap()[0].id;
        undo(&s.catalog, app, &connected, &closed).unwrap();
        assert!(!s.root.join(customizer::FAVORITES).exists());
        assert_eq!(read(&s, customizer::SETTINGS), es_before);
        // A track that isn't there, or no tracks at all, is refused.
        let bad = create(&s.catalog, "Missing", DesignSpec { play_tracks: Some(vec!["Nope.ogg".into()]), ..DesignSpec::default() }).unwrap();
        finish(&s.catalog, &bad).unwrap();
        assert!(preview(&s.catalog, &bad, &s.install, &connected).unwrap_err().contains("isn't on this RetroBat"));
        let none = create(&s.catalog, "None", DesignSpec { play_tracks: Some(vec![]), ..DesignSpec::default() }).unwrap();
        assert!(finish(&s.catalog, &none).is_err());
    }

    #[test]
    fn a_design_can_start_from_a_retrobats_current_setup() {
        let s = setup();
        let seed = seed_from(&s.catalog, &s.install, &connected).unwrap();
        assert_eq!(seed.theme, Some(ThemeChoice::Existing { folder: "es-theme-test".into() }));
        assert_eq!(seed.systems.get("nes.shaderset"), Some(&Some("vhs".to_string())));
        assert_eq!(seed.ini.get("SplashScreen.RandomVideo").map(String::as_str), Some("1"));
        let id = create(&s.catalog, "As it is", seed).unwrap();
        finish(&s.catalog, &id).unwrap();
        assert!(preview(&s.catalog, &id, &s.install, &connected).unwrap().nothing, "the current setup changes nothing");
        // Even when a current value isn't among the install's choices (e.g. a shader folder removed since).
        fs::remove_dir_all(s.root.join("system/shaders/configs/vhs")).unwrap();
        assert!(preview(&s.catalog, &id, &s.install, &connected).unwrap().nothing, "unchanged values aren't re-checked");
        // A current theme that isn't on the drive isn't copied into the design.
        fs::write(s.root.join(customizer::SETTINGS), ES.replace("es-theme-test", "Gone Theme")).unwrap();
        assert_eq!(seed_from(&s.catalog, &s.install, &connected).unwrap().theme, None);
    }
}
