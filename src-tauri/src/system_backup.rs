//! Backup, step 1 (FR-09, owner 2026-11-09 "Backup and restore"; ADR-090; docs/18 §56):
//! copy a system's saves, settings and game lists into a new folder on this PC.
//!
//! Reads only on the system. On the PC it only creates a new folder and new files in it
//! (`gamelists::copy_image`: temporary file, checked against the original, then named
//! without replacing anything). While copying, the folder's name ends in
//! "(incomplete)"; `manifest.json` (every file's size and SHA-256) is written last and
//! the folder is then renamed. Nothing is ever deleted: a backup that didn't finish
//! keeps its "(incomplete)" name and is listed as such.
//!
//! What is backed up (checked on the owner's RetroBat 8.3.1.1 and portable ES-DE 3.5.0):
//! - RetroBat: `saves\`; `retrobat.ini`; EmulationStation's own settings
//!   (`es_settings.cfg`, `es_input.cfg`, `gamecontrollerdb.txt`, `collections\`,
//!   `scripts\`, `themesettings\`) and `user\`; each system's `roms\<system>\gamelist.xml`.
//!   Optional: menu music and boot videos. Left out: themes, emulators, BIOS and games
//!   (they come with RetroBat or from the library).
//! - ES-DE: its data folder's `settings\`, `collections\`, `custom_systems\`,
//!   `controllers\`, `scripts\`, `gamelists\`; RetroArch's `saves\` and `states\` when
//!   RetroArch is in ES-DE's Emulators folder. Optional: `downloaded_media\` (art).
//!
//! Restore is step 2 and needs its own safety review.

use crate::customizer::RunningFn;
use crate::installations::{self, Located};
use crate::volumes::Presence;
use crate::{adapters, bios_step, copier, gamelists};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "manifest.json";
const INCOMPLETE: &str = " (incomplete)";
/// Manifest format; restore refuses formats it doesn't know.
const FORMAT: u32 = 1;

/// Where a backed-up file came from, so restore can find the place again even if the
/// drive letter changed: the installation folder, ES-DE's data folder, or RetroArch's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Install,
    EsdeData,
    Retroarch,
}

impl Area {
    fn folder(self) -> &'static str {
        match self {
            Area::Install => "install",
            Area::EsdeData => "esde_data",
            Area::Retroarch => "retroarch",
        }
    }
}

struct GroupDef {
    id: &'static str,
    label: &'static str,
    optional: bool,
}

const GROUPS: &[GroupDef] = &[
    GroupDef { id: "saves", label: "Saves", optional: false },
    GroupDef { id: "settings", label: "Settings", optional: false },
    GroupDef { id: "gamelists", label: "Game lists", optional: false },
    GroupDef { id: "media", label: "Music and boot videos", optional: true },
    GroupDef { id: "art", label: "Downloaded art", optional: true },
];

/// One file to back up.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    group: &'static str,
    area: Area,
    /// Below the area's folder, with `/`.
    rel: String,
    path: PathBuf,
    size: u64,
}

/// The folders an installation's files are found in.
struct Places {
    install: PathBuf,
    esde_data: Option<PathBuf>,
    retroarch: Option<PathBuf>,
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

fn supported(adapter: &str) -> bool {
    adapter == "retrobat" || adapter == "esde"
}

fn located(connection: &Connection, installation_id: &str, presence: crate::customizer::PresenceFn) -> Result<Located, String> {
    let l = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if !supported(&l.adapter_id) {
        return Err("Backups are available for RetroBat and ES-DE so far.".into());
    }
    if l.presence != Presence::Connected {
        return Err("The drive isn't connected.".into());
    }
    Ok(l)
}

fn places(l: &Located) -> Result<Places, String> {
    Ok(match l.adapter_id.as_str() {
        "esde" => Places {
            install: l.root.clone(),
            esde_data: Some(gamelists::esde_data_dir(&l.root).ok_or("ES-DE's data folder wasn't found next to this ROMs folder. Start ES-DE once, then try again.")?),
            retroarch: bios_step::esde_retroarch(&l.root),
        },
        _ => Places { install: l.root.clone(), esde_data: None, retroarch: None },
    })
}

/// Files below `dir` (recursively), skipping links, RetroDrive's own folder and its
/// temporary files.
fn tree(out: &mut Vec<Source>, group: &'static str, area: Area, base: &Path, rel_dir: &str) {
    let dir = crate::paths::join(&base.display().to_string(), rel_dir);
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

const ES: &str = "emulationstation/.emulationstation";

fn sources(adapter: &str, p: &Places) -> Vec<Source> {
    let mut out = Vec::new();
    match adapter {
        "retrobat" => {
            let root = &p.install;
            tree(&mut out, "saves", Area::Install, root, "saves");
            file(&mut out, "settings", Area::Install, root, "retrobat.ini");
            for f in ["es_settings.cfg", "es_input.cfg", "gamecontrollerdb.txt"] {
                file(&mut out, "settings", Area::Install, root, &format!("{ES}/{f}"));
            }
            for d in ["collections", "scripts", "themesettings"] {
                tree(&mut out, "settings", Area::Install, root, &format!("{ES}/{d}"));
            }
            tree(&mut out, "settings", Area::Install, root, "user");
            if let Ok(systems) = fs::read_dir(root.join("roms")) {
                let mut names: Vec<String> = systems.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                names.sort();
                for name in names {
                    file(&mut out, "gamelists", Area::Install, root, &format!("roms/{name}/gamelist.xml"));
                }
            }
            tree(&mut out, "media", Area::Install, root, &format!("{ES}/music"));
            tree(&mut out, "media", Area::Install, root, &format!("{ES}/video"));
        }
        "esde" => {
            if let Some(data) = &p.esde_data {
                for d in ["settings", "collections", "custom_systems", "controllers", "scripts"] {
                    tree(&mut out, "settings", Area::EsdeData, data, d);
                }
                tree(&mut out, "gamelists", Area::EsdeData, data, "gamelists");
                tree(&mut out, "art", Area::EsdeData, data, "downloaded_media");
            }
            if let Some(ra) = &p.retroarch {
                tree(&mut out, "saves", Area::Retroarch, ra, "saves");
                tree(&mut out, "saves", Area::Retroarch, ra, "states");
            }
        }
        _ => {}
    }
    out
}

/// The groups this adapter has, with what each would back up.
fn group_previews(adapter: &str, p: &Places, all: &[Source]) -> Vec<GroupPreview> {
    let wanted: &[&str] = if adapter == "esde" { &["saves", "settings", "gamelists", "art"] } else { &["saves", "settings", "gamelists", "media"] };
    GROUPS
        .iter()
        .filter(|g| wanted.contains(&g.id))
        .map(|g| {
            let files: Vec<&Source> = all.iter().filter(|s| s.group == g.id).collect();
            let note = (files.is_empty()).then(|| match (adapter, g.id) {
                ("esde", "saves") if p.retroarch.is_none() => "RetroArch isn't in ES-DE's Emulators folder, so there are no RetroArch saves here.".to_string(),
                _ => "Nothing to back up.".to_string(),
            });
            GroupPreview { id: g.id, label: g.label, optional: g.optional, files: files.len(), bytes: files.iter().map(|s| s.size).sum(), note }
        })
        .collect()
}

fn chosen(adapter: &str, optional: &[String]) -> Vec<&'static str> {
    let wanted: &[&str] = if adapter == "esde" { &["saves", "settings", "gamelists", "art"] } else { &["saves", "settings", "gamelists", "media"] };
    GROUPS.iter().filter(|g| wanted.contains(&g.id) && (!g.optional || optional.iter().any(|o| o == g.id))).map(|g| g.id).collect()
}

fn adapter_name(id: &str) -> String {
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
fn safe_name(text: &str) -> String {
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
fn free_folder(destination: &Path, name: &str) -> Result<String, String> {
    for n in 1..1000 {
        let candidate = if n == 1 { name.to_string() } else { format!("{name} ({n})") };
        let taken = |s: &str| fs::symlink_metadata(destination.join(s)).is_ok();
        if !taken(&candidate) && !taken(&format!("{candidate}{INCOMPLETE}")) {
            return Ok(candidate);
        }
    }
    Err("Couldn't find a free folder name for the backup.".into())
}

fn now(connection: &Connection, format: &str) -> String {
    connection.query_row("SELECT strftime(?1, 'now', 'localtime')", [format], |r| r.get(0)).unwrap_or_default()
}

fn write_manifest(folder: &Path, manifest: &Manifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
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

fn read_manifest(folder: &Path) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(folder.join(MANIFEST)).ok()?).ok()
}

fn entry(folder: &Path) -> Option<BackupEntry> {
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
}
