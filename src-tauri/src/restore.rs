//! Backups made by Replace, and undoing a replace (docs/18 §10, owner-approved
//! 2026-11-06).
//!
//! When the user chose Replace, the copier moved the drive's original file to
//! `.retrodrive/backup/<deployment>/…` before copying. Undo swaps the files back,
//! without deleting anything:
//!
//! 1. Re-check: drive confirmed by its fingerprint, both paths inside the
//!    installation, the backup is a plain file, and the file now on the drive is
//!    exactly the one RetroDrive copied (same size and SHA-256). Otherwise refuse.
//! 2. Journal `undoing` with the path the current file will move to
//!    (`.retrodrive/backup/undo-<item>/…`).
//! 3. Move the current file there (no-replace rename).
//! 4. Move the original back to its place (no-replace rename). If that fails, move
//!    the current file back and record the replace as still in place.
//! 5. Journal `undone`.
//!
//! A crash between steps is repaired at startup (`recover_undo`) from what's on the
//! drive, rolling back to the replaced state when in doubt. RetroDrive never offers
//! to delete backups; "Open backup folder" lets the user do that themselves.

use crate::copier::{self, BACKUP_DIR};
use crate::paths;
use crate::volumes::Presence;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct BackupRow {
    pub item_id: i64,
    pub game_title: String,
    /// Path on the drive, relative to the installation (`roms/nes/Game.nes`).
    pub path: String,
    /// `replaced` (the original is in the backup folder) or `undone`.
    pub state: String,
    /// Where the original is (replaced) or where RetroDrive's copy went (undone), relative to the installation.
    pub backup: Option<String>,
    pub backup_bytes: Option<i64>,
    pub when: String,
    /// Why undo isn't possible right now; `None` means it can be tried.
    pub blocked: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BackupList {
    pub installation_path: String,
    pub connected: bool,
    pub rows: Vec<BackupRow>,
    /// Total size of files in `.retrodrive/backup` (originals and undone copies).
    pub folder_bytes: i64,
}

struct Item {
    id: i64,
    title: String,
    destination: String,
    backup: Option<String>,
    undo: Option<String>,
    size: i64,
    sha: Option<String>,
    state: String,
    when: String,
    root: String,
    stable: Option<String>,
}

fn items(connection: &Connection, where_clause: &str, value: &dyn rusqlite::ToSql) -> Result<Vec<Item>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT i.id, i.game_title, i.destination_path, i.backup_path, i.undo_path, i.size_bytes, i.sha256, i.state, i.updated_at,
                    d.destination_root, d.destination_stable_id
             FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
             WHERE {where_clause} ORDER BY i.updated_at DESC, i.id DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([value], |r| {
            Ok(Item {
                id: r.get(0)?,
                title: r.get(1)?,
                destination: r.get(2)?,
                backup: r.get(3)?,
                undo: r.get(4)?,
                size: r.get(5)?,
                sha: r.get(6)?,
                state: r.get(7)?,
                when: r.get(8)?,
                root: r.get(9)?,
                stable: r.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn relative_to(path: &str, root: &str) -> String {
    path.get(root.len()..).filter(|_| copier::is_inside(path, root)).unwrap_or(path).trim_start_matches(['\\', '/']).replace('\\', "/")
}

fn backup_root(root: &str) -> String {
    format!("{}\\{}", root.trim_end_matches(['\\', '/']), BACKUP_DIR.replace('/', "\\"))
}

fn inside_backup_folder(path: &str, root: &str) -> bool {
    copier::is_inside(&path.replace('/', "\\"), &backup_root(root))
}

fn connected(item: &Item, presence: PresenceFn) -> bool {
    paths::split_volume(Path::new(&item.root)).is_some_and(|(volume, _)| presence(&volume, item.stable.as_deref()) == Presence::Connected)
}

fn plain_file(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path).ok().filter(|m| m.is_file() && !m.file_type().is_symlink()).map(|m| m.len())
}

fn folder_size(path: &Path) -> i64 {
    walkdir::WalkDir::new(path).follow_links(false).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).filter_map(|e| e.metadata().ok()).map(|m| m.len() as i64).sum()
}

/// Cheap checks for the list (no hashing): what would stop an undo now.
fn quick_block(item: &Item, online: bool) -> Option<String> {
    if !online {
        return Some("The drive isn't connected.".into());
    }
    let Some(backup) = item.backup.as_deref() else { return Some("No backup is recorded.".into()) };
    if !inside_backup_folder(backup, &item.root) || !copier::is_inside(&item.destination, &item.root) {
        return Some("The recorded paths aren't inside this installation.".into());
    }
    if plain_file(Path::new(backup)).is_none() {
        return Some("The original isn't in the backup folder any more.".into());
    }
    match plain_file(Path::new(&item.destination)) {
        None => Some("There's no file in its place on the drive any more.".into()),
        Some(len) if len as i64 != item.size => Some("The file on the drive changed since RetroDrive copied it.".into()),
        Some(_) => None,
    }
}

/// Replaced and undone files for one installation, newest first.
pub fn list(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<BackupList, String> {
    let found = items(connection, "d.installation_id = ?1 AND ((i.state = 'replaced' AND i.backup_path IS NOT NULL) OR i.state IN ('undone', 'undoing'))", &installation_id)?;
    let install = crate::installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    let installation_path = install.root.display().to_string();
    let online = install.presence == Presence::Connected;
    let rows = found
        .iter()
        .map(|item| {
            let (backup, bytes) = match item.state.as_str() {
                "replaced" => (item.backup.clone(), item.backup.as_deref().filter(|_| online).and_then(|b| plain_file(Path::new(b))).map(|n| n as i64)),
                _ => (item.undo.clone(), item.undo.as_deref().filter(|_| online).and_then(|u| plain_file(Path::new(u))).map(|n| n as i64)),
            };
            BackupRow {
                item_id: item.id,
                game_title: item.title.clone(),
                path: relative_to(&item.destination, &item.root),
                state: if item.state == "undoing" { "replaced".into() } else { item.state.clone() },
                backup: backup.map(|b| relative_to(&b, &item.root)),
                backup_bytes: bytes,
                when: item.when.clone(),
                blocked: match item.state.as_str() {
                    "replaced" => quick_block(item, online && connected(item, presence)),
                    "undoing" => Some("An undo was interrupted; restart RetroDrive with the drive connected to finish it.".into()),
                    _ => Some("Already undone.".into()),
                },
            }
        })
        .collect();
    let backups = install.root.join(".retrodrive").join("backup");
    let folder_bytes = if online && backups.is_dir() { folder_size(&backups) } else { 0 };
    Ok(BackupList { installation_path, connected: online, rows, folder_bytes })
}

/// The installation's backup folder, if it exists (for "Open backup folder").
pub fn backup_folder(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<PathBuf, String> {
    let install = crate::installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if install.presence != Presence::Connected {
        return Err("The drive isn't connected.".into());
    }
    let folder = install.root.join(".retrodrive").join("backup");
    match fs::symlink_metadata(&folder) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(folder),
        _ => Err("There's no backup folder on this drive yet.".into()),
    }
}

fn set(connection: &Connection, id: i64, state: &str, backup: Option<&str>, undo: Option<&str>, message: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE deployment_items SET state = ?2, backup_path = ?3, undo_path = ?4, message = ?5, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, state, backup, undo, message],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

const REPLACED: &str = "Replaced; the original file is in RetroDrive's backup folder on this drive.";

/// Undo one replace: the original goes back, RetroDrive's copy moves into the backup folder.
pub fn undo(connection: &Connection, item_id: i64, presence: PresenceFn) -> Result<String, String> {
    let item = items(connection, "i.id = ?1", &item_id)?.into_iter().next().ok_or("That backup wasn't found.")?;
    if item.state != "replaced" {
        return Err("Only a completed replace can be undone.".into());
    }
    let running: Option<String> = connection
        .query_row("SELECT d.id FROM deployments d JOIN deployment_items i ON i.deployment_id = d.id WHERE i.id = ?1 AND d.status = 'running'", [item_id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if running.is_some() {
        return Err("That copy is still running.".into());
    }
    if let Some(reason) = quick_block(&item, connected(&item, presence)) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let (backup, destination) = (PathBuf::from(item.backup.as_deref().unwrap_or_default()), PathBuf::from(&item.destination));
    // The file on the drive must be exactly the one RetroDrive copied.
    let Some(expected) = item.sha.as_deref() else { return Err("RetroDrive has no checksum for the copied file, so it can't confirm it's unchanged. Nothing was changed.".into()) };
    let actual = copier::sha256_of(&destination).map_err(|e| format!("Couldn't read the file on the drive: {e}. Nothing was changed."))?;
    if actual != expected {
        return Err("The file on the drive changed since RetroDrive copied it, so it wasn't touched. Nothing was changed.".into());
    }

    let relative = relative_to(&item.destination, &item.root);
    let undo_relative = format!("{BACKUP_DIR}/undo-{item_id}/{relative}");
    let root_path = PathBuf::from(&item.root);
    copier::ensure_folders(&root_path, &undo_relative).map_err(|e| format!("Couldn't prepare the backup folder: {e} Nothing was changed."))?;
    let undo_path = paths::join(&item.root, &undo_relative);
    if fs::symlink_metadata(&undo_path).is_ok() {
        return Err(format!("{} already exists, so nothing was changed.", undo_path.display()));
    }
    let backup_text = backup.display().to_string();
    let undo_text = undo_path.display().to_string();
    set(connection, item_id, "undoing", Some(&backup_text), Some(&undo_text), "Undoing the replace.")?;

    if let Err(e) = copier::rename_no_replace(&destination, &undo_path) {
        set(connection, item_id, "replaced", Some(&backup_text), None, REPLACED)?;
        return Err(format!("Couldn't move RetroDrive's copy aside, so nothing was changed: {e}"));
    }
    if let Err(e) = copier::rename_no_replace(&backup, &destination) {
        // Put RetroDrive's copy back where it was.
        if copier::rename_no_replace(&undo_path, &destination).is_ok() {
            set(connection, item_id, "replaced", Some(&backup_text), None, REPLACED)?;
            return Err(format!("Couldn't move the original back, so everything was left as it was: {e}"));
        }
        return Err(format!(
            "Couldn't move the original back ({e}), and RetroDrive's copy couldn't be put back either. Both files are safe: the original is {} and RetroDrive's copy is {}. Restart RetroDrive with the drive connected to finish.",
            backup.display(),
            undo_path.display()
        ));
    }
    let message = format!("Undone: the original is back. RetroDrive's copy is in {undo_relative}.");
    set(connection, item_id, "undone", None, Some(&undo_text), &message)?;
    Ok(message)
}

/// After a crash during an undo, finish or roll back from what's on the drive.
/// Only on the confirmed drive, only for paths inside the installation's backup folder.
pub fn recover_undo(connection: &Connection, presence: PresenceFn) -> Result<usize, String> {
    let pending = items(connection, "i.state = ?1", &"undoing")?;
    let mut fixed = 0;
    for item in pending {
        if !connected(&item, presence) {
            continue;
        }
        let (Some(backup), Some(undo)) = (item.backup.as_deref(), item.undo.as_deref()) else { continue };
        if !inside_backup_folder(backup, &item.root) || !inside_backup_folder(undo, &item.root) || !copier::is_inside(&item.destination, &item.root) {
            continue; // never move anything that isn't ours
        }
        let (backup_path, undo_path, destination) = (Path::new(backup), Path::new(undo), Path::new(&item.destination));
        let (has_backup, has_undo, has_dest) = (plain_file(backup_path).is_some(), plain_file(undo_path).is_some(), fs::symlink_metadata(destination).is_ok());
        match (has_dest, has_undo, has_backup) {
            // Both moves happened.
            (true, true, false) => {
                let undo_relative = relative_to(undo, &item.root);
                set(connection, item.id, "undone", None, Some(undo), &format!("Undone (finished before an interruption). RetroDrive's copy is in {undo_relative}."))?;
                fixed += 1;
            }
            // RetroDrive's copy was moved aside but the original didn't arrive: put the copy back.
            (false, true, true) => {
                if copier::rename_no_replace(undo_path, destination).is_ok() {
                    set(connection, item.id, "replaced", Some(backup), None, REPLACED)?;
                    fixed += 1;
                }
            }
            // Nothing moved yet.
            (true, false, true) => {
                set(connection, item.id, "replaced", Some(backup), None, REPLACED)?;
                fixed += 1;
            }
            _ => {} // unexpected: leave everything as it is for the user to check
        }
    }
    Ok(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copier::{deploy, DeployRequest};
    use crate::volumes::VolumeProbe;
    use crate::{db, deployment, installations, scanner};

    fn connected_drive(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn unplugged(_: &str, _: Option<&str>) -> Presence {
        Presence::NotConnected
    }
    fn roomy(_: &Path) -> Option<crate::volumes::VolumeSpace> {
        Some(crate::volumes::VolumeSpace { free_bytes: 500 << 30, total_bytes: 1 << 40, filesystem: Some("NTFS".into()), cluster_bytes: 4096 })
    }
    fn unknown(_: &str) -> VolumeProbe {
        VolumeProbe::unknown()
    }

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
    }

    /// A RetroBat with `roms/nes/Diff.nes` ("theirs"), replaced by the library's version ("ours").
    fn replaced() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        for d in ["emulationstation", "roms/nes", "bios"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("roms/nes/Diff.nes"), b"theirs, longer").unwrap();
        let library = dir.path().join("library");
        fs::create_dir_all(library.join("nes")).unwrap();
        fs::write(library.join("nes/Diff.nes"), b"ours").unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let install = installations::add(&catalog, &root, "retrobat", &unknown).unwrap();
        let plan = deployment::plan_with(&catalog, &install, None, None, &deployment::Choices::new(), &connected_drive, &roomy).unwrap();
        let key = plan.attention[0].choice.as_ref().unwrap().key.clone();
        let mut choices = deployment::Choices::new();
        choices.insert(key, deployment::Choice::Replace);
        let plan = deployment::plan_with(&catalog, &install, None, None, &choices, &connected_drive, &roomy).unwrap();
        let request = DeployRequest { installation_id: &install, systems: None, fingerprint: &plan.fingerprint, choices: &choices, collection: None };
        let summary = deploy(&catalog, None, &request, &connected_drive, &roomy, &mut Quiet).unwrap();
        assert_eq!(summary.files_replaced, 1);
        // The catalog stores the full path; temp folders can be 8.3 short names (CI), so use the same form.
        let root = paths::normalize(&root).unwrap();
        Setup { _dir: dir, root, catalog, install }
    }

    struct Quiet;
    impl copier::DeployObserver for Quiet {}

    #[test]
    fn undo_puts_the_original_back_and_keeps_retrodrives_copy() {
        let s = replaced();
        let listed = list(&s.catalog, &s.install, &connected_drive).unwrap();
        assert_eq!(listed.rows.len(), 1);
        let row = &listed.rows[0];
        assert_eq!((row.path.as_str(), row.state.as_str(), row.blocked.as_deref()), ("roms/nes/Diff.nes", "replaced", None));
        assert_eq!(row.backup_bytes, Some(14));
        assert!(listed.folder_bytes >= 14);

        undo(&s.catalog, row.item_id, &connected_drive).unwrap();
        assert_eq!(fs::read(s.root.join("roms/nes/Diff.nes")).unwrap(), b"theirs, longer", "the original is back");
        let kept = s.root.join(format!(".retrodrive/backup/undo-{}/roms/nes/Diff.nes", row.item_id));
        assert_eq!(fs::read(&kept).unwrap(), b"ours", "RetroDrive's copy is kept, not deleted");
        let after = list(&s.catalog, &s.install, &connected_drive).unwrap();
        assert_eq!(after.rows[0].state, "undone");
        assert!(after.rows[0].blocked.is_some());
        assert!(undo(&s.catalog, row.item_id, &connected_drive).is_err(), "can't undo twice");
    }

    #[test]
    fn undo_refuses_when_anything_changed() {
        let s = replaced();
        let id = list(&s.catalog, &s.install, &connected_drive).unwrap().rows[0].item_id;
        let snapshot = |root: &Path| -> Vec<(PathBuf, Vec<u8>)> {
            walkdir::WalkDir::new(root).sort_by_file_name().into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| (e.path().to_path_buf(), fs::read(e.path()).unwrap())).collect()
        };

        // Drive not connected.
        assert!(undo(&s.catalog, id, &unplugged).unwrap_err().contains("isn't connected"));
        // Same size, different contents: caught by the checksum.
        fs::write(s.root.join("roms/nes/Diff.nes"), b"OURS").unwrap();
        let before = snapshot(&s.root);
        assert!(undo(&s.catalog, id, &connected_drive).unwrap_err().contains("changed"));
        assert_eq!(snapshot(&s.root), before, "nothing was touched");
        // Backup gone.
        fs::write(s.root.join("roms/nes/Diff.nes"), b"ours").unwrap();
        let backup = s.catalog.query_row("SELECT backup_path FROM deployment_items WHERE id = ?1", [id], |r| r.get::<_, String>(0)).unwrap();
        fs::rename(&backup, s.root.join("moved-away")).unwrap();
        assert!(undo(&s.catalog, id, &connected_drive).unwrap_err().contains("isn't in the backup folder"));
        // A recorded path outside the installation is never used.
        s.catalog.execute("UPDATE deployment_items SET backup_path = ?2 WHERE id = ?1", params![id, s.root.join("moved-away").display().to_string()]).unwrap();
        assert!(undo(&s.catalog, id, &connected_drive).unwrap_err().contains("aren't inside"));
        let state: String = s.catalog.query_row("SELECT state FROM deployment_items WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(state, "replaced");
    }

    #[test]
    fn an_interrupted_undo_is_finished_or_rolled_back_at_startup() {
        let s = replaced();
        let id = list(&s.catalog, &s.install, &connected_drive).unwrap().rows[0].item_id;
        let (backup, destination): (String, String) = s.catalog.query_row("SELECT backup_path, destination_path FROM deployment_items WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let undo_path = s.root.join(format!(".retrodrive/backup/undo-{id}/roms/nes/Diff.nes"));
        fs::create_dir_all(undo_path.parent().unwrap()).unwrap();
        let journal = || set(&s.catalog, id, "undoing", Some(&backup), Some(&undo_path.display().to_string()), "Undoing the replace.").unwrap();
        let state = || s.catalog.query_row("SELECT state FROM deployment_items WHERE id = ?1", [id], |r| r.get::<_, String>(0)).unwrap();

        // Crash after the journal entry, before any move: back to replaced.
        journal();
        assert!(undo(&s.catalog, id, &connected_drive).is_err(), "an unfinished undo blocks a new one");
        assert_eq!(recover_undo(&s.catalog, &unplugged).unwrap(), 0, "nothing happens while the drive is away");
        assert_eq!(recover_undo(&s.catalog, &connected_drive).unwrap(), 1);
        assert_eq!(state(), "replaced");

        // Crash after the first move: RetroDrive's copy goes back.
        journal();
        fs::rename(&destination, &undo_path).unwrap();
        recover_undo(&s.catalog, &connected_drive).unwrap();
        assert_eq!((state().as_str(), fs::read(&destination).unwrap()), ("replaced", b"ours".to_vec()));
        assert!(Path::new(&backup).exists());

        // Crash after both moves: recorded as undone.
        journal();
        fs::rename(&destination, &undo_path).unwrap();
        fs::rename(&backup, &destination).unwrap();
        recover_undo(&s.catalog, &connected_drive).unwrap();
        assert_eq!((state().as_str(), fs::read(&destination).unwrap()), ("undone", b"theirs, longer".to_vec()));
        assert!(undo_path.exists());

        // The copier's own crash recovery leaves undo items alone.
        assert_eq!(copier::cleanup_leftovers(&s.catalog, &connected_drive).unwrap(), 0);
        assert_eq!(fs::read(&destination).unwrap(), b"theirs, longer");
    }
}
