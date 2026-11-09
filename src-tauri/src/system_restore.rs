//! Restore, step 2 of system backups (FR-09; owner 2026-11-09 "Yes, build it as
//! planned"; ADR-091; docs/18 §57): put files from a backup back onto a RetroBat or
//! ES-DE of the same kind.
//!
//! - **Preview first** (reads only): per file "add" (missing on the system), "same"
//!   (identical, skipped) or "different". Different files keep the system's copy unless
//!   the user chooses the backup's; then the system's file is moved to
//!   `.retrodrive/backup/restore-<id>/…` (never deleted).
//! - **Only places a backup covers.** Every path in the manifest is checked: relative,
//!   no `..`, not in `.retrodrive`, and inside what a backup of that system contains
//!   (saves, settings, game lists…), so an edited manifest can't write elsewhere.
//! - **Every backup file is checked** against its SHA-256 before it's used; each copy
//!   goes through a temporary file, is checked again, and is named without replacing.
//! - **Journal first:** `.retrodrive/restores/<id>.json` lists every planned change
//!   before anything is written. **Undo** works from what is on the disk (so it also
//!   repairs an interrupted restore): an added file still identical to the backup's is
//!   moved to `.retrodrive/backup/restore-<id>-undo/…`; a replaced file is moved aside
//!   the same way and the original moved back. Files changed since the restore are kept
//!   and reported. Nothing is deleted.
//! - Refused while RetroBat or ES-DE runs.

use crate::customizer::{PresenceFn, RunningFn};
use crate::system_backup::{self, Area, Manifest, Places, ES, FORMAT, GROUPS};
use crate::{copier, db, gamelists};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const JOURNALS: &str = ".retrodrive/restores";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Add,
    Same,
    Different,
    /// The place for it doesn't exist on this system (e.g. no RetroArch).
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RestoreGroup {
    pub id: String,
    pub label: String,
    pub add: usize,
    pub same: usize,
    pub different: usize,
    pub unavailable: usize,
}

#[derive(Debug, Serialize)]
pub struct RestorePreview {
    pub backup: String,
    pub created_at: String,
    pub system: String,
    pub from_location: String,
    pub target_location: String,
    pub same_system: bool,
    pub groups: Vec<RestoreGroup>,
    pub add: usize,
    pub same: usize,
    pub different: usize,
    pub unavailable: usize,
    pub bytes_add: u64,
    pub bytes_different: u64,
    /// First different files (path below its folder).
    pub different_files: Vec<String>,
    pub unavailable_note: Option<String>,
    pub blocked: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct RestoreSummary {
    pub id: String,
    pub added: usize,
    pub replaced: usize,
    pub kept: usize,
    pub failed: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct JournalItem {
    area: Area,
    path: String,
    /// `add` or `replace`.
    action: String,
    sha256: String,
    /// Where the system's own file was moved (replace), below the area's folder.
    moved_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    format: u32,
    id: String,
    created_at: String,
    backup_folder: String,
    backup_name: String,
    installation_id: String,
    /// `applying`, `applied`, `undone`.
    state: String,
    items: Vec<JournalItem>,
    #[serde(default)]
    notes: Vec<String>,
}

/// A restore, as listed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RestoreRecord {
    pub id: String,
    pub created_at: String,
    pub backup: String,
    pub added: usize,
    pub replaced: usize,
    pub state: String,
}

/// A manifest path is relative and plain: no `..`, drive or share, nothing of RetroDrive's.
fn plain(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    !path.is_empty()
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains([':', '\\']) && !p.chars().any(char::is_control) && !p.contains(copier::TEMP_MARKER))
        && !parts[0].eq_ignore_ascii_case(".retrodrive")
}

/// Whether a backup of this kind of system covers this place (the same places
/// `system_backup::sources` reads).
fn allowed(adapter: &str, area: Area, path: &str) -> bool {
    if !plain(path) {
        return false;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let under = |prefix: &str| path.len() > prefix.len() + 1 && path.starts_with(prefix) && path.as_bytes()[prefix.len()] == b'/';
    match (adapter, area) {
        ("retrobat", Area::Install) => {
            path == "retrobat.ini"
                || under("saves")
                || under("user")
                || ["es_settings.cfg", "es_input.cfg", "gamecontrollerdb.txt"].iter().any(|f| path == format!("{ES}/{f}"))
                || ["collections", "scripts", "themesettings", "music", "video"].iter().any(|d| under(&format!("{ES}/{d}")))
                || (parts.len() == 3 && parts[0] == "roms" && parts[2] == "gamelist.xml")
        }
        ("esde", Area::EsdeData) => parts.len() >= 2 && ["settings", "collections", "custom_systems", "controllers", "scripts", "gamelists", "downloaded_media"].contains(&parts[0]),
        ("esde", Area::Retroarch) => parts.len() >= 2 && ["saves", "states"].contains(&parts[0]),
        _ => false,
    }
}

fn base(p: &Places, area: Area) -> Option<&Path> {
    match area {
        Area::Install => Some(&p.install),
        Area::EsdeData => p.esde_data.as_deref(),
        Area::Retroarch => p.retroarch.as_deref(),
    }
}

fn target_path(base: &Path, rel: &str) -> PathBuf {
    crate::paths::join(&base.display().to_string(), rel)
}

fn backup_file(folder: &Path, area: Area, rel: &str) -> PathBuf {
    crate::paths::join(&folder.join(area.folder()).display().to_string(), rel)
}

struct Planned {
    file: system_backup::ManifestFile,
    action: Action,
}

struct Loaded {
    manifest: Manifest,
    places: Places,
    located: crate::installations::Located,
    planned: Vec<Planned>,
}

/// Read the backup, check it and the target, and decide what each file needs (reads only).
fn load(connection: &Connection, backup: &Path, installation_id: &str, groups: &[String], presence: PresenceFn) -> Result<Loaded, String> {
    let manifest = system_backup::read_manifest(backup).ok_or("This folder has no readable backup list (manifest.json).")?;
    if manifest.format != FORMAT {
        return Err("This backup was made by a newer RetroDrive. Update RetroDrive to restore it.".into());
    }
    if !manifest.complete {
        return Err("This backup didn't finish, so it isn't offered for restoring. Make a new backup.".into());
    }
    let located = system_backup::located(connection, installation_id, presence)?;
    if located.adapter_id != manifest.adapter_id {
        return Err(format!("This is a backup of {}; it can only go back onto {}.", manifest.adapter_name, manifest.adapter_name));
    }
    if let Some(bad) = manifest.files.iter().find(|f| !allowed(&manifest.adapter_id, f.area, &f.path)) {
        return Err(format!("The backup lists a file outside what a backup contains ({}), so nothing will be restored from it.", bad.path));
    }
    let places = system_backup::places(&located)?;
    let mut planned = Vec::new();
    let wanted: Vec<&system_backup::ManifestFile> = manifest.files.iter().filter(|f| groups.contains(&f.group)).collect();
    // The backup's own files must still be what was backed up.
    let checks = gamelists::parallel_map(
        &wanted,
        "checking the backup",
        |f| {
            let src = backup_file(backup, f.area, &f.path);
            fs::metadata(&src).is_ok_and(|m| m.is_file() && m.len() == f.size) && copier::sha256_of(&src).ok().as_deref() == Some(f.sha256.as_str())
        },
        &|_, _, _| {},
    );
    let broken: Vec<&str> = wanted.iter().zip(&checks).filter(|(_, ok)| !**ok).map(|(f, _)| f.path.as_str()).collect();
    if !broken.is_empty() {
        return Err(format!("{} file{} in the backup changed or went missing since it was made (e.g. {}), so nothing will be restored from it.", broken.len(), if broken.len() == 1 { "" } else { "s" }, broken[0]));
    }
    let actions = gamelists::parallel_map(
        &wanted,
        "comparing",
        |f| {
            let Some(base) = base(&places, f.area) else { return Action::Unavailable };
            let target = target_path(base, &f.path);
            match fs::symlink_metadata(&target) {
                Err(_) => Action::Add,
                Ok(m) if !m.is_file() => Action::Unavailable,
                Ok(m) if m.len() == f.size && copier::sha256_of(&target).ok().as_deref() == Some(f.sha256.as_str()) => Action::Same,
                Ok(_) => Action::Different,
            }
        },
        &|_, _, _| {},
    );
    for (f, action) in wanted.into_iter().zip(actions) {
        planned.push(Planned { file: f.clone(), action });
    }
    Ok(Loaded { manifest, places, located, planned })
}

fn fingerprint(l: &Loaded) -> String {
    let mut h = Sha256::new();
    for p in &l.planned {
        h.update(format!("{:?}|{}|{:?}|{}\n", p.file.area, p.file.path, p.action, p.file.sha256));
    }
    format!("{:x}", h.finalize())[..16].to_string()
}

fn blocked(adapter: &str, running: RunningFn) -> Option<String> {
    running().then(|| format!("{} is running. Close it first: it saves its own settings and game lists when it closes, which would undo a restore.", system_backup::adapter_name(adapter)))
}

/// What restoring these groups would do (reads only).
pub fn preview(connection: &Connection, backup: &Path, installation_id: &str, groups: &[String], presence: PresenceFn, running: RunningFn) -> Result<RestorePreview, String> {
    let l = load(connection, backup, installation_id, groups, presence)?;
    let count = |a: Action| l.planned.iter().filter(|p| p.action == a).count();
    let bytes = |a: Action| l.planned.iter().filter(|p| p.action == a).map(|p| p.file.size).sum();
    let groups_out = GROUPS
        .iter()
        .filter(|g| l.planned.iter().any(|p| p.file.group == g.id))
        .map(|g| {
            let n = |a: Action| l.planned.iter().filter(|p| p.file.group == g.id && p.action == a).count();
            RestoreGroup { id: g.id.into(), label: g.label.into(), add: n(Action::Add), same: n(Action::Same), different: n(Action::Different), unavailable: n(Action::Unavailable) }
        })
        .collect();
    let unavailable = count(Action::Unavailable);
    Ok(RestorePreview {
        backup: backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        created_at: l.manifest.created_at.clone(),
        system: l.manifest.adapter_name.clone(),
        from_location: l.manifest.location.clone(),
        target_location: l.located.root.display().to_string(),
        same_system: l.manifest.installation_id == l.located.id,
        groups: groups_out,
        add: count(Action::Add),
        same: count(Action::Same),
        different: count(Action::Different),
        unavailable,
        bytes_add: bytes(Action::Add),
        bytes_different: bytes(Action::Different),
        different_files: l.planned.iter().filter(|p| p.action == Action::Different).take(50).map(|p| p.file.path.clone()).collect(),
        unavailable_note: (unavailable > 0).then(|| {
            if l.places.retroarch.is_none() && l.planned.iter().any(|p| p.action == Action::Unavailable && p.file.area == Area::Retroarch) {
                "RetroArch isn't in this ES-DE's Emulators folder, so its saves can't go back yet.".to_string()
            } else {
                "A folder or link is where a file would go, so those files are left out.".to_string()
            }
        }),
        blocked: blocked(&l.manifest.adapter_id, running),
        fingerprint: fingerprint(&l),
    })
}

fn journal_path(install: &Path, id: &str) -> PathBuf {
    target_path(install, &format!("{JOURNALS}/{id}.json"))
}

fn write_journal(install: &Path, j: &Journal) -> Result<(), String> {
    copier::ensure_folders(install, &format!("{JOURNALS}/{}.json", j.id))?;
    let path = journal_path(install, &j.id);
    let temp = PathBuf::from(format!("{}{}journal", path.display(), copier::TEMP_MARKER));
    fs::write(&temp, serde_json::to_string_pretty(j).map_err(|e| e.to_string())?).map_err(|e| format!("Couldn't write RetroDrive's restore record: {e}"))?;
    fs::rename(&temp, &path).map_err(|e| format!("Couldn't save RetroDrive's restore record: {e}"))
}

fn read_journal(install: &Path, id: &str) -> Option<Journal> {
    serde_json::from_str(&fs::read_to_string(journal_path(install, id)).ok()?).ok()
}

/// Copy one backup file into place (new name only), creating folders without following links.
fn place(base: &Path, rel: &str, source: &Path) -> Result<(), String> {
    copier::ensure_folders(base, rel)?;
    gamelists::copy_image(source, &target_path(base, rel))
}

/// Restore the groups. `use_backup`: for different files, use the backup's copy (the
/// system's is moved aside) instead of keeping the system's.
#[allow(clippy::too_many_arguments)]
pub fn apply(connection: &Connection, backup: &Path, installation_id: &str, groups: &[String], use_backup: bool, expected: &str, presence: PresenceFn, running: RunningFn) -> Result<RestoreSummary, String> {
    let l = load(connection, backup, installation_id, groups, presence)?;
    if let Some(reason) = blocked(&l.manifest.adapter_id, running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    if fingerprint(&l) != expected {
        return Err("The system or the backup changed since you reviewed this, so nothing was changed. Review it again.".into());
    }
    let id = format!("{}-{}", system_backup::now(connection, "%Y%m%d-%H%M%S"), &db::new_id()[..6]);
    let mut items = Vec::new();
    let mut kept = 0;
    for p in &l.planned {
        match p.action {
            Action::Add => items.push(JournalItem { area: p.file.area, path: p.file.path.clone(), action: "add".into(), sha256: p.file.sha256.clone(), moved_to: None }),
            Action::Different if use_backup => items.push(JournalItem { area: p.file.area, path: p.file.path.clone(), action: "replace".into(), sha256: p.file.sha256.clone(), moved_to: Some(format!(".retrodrive/backup/restore-{id}/{}", p.file.path)) }),
            Action::Different => kept += 1,
            _ => {}
        }
    }
    if items.is_empty() {
        return Err(if kept > 0 { "Nothing to do: every file is already there, and you chose to keep the system's copies of the different ones.".into() } else { "Nothing to do: every file in the backup is already on the system.".into() });
    }
    let need: u64 = l.planned.iter().filter(|p| items.iter().any(|i| i.path == p.file.path && i.area == p.file.area)).map(|p| p.file.size).sum();
    if let Some(space) = crate::volumes::space(&l.places.install) {
        if space.free_bytes < need + 16 * 1024 * 1024 {
            return Err("There isn't enough free space on the system's drive for this restore. Nothing was changed.".into());
        }
    }
    let install = l.places.install.clone();
    let mut journal = Journal {
        format: 1,
        id: id.clone(),
        created_at: system_backup::now(connection, "%Y-%m-%dT%H:%M:%S"),
        backup_folder: backup.display().to_string(),
        backup_name: backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        installation_id: l.located.id.clone(),
        state: "applying".into(),
        items: items.clone(),
        notes: Vec::new(),
    };
    // Journal first: nothing is written to the system before this record exists.
    write_journal(&install, &journal)?;
    let (mut added, mut replaced, mut failed) = (0, 0, Vec::new());
    for item in &items {
        let Some(base) = base(&l.places, item.area) else { continue };
        let source = backup_file(backup, item.area, &item.path);
        let result = if item.action == "add" {
            place(base, &item.path, &source).map(|_| added += 1)
        } else {
            let moved = item.moved_to.as_deref().unwrap_or_default();
            let target = target_path(base, &item.path);
            copier::ensure_folders(base, moved)
                .and_then(|_| copier::rename_no_replace(&target, &target_path(base, moved)).map_err(|e| format!("couldn't move the system's copy aside: {e}")))
                .and_then(|_| match place(base, &item.path, &source) {
                    Ok(()) => {
                        replaced += 1;
                        Ok(())
                    }
                    Err(e) => {
                        // Put the system's own file back where it was.
                        let _ = copier::rename_no_replace(&target_path(base, moved), &target);
                        Err(e)
                    }
                })
        };
        if let Err(e) = result {
            failed.push(format!("{} ({e})", item.path));
        }
    }
    journal.state = "applied".into();
    journal.notes = failed.clone();
    write_journal(&install, &journal)?;
    let name = &l.manifest.adapter_name;
    let message = format!(
        "Restored: {added} file{} added, {replaced} replaced (the system's copies are in .retrodrive\\backup\\restore-{id}){}{}. Undo is under Show backups.",
        if added == 1 { "" } else { "s" },
        if kept > 0 { format!(", {kept} different file{} kept as {name} had {}", if kept == 1 { "" } else { "s" }, if kept == 1 { "it" } else { "them" }) } else { String::new() },
        if failed.is_empty() { String::new() } else { format!(", {} couldn't be restored", failed.len()) },
    );
    Ok(RestoreSummary { id, added, replaced, kept, failed, message })
}

/// This system's restores, newest first (reads only).
pub fn list(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Vec<RestoreRecord>, String> {
    let l = system_backup::located(connection, installation_id, presence)?;
    let dir = target_path(&l.root, JOURNALS);
    let Ok(entries) = fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut out: Vec<RestoreRecord> = entries
        .filter_map(Result::ok)
        .filter_map(|e| serde_json::from_str::<Journal>(&fs::read_to_string(e.path()).ok()?).ok())
        .filter(|j| j.installation_id == l.id)
        .map(|j| RestoreRecord {
            added: j.items.iter().filter(|i| i.action == "add").count(),
            replaced: j.items.iter().filter(|i| i.action == "replace").count(),
            id: j.id,
            created_at: j.created_at,
            backup: j.backup_name,
            state: j.state,
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    Ok(out)
}

/// Undo a restore from what is on the disk; works for an interrupted restore too.
pub fn undo(connection: &Connection, installation_id: &str, restore_id: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    if !restore_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("That restore wasn't found.".into());
    }
    let l = system_backup::located(connection, installation_id, presence)?;
    if let Some(reason) = blocked(&l.adapter_id, running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let places = system_backup::places(&l)?;
    let mut j = read_journal(&l.root, restore_id).ok_or("That restore wasn't found.")?;
    if j.installation_id != l.id {
        return Err("That restore belongs to another system.".into());
    }
    if j.state == "undone" {
        return Err("That restore was already undone.".into());
    }
    let aside = format!(".retrodrive/backup/restore-{}-undo", j.id);
    let (mut undone, mut kept) = (0, Vec::new());
    for item in &j.items {
        if !allowed(&l.adapter_id, item.area, &item.path) || item.moved_to.as_deref().is_some_and(|m| !m.starts_with(".retrodrive/backup/restore-") || m.contains("..")) {
            kept.push(format!("{} (not a place RetroDrive restores to)", item.path));
            continue;
        }
        let Some(base) = base(&places, item.area) else { continue };
        let target = target_path(base, &item.path);
        let current = fs::symlink_metadata(&target).ok().filter(|m| m.is_file()).and_then(|_| copier::sha256_of(&target).ok());
        let original = item.moved_to.as_deref().map(|m| target_path(base, m)).filter(|p| p.is_file());
        // The restored file, still exactly as restored: move it aside.
        if current.as_deref() == Some(item.sha256.as_str()) {
            let rel = format!("{aside}/{}", item.path);
            if let Err(e) = copier::ensure_folders(base, &rel).and_then(|_| copier::rename_no_replace(&target, &target_path(base, &rel)).map_err(|e| e.to_string())) {
                kept.push(format!("{} ({e})", item.path));
                continue;
            }
        } else if current.is_some() {
            kept.push(format!("{} (changed since the restore; kept)", item.path));
            continue;
        }
        // Put the system's own file back.
        if let Some(original) = original {
            if let Err(e) = copier::rename_no_replace(&original, &target) {
                kept.push(format!("{} (couldn't put the original back: {e})", item.path));
                continue;
            }
        }
        undone += 1;
    }
    j.state = if kept.is_empty() { "undone".into() } else { "partly undone".into() };
    j.notes = kept.clone();
    write_journal(&l.root, &j)?;
    Ok(if kept.is_empty() {
        format!("Undone: {undone} file{} put back as before. The restored copies are in .retrodrive\\backup\\restore-{}-undo.", if undone == 1 { "" } else { "s" }, j.id)
    } else {
        format!("Partly undone: {undone} put back; {} kept: {}.", kept.len(), kept.iter().take(3).cloned().collect::<Vec<_>>().join("; "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installations;
    use crate::volumes::{Presence, VolumeProbe};

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
        let mut out: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(root).into_iter().filter_entry(|e| e.file_name() != ".retrodrive").filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| (e.path().strip_prefix(root).unwrap().display().to_string(), fs::read(e.path()).unwrap())).collect();
        out.sort();
        out
    }
    fn all() -> Vec<String> {
        ["saves", "settings", "gamelists", "media"].iter().map(|s| s.to_string()).collect()
    }

    struct S {
        dir: tempfile::TempDir,
        c: Connection,
        id: String,
        root: PathBuf,
        backup: PathBuf,
    }

    /// A RetroBat, backed up.
    fn setup() -> S {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        for f in ["RetroBat.exe", "retrobat.ini"] {
            put(&root, f, "ini");
        }
        put(&root, "emulationstation/.emulationstation/es_settings.cfg", "<config />");
        put(&root, "emulationstation/emulationstation.exe", "x");
        put(&root, "saves/snes/F-Zero.srm", "level 1");
        put(&root, "saves/snes/Mario.srm", "world 2");
        put(&root, "roms/snes/gamelist.xml", "<gameList />");
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let b = system_backup::run(&c, &id, &[], &dir.path().join("Backups"), &connected, &closed, &|_, _, _| {}).unwrap();
        S { backup: PathBuf::from(b.folder), dir, c, id, root }
    }

    #[test]
    fn the_preview_sorts_files_into_add_same_and_different() {
        let s = setup();
        fs::remove_file(s.root.join("saves/snes/Mario.srm")).unwrap();
        fs::write(s.root.join("saves/snes/F-Zero.srm"), "level 9").unwrap();
        let before = listing(&s.root);
        let p = preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).unwrap();
        assert_eq!((p.add, p.same, p.different, p.unavailable), (1, 3, 1, 0));
        assert_eq!(p.different_files, ["saves/snes/F-Zero.srm"]);
        assert!(p.same_system && p.blocked.is_none());
        let saves = p.groups.iter().find(|g| g.id == "saves").unwrap();
        assert_eq!((saves.add, saves.same, saves.different), (1, 0, 1));
        assert_eq!(preview(&s.c, &s.backup, &s.id, &["settings".to_string()], &connected, &closed).unwrap().add, 0, "only the chosen groups");
        assert!(preview(&s.c, &s.backup, &s.id, &all(), &connected, &open).unwrap().blocked.is_some());
        assert_eq!(listing(&s.root), before, "a preview changes nothing");
    }

    #[test]
    fn restoring_adds_missing_files_and_keeps_different_ones_by_default() {
        let s = setup();
        fs::remove_file(s.root.join("saves/snes/Mario.srm")).unwrap();
        fs::write(s.root.join("saves/snes/F-Zero.srm"), "level 9").unwrap();
        let p = preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).unwrap();
        assert!(apply(&s.c, &s.backup, &s.id, &all(), false, &p.fingerprint, &connected, &open).unwrap_err().contains("Nothing was changed"));
        assert!(!s.root.join("saves/snes/Mario.srm").exists());
        let r = apply(&s.c, &s.backup, &s.id, &all(), false, &p.fingerprint, &connected, &closed).unwrap();
        assert_eq!((r.added, r.replaced, r.kept), (1, 0, 1), "{}", r.message);
        assert_eq!(fs::read_to_string(s.root.join("saves/snes/Mario.srm")).unwrap(), "world 2");
        assert_eq!(fs::read_to_string(s.root.join("saves/snes/F-Zero.srm")).unwrap(), "level 9", "the system's copy is kept");
        // A stale review is refused (the system changed: Mario is back).
        assert!(apply(&s.c, &s.backup, &s.id, &all(), false, &p.fingerprint, &connected, &closed).unwrap_err().contains("changed since"));
    }

    #[test]
    fn using_the_backups_copy_moves_the_systems_aside_and_undo_puts_everything_back() {
        let s = setup();
        fs::remove_file(s.root.join("saves/snes/Mario.srm")).unwrap();
        fs::write(s.root.join("saves/snes/F-Zero.srm"), "level 9").unwrap();
        let before = listing(&s.root);
        let p = preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).unwrap();
        let r = apply(&s.c, &s.backup, &s.id, &all(), true, &p.fingerprint, &connected, &closed).unwrap();
        assert_eq!((r.added, r.replaced, r.failed.len()), (1, 1, 0));
        assert_eq!(fs::read_to_string(s.root.join("saves/snes/F-Zero.srm")).unwrap(), "level 1");
        assert_eq!(fs::read_to_string(s.root.join(format!(".retrodrive/backup/restore-{}/saves/snes/F-Zero.srm", r.id))).unwrap(), "level 9", "the system's copy is kept aside");
        assert!(walkdir::WalkDir::new(&s.root).into_iter().filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().contains(copier::TEMP_MARKER)));
        let l = list(&s.c, &s.id, &connected).unwrap();
        assert_eq!((l.len(), l[0].state.as_str(), l[0].added, l[0].replaced), (1, "applied", 1, 1));

        assert!(undo(&s.c, &s.id, &r.id, &connected, &open).unwrap_err().contains("Nothing was changed"));
        let m = undo(&s.c, &s.id, &r.id, &connected, &closed).unwrap();
        assert!(m.starts_with("Undone: 2"), "{m}");
        assert_eq!(listing(&s.root), before, "exactly as before the restore");
        assert_eq!(list(&s.c, &s.id, &connected).unwrap()[0].state, "undone");
        assert!(undo(&s.c, &s.id, &r.id, &connected, &closed).is_err(), "only once");
        assert!(undo(&s.c, &s.id, "../../x", &connected, &closed).is_err());
    }

    #[test]
    fn undo_keeps_files_changed_since_the_restore() {
        let s = setup();
        fs::remove_file(s.root.join("saves/snes/Mario.srm")).unwrap();
        let p = preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).unwrap();
        let r = apply(&s.c, &s.backup, &s.id, &all(), false, &p.fingerprint, &connected, &closed).unwrap();
        fs::write(s.root.join("saves/snes/Mario.srm"), "played since").unwrap();
        let m = undo(&s.c, &s.id, &r.id, &connected, &closed).unwrap();
        assert!(m.contains("changed since the restore"), "{m}");
        assert_eq!(fs::read_to_string(s.root.join("saves/snes/Mario.srm")).unwrap(), "played since");
        assert_eq!(list(&s.c, &s.id, &connected).unwrap()[0].state, "partly undone");
    }

    #[test]
    fn a_damaged_or_edited_backup_restores_nothing() {
        let s = setup();
        fs::remove_file(s.root.join("saves/snes/Mario.srm")).unwrap();
        // A backup file changed since the backup.
        fs::write(s.backup.join("install/saves/snes/Mario.srm"), "tampered").unwrap();
        assert!(preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).unwrap_err().contains("changed or went missing"));
        fs::write(s.backup.join("install/saves/snes/Mario.srm"), "world 2").unwrap();
        // A manifest pointing outside what a backup contains.
        let path = s.backup.join(system_backup::MANIFEST);
        let text = fs::read_to_string(&path).unwrap();
        for evil in ["emulators/retroarch/retroarch.exe", "../outside.txt", ".retrodrive/restores/x.json", "C:/Windows/x"] {
            fs::write(&path, text.replace("saves/snes/Mario.srm", evil)).unwrap();
            assert!(preview(&s.c, &s.backup, &s.id, &all(), &connected, &closed).is_err(), "{evil}");
        }
        assert!(!s.root.join("saves/snes/Mario.srm").exists());
    }

    #[test]
    fn the_allowed_places_match_what_a_backup_reads() {
        let s = setup();
        let m = system_backup::read_manifest(&s.backup).unwrap();
        assert!(m.files.iter().all(|f| allowed("retrobat", f.area, &f.path)), "every backed-up file can go back");
        for (adapter, area, path, ok) in [
            ("retrobat", Area::Install, "saves/x.srm", true),
            ("retrobat", Area::Install, "roms/snes/gamelist.xml", true),
            ("retrobat", Area::Install, "roms/snes/Game.zip", false),
            ("retrobat", Area::Install, "bios/scph5501.bin", false),
            ("retrobat", Area::Install, "saves", false),
            ("retrobat", Area::EsdeData, "settings/es_settings.xml", false),
            ("esde", Area::EsdeData, "settings/es_settings.xml", true),
            ("esde", Area::EsdeData, "themes/x/theme.xml", false),
            ("esde", Area::Retroarch, "states/a.state", true),
            ("esde", Area::Retroarch, "retroarch.exe", false),
        ] {
            assert_eq!(allowed(adapter, area, path), ok, "{adapter} {path}");
        }
        drop(s.dir);
    }
}
