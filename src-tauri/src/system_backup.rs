//! Backup, step 1 (FR-09, owner 2026-11-09 "Backup and restore"; ADR-090; docs/18 §56):
//! copy a system's saves, settings and game lists into a new folder on this PC. All
//! seven systems since 2026-11-09 (owner "Build all 5 backup and restore for all 5";
//! ADR-092; docs/18 §58).
//!
//! Reads only on the system. On the PC it only creates a new folder and new files in it
//! (`gamelists::copy_image`: temporary file, checked against the original, then named
//! without replacing anything). While copying, the folder's name ends in
//! "(incomplete)"; `manifest.json` (every file's size and SHA-256) is written last and
//! the folder is then renamed. Nothing is ever deleted: a backup that didn't finish
//! keeps its "(incomplete)" name and is listed as such.
//!
//! What each system's backup holds is one list of [`Rule`]s per system ([`rules`]): the
//! backup reads by them and restore only accepts paths they cover ([`covered`]), so the
//! two can't drift apart. Facts (checked in each project's source, 2026-11-09):
//! - RetroBat: `saves\`; `retrobat.ini`; EmulationStation's `es_settings.cfg`,
//!   `es_input.cfg`, `gamecontrollerdb.txt`, `collections\`, `scripts\`,
//!   `themesettings\`; `user\`; `roms\<system>\gamelist.xml`. Optional: music, boot videos.
//! - ES-DE: its data folder's `settings\`, `collections\`, `custom_systems\`,
//!   `controllers\`, `scripts\`, `gamelists\`; RetroArch's `saves\` and `states\` when
//!   RetroArch is in ES-DE's Emulators folder. Optional: `downloaded_media\` (art).
//! - Batocera (userdata): `saves\` (batocera-launch-libretro: `/userdata/saves/`);
//!   `system\batocera.conf`; `system\configs\emulationstation\` es_settings.cfg,
//!   es_input.cfg, `collections\`; `roms\<system>\gamelist.xml`.
//! - Recalbox (share): `saves\` (configgen `recalboxFiles.py`); `system\recalbox.conf`;
//!   `system\.emulationstation\` es_settings.cfg, es_input.cfg; per-game
//!   `roms\<system>\<rom>.recalbox.conf`; `roms\<system>\gamelist.xml`.
//! - RetroPie: saves sit next to the games (RetroPie sets no `savefile_directory`, so
//!   RetroArch uses the game's folder), picked out by file type ([`is_save_file`]);
//!   EmulationStation's folder is moved onto the `configs` share
//!   (`configs\all\emulationstation`: es_settings.cfg, es_input.cfg, `collections\`,
//!   `gamelists\`), with `all\retroarch.cfg`, `all\retroarch-core-options.cfg`,
//!   `all\retroarch-joypads\` and each system's `emulators.cfg`/`retroarch.cfg`. The
//!   configs share is reached over the network only.
//! - ROCKNIX: save files next to the games (`savefiles_in_content_dir = "true"`), save
//!   states in `savestates\<system>` (`setsettings.sh`); game lists in the system
//!   folders; settings on the `config` share (`system\configs\system.cfg`,
//!   `emulationstation\` es_settings.cfg, es_input.cfg, `collections\`), network only.
//! - Lakka: the `Savefiles` and `Savestates` shares; RetroArch's settings on
//!   `Configfiles` (`retroarch\retroarch.cfg`, `retroarch-core-options.cfg`,
//!   `retroarch\config\`) and `Remappings`; its game lists are the `Playlists` share.

use crate::customizer::RunningFn;
use crate::installations::{self, Located};
use crate::volumes::Presence;
use crate::{adapters, bios_step, copier, gamelists};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "manifest.json";
pub(crate) const INCOMPLETE: &str = " (incomplete)";
/// Manifest format; restore refuses formats it doesn't know.
pub(crate) const FORMAT: u32 = 1;

/// Where a backed-up file came from, so restore can find the place again even if the
/// drive letter changed. Each area is one folder (or network share) of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    /// The installation folder (RetroBat, Batocera's userdata, Recalbox's share).
    Install,
    EsdeData,
    Retroarch,
    /// The folder that holds the system folders (RetroPie, ROCKNIX).
    Games,
    /// RetroPie's `configs` share.
    Configs,
    /// ROCKNIX's `config` share.
    Config,
    /// Lakka's shares.
    Savefiles,
    Savestates,
    Configfiles,
    Playlists,
    Remappings,
}

impl Area {
    pub(crate) fn folder(self) -> &'static str {
        match self {
            Area::Install => "install",
            Area::EsdeData => "esde_data",
            Area::Retroarch => "retroarch",
            Area::Games => "games",
            Area::Configs => "configs",
            Area::Config => "config",
            Area::Savefiles => "savefiles",
            Area::Savestates => "savestates",
            Area::Configfiles => "configfiles",
            Area::Playlists => "playlists",
            Area::Remappings => "remappings",
        }
    }
}

pub(crate) struct GroupDef {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) optional: bool,
}

pub(crate) const GROUPS: &[GroupDef] = &[
    GroupDef { id: "saves", label: "Saves", optional: false },
    GroupDef { id: "settings", label: "Settings", optional: false },
    GroupDef { id: "gamelists", label: "Game lists", optional: false },
    GroupDef { id: "media", label: "Music and boot videos", optional: true },
    GroupDef { id: "art", label: "Downloaded art", optional: true },
];

/// What a rule covers, below its area's folder (paths with `/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One file.
    File(&'static str),
    /// Every file below a folder (`""`: the whole area).
    Tree(&'static str),
    /// `<dir>/<system>/<file>` (`dir` may be `""`).
    PerSystem(&'static str, &'static str),
    /// Save files directly in `<dir>/<system>/` ([`is_save_file`]).
    PerSystemSaves(&'static str),
    /// Files directly in `<dir>/<system>/` ending in a suffix.
    PerSystemSuffix(&'static str, &'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rule {
    pub(crate) area: Area,
    pub(crate) group: &'static str,
    pub(crate) kind: Kind,
}

const fn r(area: Area, group: &'static str, kind: Kind) -> Rule {
    Rule { area, group, kind }
}

/// What a backup of each kind of system holds.
pub(crate) fn rules(adapter: &str) -> Vec<Rule> {
    use Area::*;
    use Kind::*;
    match adapter {
        "retrobat" => vec![
            r(Install, "saves", Tree("saves")),
            r(Install, "settings", File("retrobat.ini")),
            r(Install, "settings", File("emulationstation/.emulationstation/es_settings.cfg")),
            r(Install, "settings", File("emulationstation/.emulationstation/es_input.cfg")),
            r(Install, "settings", File("emulationstation/.emulationstation/gamecontrollerdb.txt")),
            r(Install, "settings", Tree("emulationstation/.emulationstation/collections")),
            r(Install, "settings", Tree("emulationstation/.emulationstation/scripts")),
            r(Install, "settings", Tree("emulationstation/.emulationstation/themesettings")),
            r(Install, "settings", Tree("user")),
            r(Install, "gamelists", PerSystem("roms", "gamelist.xml")),
            r(Install, "media", Tree("emulationstation/.emulationstation/music")),
            r(Install, "media", Tree("emulationstation/.emulationstation/video")),
        ],
        "esde" => vec![
            r(Retroarch, "saves", Tree("saves")),
            r(Retroarch, "saves", Tree("states")),
            r(EsdeData, "settings", Tree("settings")),
            r(EsdeData, "settings", Tree("collections")),
            r(EsdeData, "settings", Tree("custom_systems")),
            r(EsdeData, "settings", Tree("controllers")),
            r(EsdeData, "settings", Tree("scripts")),
            r(EsdeData, "gamelists", Tree("gamelists")),
            r(EsdeData, "art", Tree("downloaded_media")),
        ],
        "batocera" => vec![
            r(Install, "saves", Tree("saves")),
            r(Install, "settings", File("system/batocera.conf")),
            r(Install, "settings", File("system/configs/emulationstation/es_settings.cfg")),
            r(Install, "settings", File("system/configs/emulationstation/es_input.cfg")),
            r(Install, "settings", Tree("system/configs/emulationstation/collections")),
            r(Install, "gamelists", PerSystem("roms", "gamelist.xml")),
        ],
        "recalbox" => vec![
            r(Install, "saves", Tree("saves")),
            r(Install, "settings", File("system/recalbox.conf")),
            r(Install, "settings", File("system/.emulationstation/es_settings.cfg")),
            r(Install, "settings", File("system/.emulationstation/es_input.cfg")),
            r(Install, "settings", PerSystemSuffix("roms", ".recalbox.conf")),
            r(Install, "gamelists", PerSystem("roms", "gamelist.xml")),
        ],
        "retropie" => vec![
            r(Games, "saves", PerSystemSaves("")),
            r(Configs, "settings", File("all/emulationstation/es_settings.cfg")),
            r(Configs, "settings", File("all/emulationstation/es_input.cfg")),
            r(Configs, "settings", Tree("all/emulationstation/collections")),
            r(Configs, "settings", File("all/retroarch-core-options.cfg")),
            r(Configs, "settings", Tree("all/retroarch-joypads")),
            r(Configs, "settings", PerSystem("", "emulators.cfg")),
            r(Configs, "settings", PerSystem("", "retroarch.cfg")),
            r(Games, "gamelists", PerSystem("", "gamelist.xml")),
            r(Configs, "gamelists", Tree("all/emulationstation/gamelists")),
        ],
        "rocknix" => vec![
            r(Games, "saves", PerSystemSaves("")),
            r(Games, "saves", Tree("savestates")),
            r(Config, "settings", File("system/configs/system.cfg")),
            r(Config, "settings", File("emulationstation/es_settings.cfg")),
            r(Config, "settings", File("emulationstation/es_input.cfg")),
            r(Config, "settings", Tree("emulationstation/collections")),
            r(Games, "gamelists", PerSystem("", "gamelist.xml")),
        ],
        "lakka" => vec![
            r(Savefiles, "saves", Tree("")),
            r(Savestates, "saves", Tree("")),
            r(Configfiles, "settings", File("retroarch/retroarch.cfg")),
            r(Configfiles, "settings", File("retroarch/retroarch-core-options.cfg")),
            r(Configfiles, "settings", Tree("retroarch/config")),
            r(Remappings, "settings", Tree("")),
            r(Playlists, "gamelists", Tree("")),
        ],
        _ => Vec::new(),
    }
}

/// A save file RetroArch (or an emulator) writes next to a game: battery saves, memory
/// cards and save states (with their thumbnails), never a game's own file type.
pub(crate) fn is_save_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if [".srm", ".sav", ".rtc", ".eep", ".fla", ".mpk", ".sra", ".mcr"].iter().any(|e| n.ends_with(e) && n.len() > e.len()) {
        return true;
    }
    match n.rfind(".state") {
        Some(i) if i > 0 => {
            let rest = n[i + 6..].trim_end_matches(".png");
            let rest = rest.strip_prefix('.').unwrap_or(rest);
            rest.is_empty() || rest == "auto" || rest.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// Whether a rule covers a path (already checked to be plain).
fn rule_covers(rule: &Rule, path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let per_system = |dir: &str| -> Option<&str> {
        match (dir.is_empty(), parts.len()) {
            (true, 2) if !parts[0].starts_with('.') => Some(parts[1]),
            (false, 3) if parts[0] == dir && !parts[1].starts_with('.') => Some(parts[2]),
            _ => None,
        }
    };
    match rule.kind {
        Kind::File(f) => path == f,
        Kind::Tree("") => true,
        Kind::Tree(prefix) => path.len() > prefix.len() + 1 && path.starts_with(prefix) && path.as_bytes()[prefix.len()] == b'/',
        Kind::PerSystem(dir, file) => per_system(dir) == Some(file),
        Kind::PerSystemSaves(dir) => per_system(dir).is_some_and(is_save_file),
        Kind::PerSystemSuffix(dir, suffix) => per_system(dir).is_some_and(|n| n.len() > suffix.len() && n.to_ascii_lowercase().ends_with(suffix)),
    }
}

/// A relative path with nothing unusual in it: no `..`, drive, share, control
/// characters, RetroDrive's temporary marker or `.retrodrive` folder.
pub(crate) fn plain(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    !path.is_empty()
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains([':', '\\']) && !p.chars().any(char::is_control) && !p.contains(copier::TEMP_MARKER))
        && !parts.iter().any(|p| p.eq_ignore_ascii_case(".retrodrive"))
}

/// Whether a backup of this kind of system covers this place (restore only writes there).
pub(crate) fn covered(adapter: &str, area: Area, path: &str) -> bool {
    plain(path) && rules(adapter).iter().any(|rule| rule.area == area && rule_covers(rule, path))
}

/// One file to back up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) group: &'static str,
    pub(crate) area: Area,
    /// Below the area's folder, with `/`.
    pub(crate) rel: String,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
}

/// The folders (or shares) an installation's files are found in.
pub(crate) struct Places {
    /// The installation folder; RetroDrive's restore records go here.
    pub(crate) install: PathBuf,
    pub(crate) bases: Vec<(Area, PathBuf)>,
    /// Areas that can't be reached, with why.
    pub(crate) missing: Vec<(Area, String)>,
}

impl Places {
    pub(crate) fn base(&self, area: Area) -> Option<&Path> {
        if area == Area::Install {
            return Some(&self.install);
        }
        self.bases.iter().find(|(a, _)| *a == area).map(|(_, p)| p.as_path())
    }
    pub(crate) fn missing_note(&self, area: Area) -> Option<&str> {
        self.missing.iter().find(|(a, _)| *a == area).map(|(_, n)| n.as_str())
    }
}

/// What a group would back up.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GroupPreview {
    pub id: &'static str,
    pub label: &'static str,
    pub optional: bool,
    pub files: usize,
    pub bytes: u64,
    /// e.g. why there is nothing in it.
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BackupPreview {
    pub system: String,
    pub location: String,
    pub groups: Vec<GroupPreview>,
    /// The folder backups go into.
    pub destination: String,
    pub free_bytes: Option<u64>,
    /// Why a backup can't start right now (e.g. RetroBat is running).
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    pub group: String,
    pub area: Area,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    pub created_at: String,
    pub adapter_id: String,
    pub adapter_name: String,
    pub version: Option<String>,
    pub installation_id: String,
    /// Where the system was when backed up (shown; restore finds it by installation).
    pub location: String,
    pub groups: Vec<String>,
    pub complete: bool,
    /// Files that couldn't be copied (kept out of `files`).
    pub problems: Vec<String>,
    pub files: Vec<ManifestFile>,
}

/// A backup folder, as listed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackupEntry {
    pub folder: String,
    pub name: String,
    pub created_at: Option<String>,
    pub installation_id: Option<String>,
    pub system: Option<String>,
    pub location: Option<String>,
    pub files: usize,
    pub bytes: u64,
    pub groups: Vec<String>,
    pub complete: bool,
    pub problems: usize,
}

/// The result of checking a backup against its manifest (reads only).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CheckResult {
    pub files: usize,
    pub ok: usize,
    pub missing: Vec<String>,
    pub changed: Vec<String>,
}

pub(crate) fn located(connection: &Connection, installation_id: &str, presence: crate::customizer::PresenceFn) -> Result<Located, String> {
    let l = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if rules(&l.adapter_id).is_empty() {
        return Err("Backups aren't available for this system yet.".into());
    }
    if l.presence != Presence::Connected {
        return Err("The drive isn't connected.".into());
    }
    Ok(l)
}

pub(crate) fn places(l: &Located) -> Result<Places, String> {
    let mut p = Places { install: l.root.clone(), bases: Vec::new(), missing: Vec::new() };
    let mut add = |area: Area, dir: Option<PathBuf>, why: String| match dir.filter(|d| d.is_dir()) {
        Some(d) => p.bases.push((area, d)),
        None => p.missing.push((area, why)),
    };
    match l.adapter_id.as_str() {
        "esde" => {
            let data = gamelists::esde_data_dir(&l.root).ok_or("ES-DE's data folder wasn't found next to this ROMs folder. Start ES-DE once, then try again.")?;
            add(Area::EsdeData, Some(data), String::new());
            add(Area::Retroarch, bios_step::esde_retroarch(&l.root), "RetroArch isn't in ES-DE's Emulators folder, so there are no RetroArch saves here.".into());
        }
        "retropie" => {
            add(Area::Games, Some(gamelists::games_dir(&l.root).0), String::new());
            let configs = crate::retropie_settings::configs_root(&l.root.display().to_string()).map(PathBuf::from);
            add(Area::Configs, configs, "RetroPie keeps its settings and most game lists on its configs network share. Add RetroPie by its network share (\\\\RETROPIE\\roms) to back them up too.".into());
        }
        "rocknix" => {
            add(Area::Games, Some(adapters::rocknix::games_folder(&l.root)), String::new());
            add(Area::Config, crate::rocknix_settings::network_config(&l.root).ok(), "ROCKNIX keeps its settings in a part of the card Windows can't read. Add the ROCKNIX device by its network share (\\\\ROCKNIX\\games-roms) to back them up too.".into());
        }
        "lakka" => {
            for (area, name) in [(Area::Savefiles, "Savefiles"), (Area::Savestates, "Savestates"), (Area::Configfiles, "Configfiles"), (Area::Playlists, "Playlists"), (Area::Remappings, "Remappings")] {
                add(area, adapters::lakka::share(&l.root, name), format!("Lakka's {name} share can't be reached."));
            }
        }
        _ => {}
    }
    Ok(p)
}

/// Files below `dir` (recursively), skipping links, RetroDrive's own folder and its
/// temporary files.
fn tree(out: &mut Vec<Source>, group: &'static str, area: Area, base: &Path, rel_dir: &str) {
    let dir = if rel_dir.is_empty() { base.to_path_buf() } else { crate::paths::join(&base.display().to_string(), rel_dir) };
    if !dir.is_dir() {
        return;
    }
    for entry in walkdir::WalkDir::new(&dir).follow_links(false).into_iter().filter_entry(|e| e.file_name() != ".retrodrive").filter_map(Result::ok) {
        if !entry.file_type().is_file() || entry.file_name().to_string_lossy().contains(copier::TEMP_MARKER) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(base) else { continue };
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(Source { group, area, rel, path: entry.into_path(), size });
    }
}

fn file(out: &mut Vec<Source>, group: &'static str, area: Area, base: &Path, rel: &str) {
    let path = crate::paths::join(&base.display().to_string(), rel);
    if let Ok(m) = fs::symlink_metadata(&path) {
        if m.is_file() {
            out.push(Source { group, area, rel: rel.to_string(), path, size: m.len() });
        }
    }
}

/// The system folders in `<base>/<dir>` (not hidden ones), sorted.
fn system_folders(base: &Path, dir: &str) -> Vec<String> {
    let folder = if dir.is_empty() { base.to_path_buf() } else { base.join(dir) };
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> = entries.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect();
    names.sort();
    names
}

fn join_rel(dir: &str, system: &str, name: &str) -> String {
    if dir.is_empty() { format!("{system}/{name}") } else { format!("{dir}/{system}/{name}") }
}

/// Every file the system's rules cover, once each.
pub(crate) fn sources(adapter: &str, p: &Places) -> Vec<Source> {
    let mut out = Vec::new();
    for rule in rules(adapter) {
        let Some(base) = p.base(rule.area) else { continue };
        match rule.kind {
            Kind::File(f) => file(&mut out, rule.group, rule.area, base, f),
            Kind::Tree(d) => tree(&mut out, rule.group, rule.area, base, d),
            Kind::PerSystem(dir, name) => {
                for system in system_folders(base, dir) {
                    file(&mut out, rule.group, rule.area, base, &join_rel(dir, &system, name));
                }
            }
            Kind::PerSystemSaves(dir) | Kind::PerSystemSuffix(dir, _) => {
                for system in system_folders(base, dir) {
                    let folder = crate::paths::join(&base.display().to_string(), &join_rel(dir, &system, ""));
                    let Ok(entries) = fs::read_dir(&folder) else { continue };
                    let mut names: Vec<String> = entries.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_file())).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                    names.sort();
                    for name in names {
                        let rel = join_rel(dir, &system, &name);
                        if plain(&rel) && rule_covers(&rule, &rel) {
                            file(&mut out, rule.group, rule.area, base, &rel);
                        }
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|s| seen.insert((s.area, s.rel.to_ascii_lowercase())));
    out
}

/// The groups this system has, in order.
fn adapter_groups(adapter: &str) -> Vec<&'static GroupDef> {
    let rules = rules(adapter);
    GROUPS.iter().filter(|g| rules.iter().any(|r| r.group == g.id)).collect()
}

/// The groups this adapter has, with what each would back up.
fn group_previews(adapter: &str, p: &Places, all: &[Source]) -> Vec<GroupPreview> {
    let rules = rules(adapter);
    adapter_groups(adapter)
        .into_iter()
        .map(|g| {
            let files: Vec<&Source> = all.iter().filter(|s| s.group == g.id).collect();
            let unreachable: Vec<&str> = rules.iter().filter(|r| r.group == g.id).filter_map(|r| p.missing_note(r.area)).collect();
            let note = match (files.is_empty(), unreachable.first()) {
                (true, Some(n)) => Some(n.to_string()),
                (true, None) => Some("Nothing to back up.".to_string()),
                (false, Some(n)) => Some(format!("Some are left out: {n}")),
                (false, None) => None,
            };
            GroupPreview { id: g.id, label: g.label, optional: g.optional, files: files.len(), bytes: files.iter().map(|s| s.size).sum(), note }
        })
        .collect()
}

fn chosen(adapter: &str, optional: &[String]) -> Vec<&'static str> {
    adapter_groups(adapter).into_iter().filter(|g| !g.optional || optional.iter().any(|o| o == g.id)).map(|g| g.id).collect()
}

pub(crate) fn adapter_name(id: &str) -> String {
    adapters::find(id).map(|a| a.display_name().to_string()).unwrap_or_else(|| id.to_string())
}

fn blocked(adapter: &str, running: RunningFn) -> Option<String> {
    running().then(|| format!("{} is running. Close it first, so no save is half-written while it's copied.", adapter_name(adapter)))
}

/// What a backup would copy (reads only).
pub fn preview(connection: &Connection, installation_id: &str, destination: &Path, presence: crate::customizer::PresenceFn, running: RunningFn) -> Result<BackupPreview, String> {
    let l = located(connection, installation_id, presence)?;
    let p = places(&l)?;
    let all = sources(&l.adapter_id, &p);
    Ok(BackupPreview {
        system: adapter_name(&l.adapter_id),
        location: l.root.display().to_string(),
        groups: group_previews(&l.adapter_id, &p, &all),
        destination: destination.display().to_string(),
        free_bytes: existing_ancestor(destination).and_then(|d| crate::volumes::space(&d)).map(|s| s.free_bytes),
        blocked: blocked(&l.adapter_id, running),
    })
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|a| a.is_dir()).map(Path::to_path_buf)
}

/// A folder name Windows accepts.
pub(crate) fn safe_name(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// e.g. `RetroBat (G) 2026-11-09 14.32`.
fn folder_name(system: &str, location: &Path, stamp: &str) -> String {
    let text = location.display().to_string();
    let place = if let Some(share) = text.strip_prefix(r"\\") {
        share.split('\\').take(2).collect::<Vec<_>>().join(" ")
    } else {
        text.chars().next().filter(|c| c.is_ascii_alphabetic() && text.get(1..2) == Some(":")).map(|c| c.to_ascii_uppercase().to_string()).unwrap_or_default()
    };
    safe_name(&if place.is_empty() { format!("{system} {stamp}") } else { format!("{system} ({place}) {stamp}") })
}

/// A name not taken yet, as a finished or an incomplete backup.
pub(crate) fn free_folder(destination: &Path, name: &str) -> Result<String, String> {
    for n in 1..1000 {
        let candidate = if n == 1 { name.to_string() } else { format!("{name} ({n})") };
        let taken = |s: &str| fs::symlink_metadata(destination.join(s)).is_ok();
        if !taken(&candidate) && !taken(&format!("{candidate}{INCOMPLETE}")) {
            return Ok(candidate);
        }
    }
    Err("Couldn't find a free folder name for the backup.".into())
}

pub(crate) fn now(connection: &Connection, format: &str) -> String {
    connection.query_row("SELECT strftime(?1, 'now', 'localtime')", [format], |r| r.get(0)).unwrap_or_default()
}

fn write_manifest(folder: &Path, manifest: &Manifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    write_manifest_bytes(folder, text.as_bytes())
}

/// Write a backup's list of files (temporary name, then named without replacing).
pub(crate) fn write_manifest_bytes(folder: &Path, text: &[u8]) -> Result<(), String> {
    let temp = folder.join(format!("{MANIFEST}{}manifest", copier::TEMP_MARKER));
    fs::write(&temp, text).map_err(|e| format!("Couldn't write the backup's list of files: {e}"))?;
    copier::rename_no_replace(&temp, &folder.join(MANIFEST)).map_err(|e| format!("Couldn't name the backup's list of files: {e}"))
}

/// Back the system up into a new folder in `destination`. Returns the finished backup.
pub fn run(connection: &Connection, installation_id: &str, optional: &[String], destination: &Path, presence: crate::customizer::PresenceFn, running: RunningFn, progress: gamelists::ProgressFn) -> Result<BackupEntry, String> {
    let l = located(connection, installation_id, presence)?;
    if let Some(reason) = blocked(&l.adapter_id, running) {
        return Err(format!("{reason} Nothing was copied."));
    }
    let p = places(&l)?;
    let groups = chosen(&l.adapter_id, optional);
    let files: Vec<Source> = sources(&l.adapter_id, &p).into_iter().filter(|s| groups.contains(&s.group)).collect();
    if files.is_empty() {
        return Err("There's nothing to back up on this system yet.".into());
    }
    let bytes: u64 = files.iter().map(|s| s.size).sum();
    fs::create_dir_all(destination).map_err(|e| format!("Couldn't create {}: {e}", destination.display()))?;
    if let Some(space) = crate::volumes::space(destination) {
        if space.free_bytes < bytes + bytes / 20 + 16 * 1024 * 1024 {
            return Err(format!("Not enough space in {}: the backup needs about {} MB.", destination.display(), bytes / (1024 * 1024) + 1));
        }
    }
    let system = adapter_name(&l.adapter_id);
    let name = free_folder(destination, &folder_name(&system, &l.root, &now(connection, "%Y-%m-%d %H.%M")))?;
    let working = destination.join(format!("{name}{INCOMPLETE}"));
    fs::create_dir(&working).map_err(|e| format!("Couldn't create the backup folder: {e}"))?;

    let results = gamelists::parallel_map(
        &files,
        "backing up",
        |s| {
            // The file may have been removed since the list was made.
            if !s.path.is_file() {
                return Err(format!("{} (gone)", s.rel));
            }
            let target = crate::paths::join(&working.join(s.area.folder()).display().to_string(), &s.rel);
            gamelists::copy_image(&s.path, &target).map_err(|e| format!("{} ({e})", s.rel))?;
            let size = fs::metadata(&target).map(|m| m.len()).map_err(|e| format!("{} ({e})", s.rel))?;
            let sha256 = copier::sha256_of(&target).map_err(|e| format!("{} ({e})", s.rel))?;
            Ok(ManifestFile { group: s.group.to_string(), area: s.area, path: s.rel.clone(), size, sha256 })
        },
        progress,
    );
    let mut manifest = Manifest {
        format: FORMAT,
        app: format!("RetroDrive {}", env!("CARGO_PKG_VERSION")),
        created_at: now(connection, "%Y-%m-%dT%H:%M:%S"),
        adapter_id: l.adapter_id.clone(),
        adapter_name: system,
        version: l.version.clone(),
        installation_id: l.id.clone(),
        location: l.root.display().to_string(),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        complete: false,
        problems: Vec::new(),
        files: Vec::new(),
    };
    for r in results {
        match r {
            Ok(f) => manifest.files.push(f),
            Err(e) => manifest.problems.push(e),
        }
    }
    // A save or setting that couldn't be copied makes the backup incomplete.
    manifest.complete = manifest.problems.is_empty();
    write_manifest(&working, &manifest)?;
    let finished = if manifest.complete {
        let done = destination.join(&name);
        copier::rename_no_replace(&working, &done).map_err(|e| format!("The backup is complete but couldn't be renamed: {e}. It's in {}.", working.display()))?;
        done
    } else {
        working
    };
    Ok(entry(&finished).unwrap_or_else(|| BackupEntry { folder: finished.display().to_string(), name, created_at: None, installation_id: None, system: None, location: None, files: 0, bytes: 0, groups: Vec::new(), complete: false, problems: 0 }))
}

pub(crate) fn read_manifest(folder: &Path) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(folder.join(MANIFEST)).ok()?).ok()
}

pub(crate) fn entry(folder: &Path) -> Option<BackupEntry> {
    let name = folder.file_name()?.to_string_lossy().into_owned();
    let m = read_manifest(folder);
    if m.is_none() && !name.ends_with(INCOMPLETE) {
        return None;
    }
    Some(match m {
        Some(m) => BackupEntry {
            folder: folder.display().to_string(),
            name,
            created_at: Some(m.created_at),
            installation_id: Some(m.installation_id),
            system: Some(m.adapter_name),
            location: Some(m.location),
            files: m.files.len(),
            bytes: m.files.iter().map(|f| f.size).sum(),
            groups: m.groups,
            complete: m.complete && m.format == FORMAT,
            problems: m.problems.len(),
        },
        None => BackupEntry { folder: folder.display().to_string(), name, created_at: None, installation_id: None, system: None, location: None, files: 0, bytes: 0, groups: Vec::new(), complete: false, problems: 0 },
    })
}

/// The backups in `destination`, newest first (reads only).
pub fn list(destination: &Path) -> Vec<BackupEntry> {
    let Ok(dirs) = fs::read_dir(destination) else { return Vec::new() };
    let mut out: Vec<BackupEntry> = dirs.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).filter_map(|e| entry(&e.path())).collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.name.cmp(&a.name)));
    out
}

/// Check every file in a backup against its manifest (reads only).
pub fn check(folder: &Path) -> Result<CheckResult, String> {
    let m = read_manifest(folder).ok_or("This folder has no readable backup list (manifest.json).")?;
    let mut result = CheckResult { files: m.files.len(), ok: 0, missing: Vec::new(), changed: Vec::new() };
    let checks = gamelists::parallel_map(
        &m.files,
        "checking",
        |f| {
            let path = crate::paths::join(&folder.join(f.area.folder()).display().to_string(), &f.path);
            match fs::metadata(&path) {
                Err(_) => Err(true),
                Ok(meta) if meta.len() != f.size => Err(false),
                Ok(_) if copier::sha256_of(&path).ok().as_deref() != Some(f.sha256.as_str()) => Err(false),
                Ok(_) => Ok(()),
            }
        },
        &|_, _, _| {},
    );
    for (f, c) in m.files.iter().zip(checks) {
        match c {
            Ok(()) => result.ok += 1,
            Err(true) => result.missing.push(f.path.clone()),
            Err(false) => result.changed.push(f.path.clone()),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn closed() -> bool {
        false
    }
    fn open() -> bool {
        true
    }
    fn listing(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| (e.path().display().to_string(), fs::read(e.path()).unwrap())).collect();
        out.sort();
        out
    }

    /// A RetroBat with saves, settings, game lists, music, plus things left out.
    fn retrobat(root: &Path) {
        for f in ["RetroBat.exe", "retrobat.ini"] {
            put(root, f, "x");
        }
        put(root, "emulationstation/.emulationstation/es_settings.cfg", "<config />");
        put(root, "emulationstation/.emulationstation/es_input.cfg", "<inputList />");
        put(root, "emulationstation/.emulationstation/es_systems.cfg", "<systemList />");
        put(root, "emulationstation/.emulationstation/themes/carbon/theme.xml", "theme");
        put(root, "emulationstation/.emulationstation/collections/custom-Fav.cfg", "a");
        put(root, "emulationstation/.emulationstation/music/song.ogg", "music");
        put(root, "emulationstation/emulationstation.exe", "x");
        put(root, "saves/snes/F-Zero (USA).srm", "save");
        put(root, "saves/mame/nvram/pacman/eeprom", "nv");
        put(root, "saves/snes/half.srm.rdpart-1234", "temp");
        put(root, "user/inputmapping/a.xml", "map");
        put(root, "roms/snes/gamelist.xml", "<gameList />");
        put(root, "roms/snes/F-Zero (USA).zip", "game");
        put(root, "roms/nes/Zelda.nes", "game");
        put(root, "bios/scph5501.bin", "bios");
        put(root, "emulators/retroarch/retroarch.cfg", "cfg");
        put(root, ".retrodrive/backup/x/es_settings.cfg", "old");
    }

    fn setup() -> (tempfile::TempDir, Connection, String, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        retrobat(&root);
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        (dir, c, id, root)
    }

    #[test]
    fn the_preview_lists_saves_settings_and_game_lists_only() {
        let (dir, c, id, _root) = setup();
        let p = preview(&c, &id, &dir.path().join("Backups"), &connected, &closed).unwrap();
        let g = |id: &str| p.groups.iter().find(|g| g.id == id).unwrap().clone();
        assert_eq!(g("saves").files, 2, "temporary files aren't saves");
        assert_eq!(g("settings").files, 5, "retrobat.ini, es_settings.cfg, es_input.cfg, a collection, user");
        assert_eq!(g("gamelists").files, 1);
        assert!(g("media").optional && g("media").files == 1);
        assert!(p.groups.iter().all(|g| g.id != "art"), "ES-DE only");
        assert!(p.blocked.is_none());
        assert!(preview(&c, &id, dir.path(), &connected, &open).unwrap().blocked.unwrap().contains("RetroBat is running"));
    }

    #[test]
    fn a_backup_copies_verified_files_into_a_new_folder_and_never_touches_the_system() {
        let (dir, c, id, root) = setup();
        let before = listing(&root);
        let dest = dir.path().join("Backups");
        assert!(run(&c, &id, &[], &dest, &connected, &open, &|_, _, _| {}).unwrap_err().contains("Nothing was copied"));
        assert!(!dest.exists(), "nothing created while RetroBat runs");

        let b = run(&c, &id, &[], &dest, &connected, &closed, &|_, _, _| {}).unwrap();
        assert!(b.complete && b.files == 8 && b.problems == 0, "{b:?}");
        assert!(b.name.starts_with("RetroBat (") && !b.name.contains("incomplete"), "{}", b.name);
        let folder = PathBuf::from(&b.folder);
        assert_eq!(fs::read_to_string(folder.join("install/saves/snes/F-Zero (USA).srm")).unwrap(), "save");
        assert_eq!(fs::read_to_string(folder.join("install/roms/snes/gamelist.xml")).unwrap(), "<gameList />");
        for left_out in ["install/roms/snes/F-Zero (USA).zip", "install/bios", "install/emulators", "install/emulationstation/.emulationstation/themes", "install/emulationstation/.emulationstation/es_systems.cfg", "install/emulationstation/.emulationstation/music", "install/.retrodrive", "install/saves/snes/half.srm.rdpart-1234"] {
            assert!(!folder.join(left_out).exists(), "{left_out}");
        }
        let m = read_manifest(&folder).unwrap();
        assert_eq!((m.format, m.adapter_id.as_str(), m.complete), (1, "retrobat", true));
        assert_eq!(m.files.iter().find(|f| f.path == "saves/snes/F-Zero (USA).srm").unwrap().sha256, copier::sha256_of(&root.join("saves/snes/F-Zero (USA).srm")).unwrap());
        assert_eq!(listing(&root), before, "the system is unchanged");
        assert!(walkdir::WalkDir::new(&dest).into_iter().filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().contains(copier::TEMP_MARKER)));

        // A second backup goes into its own folder; the first is untouched.
        let first = listing(&folder);
        let b2 = run(&c, &id, &["media".to_string()], &dest, &connected, &closed, &|_, _, _| {}).unwrap();
        assert_ne!(b2.folder, b.folder);
        assert_eq!(b2.files, 9, "music added");
        assert_eq!(listing(&folder), first);
        assert_eq!(list(&dest).len(), 2);
    }

    #[test]
    fn checking_finds_missing_and_changed_files() {
        let (dir, c, id, _root) = setup();
        let dest = dir.path().join("Backups");
        let b = run(&c, &id, &[], &dest, &connected, &closed, &|_, _, _| {}).unwrap();
        let folder = PathBuf::from(&b.folder);
        assert_eq!(check(&folder).unwrap(), CheckResult { files: 8, ok: 8, missing: vec![], changed: vec![] });
        fs::write(folder.join("install/saves/snes/F-Zero (USA).srm"), "edited").unwrap();
        fs::remove_file(folder.join("install/retrobat.ini")).unwrap();
        let r = check(&folder).unwrap();
        assert_eq!((r.ok, r.missing, r.changed), (6, vec!["retrobat.ini".to_string()], vec!["saves/snes/F-Zero (USA).srm".to_string()]));
    }

    #[test]
    fn incomplete_backups_are_listed_and_names_are_never_reused() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("RetroBat (G) 2026-11-09 14.32 (incomplete)")).unwrap();
        fs::create_dir_all(dir.path().join("Holiday photos")).unwrap();
        let l = list(dir.path());
        assert_eq!(l.len(), 1, "other folders aren't backups");
        assert!(!l[0].complete);
        assert_eq!(free_folder(dir.path(), "RetroBat (G) 2026-11-09 14.32").unwrap(), "RetroBat (G) 2026-11-09 14.32 (2)");
        assert_eq!(folder_name("RetroBat", Path::new(r"G:\RetroBat"), "2026-11-09 14.32"), "RetroBat (G) 2026-11-09 14.32");
        assert_eq!(folder_name("Batocera", Path::new(r"\\BATOCERA\share"), "2026-11-09 14.32"), "Batocera (BATOCERA share) 2026-11-09 14.32");
        assert_eq!(safe_name("a:b*c?"), "a b c");
    }

    #[test]
    fn es_de_backs_up_its_data_folder_and_retroarch_saves() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("ES-DE");
        put(&p, "ES-DE.exe", "x");
        put(&p, "portable.txt", "");
        put(&p, "ROMs/atari2600/Pitfall.a26", "game");
        put(&p, "ES-DE/settings/es_settings.xml", "<string name=\"ROMDirectory\" value=\"\" />");
        put(&p, "ES-DE/gamelists/atari2600/gamelist.xml", "<gameList />");
        put(&p, "ES-DE/downloaded_media/atari2600/covers/Pitfall.png", "png");
        put(&p, "ES-DE/themes/x/theme.xml", "t");
        put(&p, "Emulators/RetroArch/retroarch.exe", "x");
        put(&p, "Emulators/RetroArch/saves/Pitfall.srm", "save");
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &p.join("ROMs"), "esde", &|_: &str| VolumeProbe::unknown()).unwrap();
        let pv = preview(&c, &id, dir.path(), &connected, &closed).unwrap();
        assert_eq!(pv.groups.iter().map(|g| (g.id, g.files)).collect::<Vec<_>>(), [("saves", 1), ("settings", 1), ("gamelists", 1), ("art", 1)]);
        let b = run(&c, &id, &[], &dir.path().join("Backups"), &connected, &closed, &|_, _, _| {}).unwrap();
        let folder = PathBuf::from(&b.folder);
        assert!(b.complete && b.files == 3);
        assert!(folder.join("esde_data/settings/es_settings.xml").is_file() && folder.join("retroarch/saves/Pitfall.srm").is_file());
        assert!(!folder.join("esde_data/downloaded_media").exists() && !folder.join("esde_data/themes").exists());
    }

    /// Back up, then lose a save and restore it; every backed-up file is covered by restore.
    fn round_trip(c: &Connection, id: &str, backups: &Path, lost: &Path) -> Manifest {
        let b = run(c, id, &[], backups, &connected, &closed, &|_, _, _| {}).unwrap();
        assert!(b.complete, "{b:?}");
        let folder = PathBuf::from(&b.folder);
        let m = read_manifest(&folder).unwrap();
        assert!(m.files.iter().all(|f| covered(&m.adapter_id, f.area, &f.path)), "every backed-up file can go back");
        let kept = fs::read(lost).unwrap();
        fs::remove_file(lost).unwrap();
        let groups: Vec<String> = m.groups.clone();
        let p = crate::system_restore::preview(c, &folder, id, &groups, &connected, &closed).unwrap();
        assert_eq!(p.add, 1, "only the lost save");
        crate::system_restore::apply(c, &folder, id, &groups, false, &p.fingerprint, &connected, &closed).unwrap();
        assert_eq!(fs::read(lost).unwrap(), kept);
        m
    }

    fn installed(root: &Path, adapter: &str) -> (Connection, String) {
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, root, adapter, &|_: &str| VolumeProbe::unknown()).unwrap();
        (c, id)
    }

    fn paths(m: &Manifest) -> Vec<String> {
        let mut v: Vec<String> = m.files.iter().map(|f| format!("{}:{}", f.area.folder(), f.path)).collect();
        v.sort();
        v
    }

    #[test]
    fn batocera_backs_up_saves_settings_and_game_lists() {
        let dir = tempfile::tempdir().unwrap();
        let u = dir.path().join("batocera");
        put(&u, "system/batocera.conf", "global.ratio=4/3");
        put(&u, "system/configs/emulationstation/es_settings.cfg", "<config />");
        put(&u, "system/configs/emulationstation/collections/custom-Fav.cfg", "a");
        put(&u, "system/configs/emulationstation/themes/x/theme.xml", "theme");
        put(&u, "saves/snes/F-Zero.srm", "save");
        put(&u, "roms/snes/F-Zero.zip", "game");
        put(&u, "roms/snes/gamelist.xml", "<gameList />");
        put(&u, "bios/scph5501.bin", "bios");
        let (c, id) = installed(&u, "batocera");
        let m = round_trip(&c, &id, &dir.path().join("Backups"), &u.join("saves/snes/F-Zero.srm"));
        assert_eq!(paths(&m), ["install:roms/snes/gamelist.xml", "install:saves/snes/F-Zero.srm", "install:system/batocera.conf", "install:system/configs/emulationstation/collections/custom-Fav.cfg", "install:system/configs/emulationstation/es_settings.cfg"]);
    }

    #[test]
    fn recalbox_includes_its_per_game_settings_files() {
        let dir = tempfile::tempdir().unwrap();
        let sh = dir.path().join("share");
        put(&sh, "system/recalbox.conf", "global.ratio=auto");
        put(&sh, "system/.emulationstation/es_input.cfg", "<inputList />");
        put(&sh, "saves/nes/Zelda.srm", "save");
        put(&sh, "roms/nes/Zelda.nes", "game");
        put(&sh, "roms/nes/Zelda.nes.recalbox.conf", "nes.ratio=4/3");
        put(&sh, "roms/nes/gamelist.xml", "<gameList />");
        let (c, id) = installed(&sh, "recalbox");
        let m = round_trip(&c, &id, &dir.path().join("Backups"), &sh.join("roms/nes/Zelda.nes.recalbox.conf"));
        assert_eq!(paths(&m), ["install:roms/nes/Zelda.nes.recalbox.conf", "install:roms/nes/gamelist.xml", "install:saves/nes/Zelda.srm", "install:system/.emulationstation/es_input.cfg", "install:system/recalbox.conf"]);
    }

    #[test]
    fn retropie_and_rocknix_pick_saves_out_of_the_game_folders() {
        let dir = tempfile::tempdir().unwrap();
        let pie = dir.path().join("retropie");
        for f in ["roms/snes/Mario.sfc", "roms/snes/Mario.srm", "roms/snes/Mario.state1", "roms/snes/Mario.state1.png", "roms/snes/gamelist.xml", "roms/nes/Zelda.nes", "roms/ports/x.sh"] {
            put(&pie, f, f);
        }
        let (c, id) = installed(&pie, "retropie");
        let pv = preview(&c, &id, dir.path(), &connected, &closed).unwrap();
        let settings = pv.groups.iter().find(|g| g.id == "settings").unwrap();
        assert!(settings.files == 0 && settings.note.as_deref().unwrap().contains("configs network share"), "a USB stick has no configs share");
        let m = round_trip(&c, &id, &dir.path().join("Backups"), &pie.join("roms/snes/Mario.srm"));
        assert_eq!(paths(&m), ["games:snes/Mario.srm", "games:snes/Mario.state1", "games:snes/Mario.state1.png", "games:snes/gamelist.xml"], "games aren't saves");
        assert!(!covered("retropie", Area::Games, "snes/Mario.sfc"), "restore never writes a game");

        let card = dir.path().join("rocknix");
        for f in ["roms/snes/Mario.sfc", "roms/snes/Mario.srm", "roms/sfc/x.sfc", "roms/nes/Zelda.nes", "roms/fbn/x.zip", "roms/bezels/a.png", "roms/savestates/snes/Mario.state", "roms/savestates/snes/Mario.state.png"] {
            put(&card, f, f);
        }
        let (c, id) = installed(&card, "rocknix");
        let m = round_trip(&c, &id, &dir.path().join("Backups"), &card.join("roms/savestates/snes/Mario.state"));
        assert_eq!(paths(&m), ["games:savestates/snes/Mario.state", "games:savestates/snes/Mario.state.png", "games:snes/Mario.srm"]);
    }

    #[test]
    fn lakka_backs_up_its_save_settings_and_playlist_shares() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        put(host, "ROMs/nes/Zelda.nes", "game");
        put(host, "Configfiles/retroarch/retroarch.cfg", "video_smooth = false");
        put(host, "Configfiles/retroarch/config/Nestopia/Nestopia.cfg", "x");
        put(host, "Configfiles/kodi/big.db", "not RetroArch's");
        put(host, "Playlists/Nintendo - Nintendo Entertainment System.lpl", "{}");
        put(host, "Savefiles/Zelda.srm", "save");
        put(host, "Savestates/Zelda.state1", "state");
        put(host, "Remappings/Nestopia/Nestopia.rmp", "map");
        let (c, id) = installed(&host.join("ROMs"), "lakka");
        let m = round_trip(&c, &id, &host.join("Backups"), &host.join("Savefiles/Zelda.srm"));
        assert_eq!(paths(&m), ["configfiles:retroarch/config/Nestopia/Nestopia.cfg", "configfiles:retroarch/retroarch.cfg", "playlists:Nintendo - Nintendo Entertainment System.lpl", "remappings:Nestopia/Nestopia.rmp", "savefiles:Zelda.srm", "savestates:Zelda.state1"]);
    }

    #[test]
    fn save_files_are_told_apart_from_games() {
        for (name, save) in [
            ("Mario.srm", true),
            ("Pokemon.sav", true),
            ("Mario.state", true),
            ("Mario.state3", true),
            ("Mario.state.auto", true),
            ("Mario.state12.png", true),
            ("Mupen.eep", true),
            ("Mario.sfc", false),
            ("Mario.zip", false),
            ("cover.png", false),
            (".srm", false),
            ("Mario.statement.txt", false),
        ] {
            assert_eq!(is_save_file(name), save, "{name}");
        }
    }
}

