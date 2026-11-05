//! Saved platform installations and read-only placement previews (RDS-006/007).
//!
//! An installation is a folder an adapter recognized (e.g. a RetroBat folder). It
//! is stored like scanned folders: a fingerprinted storage location plus a relative
//! path, so a RetroBat on a USB drive is still recognized after a letter change.
//! The placement preview shows where every catalog game *would* go; it never writes.

use crate::adapters::{self, Confidence, Detection, LayoutCheck, Placement, PlacementFile, PlacementRequest};
use crate::volumes::{Presence, VolumeProbe};
use crate::{db, paths};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;
type ProbeFn<'a> = &'a dyn Fn(&str) -> VolumeProbe;

/// Result of inspecting a folder before saving it.
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub path: String,
    pub detections: Vec<Detection>,
    /// The best confident match, if any.
    pub best: Option<Detection>,
    pub checks: Vec<LayoutCheck>,
}

#[derive(Debug, Serialize)]
pub struct InstallationRow {
    pub id: String,
    pub adapter_id: String,
    pub adapter_name: String,
    pub path: String,
    pub version: Option<String>,
    pub presence: Presence,
    /// Live layout checks (empty when the drive isn't connected).
    pub checks: Vec<LayoutCheck>,
    pub added_at: String,
}

#[derive(Debug, Serialize, Default)]
pub struct SystemPlan {
    pub system_folder: String,
    pub platforms: Vec<String>,
    pub games: i64,
    pub bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct UnsupportedGroup {
    pub platform: String,
    pub reason: String,
    pub games: i64,
}

#[derive(Debug, Serialize)]
pub struct PlannedGame {
    pub title: String,
    pub platform: String,
    pub source: String,
    /// Destination of the primary file, as a full path under the installation.
    pub destination: String,
    pub files: usize,
    pub bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct PlacementPreview {
    pub installation_id: String,
    pub adapter_name: String,
    pub installation_path: String,
    pub systems: Vec<SystemPlan>,
    pub unsupported: Vec<UnsupportedGroup>,
    /// Games skipped because their drive isn't connected or a file is missing.
    pub unavailable_games: i64,
    pub planned_games: i64,
    pub planned_bytes: i64,
    /// The first planned games, for a quick look at real destination paths.
    pub sample: Vec<PlannedGame>,
}

pub fn inspect(folder: &Path) -> Result<Inspection, String> {
    let root = paths::normalize(folder).map_err(|e| format!("Cannot open {}: {e}", folder.display()))?;
    if !root.is_dir() {
        return Err("The selected location is not a folder".into());
    }
    let detections = adapters::detect_all(&root);
    let best = detections.iter().find(|d| d.confidence == Confidence::Detected).cloned();
    let checks = match &best {
        Some(d) => adapters::find(d.adapter_id).map(|a| a.validate_layout(&root)).unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(Inspection { path: root.display().to_string(), detections, best, checks })
}

/// Save a recognized installation. Refuses folders the adapter doesn't detect.
pub fn add(connection: &Connection, folder: &Path, adapter_id: &str, probe: ProbeFn) -> Result<String, String> {
    let adapter = adapters::find(adapter_id).ok_or_else(|| format!("Unknown platform adapter: {adapter_id}"))?;
    let root = paths::normalize(folder).map_err(|e| format!("Cannot open {}: {e}", folder.display()))?;
    let detection = adapter.detect(&root);
    if detection.confidence != Confidence::Detected {
        return Err(format!("{} wasn't recognized in this folder, so it was not added.", adapter.display_name()));
    }
    let (volume_root, relative) = paths::split_volume(&root).ok_or_else(|| format!("Unsupported folder path: {}", root.display()))?;
    let location = db::resolve_storage_location(connection, &volume_root, &probe(&volume_root))?;
    if let Some(existing) = connection
        .query_row(
            "SELECT id FROM installations WHERE storage_location_id = ?1 AND relative_path = ?2",
            params![location.id, relative],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        connection
            .execute("UPDATE installations SET last_checked_at = CURRENT_TIMESTAMP, version_string = ?2 WHERE id = ?1", params![existing, detection.version])
            .map_err(|e| e.to_string())?;
        return Ok(existing);
    }
    let id = db::new_id();
    connection
        .execute(
            "INSERT INTO installations (id, adapter_id, adapter_contract, storage_location_id, relative_path, version_string, last_checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)",
            params![id, adapter_id, adapters::CONTRACT_VERSION, location.id, relative, detection.version],
        )
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Forget a saved installation (catalog record only; no files are touched).
pub fn remove(connection: &Connection, id: &str) -> Result<bool, String> {
    connection.execute("DELETE FROM installations WHERE id = ?1", [id]).map(|n| n > 0).map_err(|e| e.to_string())
}

struct Located {
    id: String,
    adapter_id: String,
    root: PathBuf,
    version: Option<String>,
    presence: Presence,
    added_at: String,
}

fn located(connection: &Connection, presence: PresenceFn, only: Option<&str>) -> Result<Vec<Located>, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.id, i.adapter_id, s.root_path, s.stable_id, i.relative_path, i.version_string, i.added_at
             FROM installations i JOIN storage_locations s ON s.id = i.storage_location_id
             WHERE ?1 IS NULL OR i.id = ?1 ORDER BY i.added_at",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([only], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id, adapter_id, root, stable, relative, version, added_at) = row.map_err(|e| e.to_string())?;
        let volume = presence(&root, stable.as_deref());
        let path = paths::join(&root, &relative);
        let presence = if volume == Presence::Connected && !path.is_dir() { Presence::NotConnected } else { volume };
        Ok(Located { id, adapter_id, root: path, version, presence, added_at })
    })
    .collect()
}

pub fn list(connection: &Connection, presence: PresenceFn) -> Result<Vec<InstallationRow>, String> {
    Ok(located(connection, presence, None)?
        .into_iter()
        .map(|i| {
            let adapter = adapters::find(&i.adapter_id);
            let checks = match (&adapter, i.presence) {
                (Some(a), Presence::Connected) => a.validate_layout(&i.root),
                _ => Vec::new(),
            };
            InstallationRow {
                adapter_name: adapter.map(|a| a.display_name().to_string()).unwrap_or_else(|| i.adapter_id.clone()),
                id: i.id,
                adapter_id: i.adapter_id,
                path: i.root.display().to_string(),
                version: i.version,
                presence: i.presence,
                checks,
                added_at: i.added_at,
            }
        })
        .collect())
}

/// Where every available catalog game would go in an installation. Read-only.
pub fn placement_preview(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<PlacementPreview, String> {
    let install = located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    let adapter = adapters::find(&install.adapter_id).ok_or_else(|| format!("Unknown platform adapter: {}", install.adapter_id))?;

    // Discs and tracks per release, with their volume-relative paths.
    let mut members: HashMap<String, Vec<(String, i64, String, String)>> = HashMap::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT gf.release_id, fl.relative_path, gf.size_bytes, gf.file_role, fl.present_status
                 FROM game_files gf JOIN file_locations fl ON fl.game_file_id = gf.id WHERE gf.file_role <> 'primary'",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (release, relative, size, role, status) = row.map_err(|e| e.to_string())?;
            members.entry(release).or_default().push((relative, size, role, status));
        }
    }

    let mut statement = connection
        .prepare(
            "SELECT g.canonical_title, g.platform, gf.extension, gf.release_id, gf.size_bytes, fl.relative_path, fl.present_status,
                    fl.storage_location_id, s.root_path, s.stable_id
             FROM game_files gf
             JOIN game_releases r ON r.id = gf.release_id
             JOIN games g ON g.id = r.game_id
             JOIN file_locations fl ON fl.game_file_id = gf.id
             JOIN storage_locations s ON s.id = fl.storage_location_id
             WHERE gf.file_role = 'primary'
             ORDER BY g.canonical_title COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, Option<String>>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut connected: HashMap<String, bool> = HashMap::new();
    let mut systems: BTreeMap<String, SystemPlan> = BTreeMap::new();
    let mut unsupported: BTreeMap<(String, String), i64> = BTreeMap::new();
    let mut sample = Vec::new();
    let (mut unavailable, mut planned_games, mut planned_bytes) = (0i64, 0i64, 0i64);

    for row in rows {
        let (title, platform, extension, release, size, relative, status, storage, root, stable) = row.map_err(|e| e.to_string())?;
        let online = *connected.entry(storage).or_insert_with(|| presence(&root, stable.as_deref()) == Presence::Connected);
        let parts = release.as_ref().and_then(|r| members.get(r)).cloned().unwrap_or_default();
        if !online || status != "present" || parts.iter().any(|m| m.3 != "present") {
            unavailable += 1;
            continue;
        }
        let primary_dir = relative.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
        let primary_name = relative.rsplit('/').next().unwrap_or(&relative).to_string();
        let mut files = vec![PlacementFile { relative_to_primary: primary_name, size }];
        for (member_relative, member_size, _, _) in &parts {
            let relative_to_primary = member_relative
                .strip_prefix(&primary_dir)
                .map(str::to_string)
                .unwrap_or_else(|| format!("../{member_relative}"));
            files.push(PlacementFile { relative_to_primary, size: *member_size });
        }
        let bytes: i64 = files.iter().map(|f| f.size).sum();
        let request = PlacementRequest { title: title.clone(), platform: platform.clone(), extension, files };
        match adapter.plan_placement(&request) {
            Placement::Planned { system_folder, destinations } => {
                let entry = systems.entry(system_folder.clone()).or_insert_with(|| SystemPlan { system_folder, ..SystemPlan::default() });
                if !entry.platforms.contains(&platform) {
                    entry.platforms.push(platform.clone());
                }
                entry.games += 1;
                entry.bytes += bytes;
                planned_games += 1;
                planned_bytes += bytes;
                if sample.len() < 50 {
                    sample.push(PlannedGame {
                        title,
                        platform,
                        source: paths::join(&root, &relative).display().to_string(),
                        destination: paths::join(&install.root.display().to_string(), &destinations[0]).display().to_string(),
                        files: request.files.len(),
                        bytes,
                    });
                }
            }
            Placement::Unsupported { reason } => *unsupported.entry((platform, reason)).or_default() += 1,
        }
    }

    Ok(PlacementPreview {
        installation_id: install.id,
        adapter_name: adapter.display_name().to_string(),
        installation_path: install.root.display().to_string(),
        systems: systems.into_values().collect(),
        unsupported: unsupported.into_iter().map(|((platform, reason), games)| UnsupportedGroup { platform, reason, games }).collect(),
        unavailable_games: unavailable,
        planned_games,
        planned_bytes,
        sample,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::CheckStatus;
    use crate::scanner;
    use std::fs;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn unknown(_: &str) -> VolumeProbe {
        VolumeProbe::unknown()
    }

    fn catalog() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        c
    }

    fn retrobat(dir: &Path) -> PathBuf {
        let root = dir.join("RetroBat");
        for d in ["emulationstation", "emulators", "roms", "bios"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("retrobat.ini"), b"").unwrap();
        root
    }

    fn write(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn inspect_recognizes_retrobat_and_runs_checks() {
        let dir = tempfile::tempdir().unwrap();
        let root = retrobat(dir.path());
        let inspection = inspect(&root).unwrap();
        assert_eq!(inspection.best.as_ref().map(|d| d.adapter_id), Some("retrobat"));
        assert!(inspection.checks.iter().any(|c| c.id == "roms" && c.status == CheckStatus::Ok));
        let plain = inspect(dir.path()).unwrap();
        assert!(plain.best.is_none());
        assert!(plain.checks.is_empty());
    }

    #[test]
    fn add_list_and_forget_installations() {
        let dir = tempfile::tempdir().unwrap();
        let root = retrobat(dir.path());
        let c = catalog();
        let id = add(&c, &root, "retrobat", &unknown).unwrap();
        assert_eq!(add(&c, &root, "retrobat", &unknown).unwrap(), id, "adding twice keeps one record");
        let rows = list(&c, &connected).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].adapter_name, "RetroBat");
        assert_eq!(rows[0].presence, Presence::Connected);
        assert!(!rows[0].checks.is_empty());
        assert!(Path::new(&rows[0].path).ends_with("RetroBat"));

        let offline = list(&c, &|_, _| Presence::NotConnected).unwrap();
        assert!(offline[0].checks.is_empty(), "no live checks for an unplugged drive");

        assert!(remove(&c, &id).unwrap());
        assert!(root.join("RetroBat.exe").exists(), "forgetting never touches files");
        assert!(list(&c, &connected).unwrap().is_empty());
    }

    #[test]
    fn unrecognized_folders_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let c = catalog();
        assert!(add(&c, dir.path(), "retrobat", &unknown).unwrap_err().contains("wasn't recognized"));
        assert!(add(&c, dir.path(), "nope", &unknown).is_err());
    }

    #[test]
    fn preview_groups_games_by_system_and_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let root = retrobat(dir.path());
        let library = dir.path().join("library");
        write(&library, "nes/Kid.nes", b"nes");
        write(&library, "nes/Disk.fds", b"fds");
        write(&library, "snes/Quest.sfc", b"snes!");
        write(&library, "arcade/pacman.zip", b"zip");
        write(&library, "psx/Saga/Saga.m3u", b"Saga (Disc 1).cue\nSaga (Disc 2).cue\n");
        write(&library, "psx/Saga/Saga (Disc 1).cue", b"FILE \"d1.bin\" BINARY\n");
        write(&library, "psx/Saga/Saga (Disc 2).cue", b"FILE \"d2.bin\" BINARY\n");
        write(&library, "psx/Saga/d1.bin", &[0u8; 100]);
        write(&library, "psx/Saga/d2.bin", &[0u8; 200]);
        let mut c = catalog();
        scanner::scan(&mut c, &library, &unknown).unwrap();
        let id = add(&c, &root, "retrobat", &unknown).unwrap();
        let before: Vec<_> = walkdir::WalkDir::new(&root).into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();

        let preview = placement_preview(&c, &id, &connected).unwrap();
        let folders: Vec<(&str, i64)> = preview.systems.iter().map(|s| (s.system_folder.as_str(), s.games)).collect();
        assert_eq!(folders, [("fds", 1), ("nes", 1), ("psx", 1), ("snes", 1)]);
        assert_eq!(preview.planned_games, 4);
        assert_eq!(preview.unsupported.len(), 1);
        assert_eq!(preview.unsupported[0].platform, "Arcade");
        let saga = preview.sample.iter().find(|g| g.title == "Saga").unwrap();
        assert_eq!(saga.files, 5, "playlist, two cue sheets and two tracks");
        assert!(saga.destination.replace('\\', "/").ends_with("RetroBat/roms/psx/Saga/Saga.m3u"), "{}", saga.destination);
        assert!(saga.bytes >= 300);

        let after: Vec<_> = walkdir::WalkDir::new(&root).into_iter().map(|e| e.unwrap().path().to_path_buf()).collect();
        assert_eq!(before, after, "the preview must not create anything in the installation");

        // Missing files and unplugged drives are left out of the plan.
        fs::remove_file(library.join("psx/Saga/d2.bin")).unwrap();
        scanner::scan(&mut c, &library, &unknown).unwrap();
        let preview = placement_preview(&c, &id, &connected).unwrap();
        assert_eq!(preview.unavailable_games, 1, "an incomplete game isn't planned");
        assert_eq!(preview.planned_games, 3);
    }
}
