//! Saved platform installations and read-only placement previews (RDS-006/007).
//!
//! An installation is a folder an adapter recognized (e.g. a RetroBat folder). It
//! is stored like scanned folders: a fingerprinted storage location plus a relative
//! path, so a RetroBat on a USB drive is still recognized after a letter change.
//! Deployment plans for an installation are built in `deployment.rs`; nothing here writes game files.

use crate::adapters::{self, ArcadeSystem, Confidence, Detection, LayoutCheck, PlacementOptions};
use crate::volumes::{Presence, VolumeProbe};
use crate::{db, paths};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;
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
    /// `mame`, `fbneo` or `None` (arcade games not placed).
    pub arcade_system: Option<ArcadeSystem>,
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

pub(crate) struct Located {
    pub id: String,
    pub adapter_id: String,
    pub root: PathBuf,
    pub version: Option<String>,
    pub presence: Presence,
    pub added_at: String,
    pub options: PlacementOptions,
}

pub(crate) fn located(connection: &Connection, presence: PresenceFn, only: Option<&str>) -> Result<Vec<Located>, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.id, i.adapter_id, s.root_path, s.stable_id, i.relative_path, i.version_string, i.added_at, i.arcade_system
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
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id, adapter_id, root, stable, relative, version, added_at, arcade) = row.map_err(|e| e.to_string())?;
        let volume = presence(&root, stable.as_deref());
        let path = paths::join(&root, &relative);
        let presence = if volume == Presence::Connected && !path.is_dir() { Presence::NotConnected } else { volume };
        let options = PlacementOptions { arcade: arcade.as_deref().and_then(ArcadeSystem::parse) };
        Ok(Located { id, adapter_id, root: path, version, presence, added_at, options })
    })
    .collect()
}

pub fn list(connection: &Connection, presence: PresenceFn) -> Result<Vec<InstallationRow>, String> {
    Ok(located(connection, presence, None)?
        .into_iter()
        .map(|i| {
            let adapter = adapters::find(&i.adapter_id);
            let (checks, live_version) = match (&adapter, i.presence) {
                (Some(a), Presence::Connected) => (a.validate_layout(&i.root), a.detect(&i.root).version),
                _ => (Vec::new(), None),
            };
            InstallationRow {
                adapter_name: adapter.map(|a| a.display_name().to_string()).unwrap_or_else(|| i.adapter_id.clone()),
                id: i.id,
                adapter_id: i.adapter_id,
                path: i.root.display().to_string(),
                version: live_version.or(i.version),
                presence: i.presence,
                checks,
                added_at: i.added_at,
                arcade_system: i.options.arcade,
            }
        })
        .collect())
}

/// What Sync keeps up to date (ADR-045): system folders and collection ids chosen in
/// plans that were copied to this installation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncScope {
    pub systems: Vec<String>,
    pub collections: Vec<String>,
}

impl SyncScope {
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty() && self.collections.is_empty()
    }
    fn tidy(mut self) -> Self {
        for list in [&mut self.systems, &mut self.collections] {
            list.retain(|s| !s.trim().is_empty());
            list.sort();
            list.dedup();
        }
        self
    }
}

/// The recorded Sync scope, if one was saved.
pub fn saved_sync_scope(connection: &Connection, id: &str) -> Result<Option<SyncScope>, String> {
    let json: Option<String> = connection.query_row("SELECT sync_scope_json FROM installations WHERE id = ?1", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// Saves the Sync scope (catalog only).
pub fn set_sync_scope(connection: &Connection, id: &str, scope: &SyncScope) -> Result<(), String> {
    let json = serde_json::to_string(&scope.clone().tidy()).map_err(|e| e.to_string())?;
    let changed = connection.execute("UPDATE installations SET sync_scope_json = ?2 WHERE id = ?1", params![id, json]).map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Installation not found".into());
    }
    Ok(())
}

/// The user's saved decisions for this installation (ADR-044). Unreadable data counts as none.
pub fn saved_choices(connection: &Connection, id: &str) -> Result<crate::deployment::Choices, String> {
    let json: Option<String> = connection.query_row("SELECT choices_json FROM installations WHERE id = ?1", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

/// Saves the user's decisions. Only "copy this version" and "copy both" are kept:
/// "keep" is the default anyway, and "replace" is never remembered, so a later Sync
/// can't replace a file on the drive without the user choosing it again that time.
pub fn set_saved_choices(connection: &Connection, id: &str, choices: &crate::deployment::Choices) -> Result<(), String> {
    use crate::deployment::Choice;
    let kept: std::collections::BTreeMap<&String, &Choice> = choices.iter().filter(|(_, c)| matches!(c, Choice::Use { .. } | Choice::KeepBoth)).collect();
    let json = (!kept.is_empty()).then(|| serde_json::to_string(&kept).unwrap_or_default());
    let changed = connection.execute("UPDATE installations SET choices_json = ?2 WHERE id = ?1", params![id, json]).map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Installation not found".into());
    }
    Ok(())
}

/// Games the user removed from the drive and chose to keep off it, by destination key
/// (the destination path, lower-case, backslashes).
pub fn kept_off(connection: &Connection, id: &str) -> Result<std::collections::HashSet<String>, String> {
    let mut statement = connection.prepare("SELECT destination_key FROM sync_kept_off WHERE installation_id = ?1").map_err(|e| e.to_string())?;
    let keys = statement.query_map([id], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    Ok(keys)
}

/// Remember (or forget) that a game stays off the drive. Nothing on the drive changes.
pub fn set_kept_off(connection: &Connection, id: &str, key: &str, title: Option<&str>, kept: bool) -> Result<(), String> {
    let key = key.replace('/', "\\").to_lowercase();
    if kept {
        connection
            .execute("INSERT INTO sync_kept_off (installation_id, destination_key, title) VALUES (?1, ?2, ?3) ON CONFLICT DO UPDATE SET title = excluded.title", params![id, key, title])
            .map_err(|e| e.to_string())?;
    } else {
        connection.execute("DELETE FROM sync_kept_off WHERE installation_id = ?1 AND destination_key = ?2", params![id, key]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Choose which arcade emulator family's romset this installation uses
/// (`"mame"`, `"fbneo"`, or `None` to stop placing arcade games). Catalog only.
pub fn set_arcade_system(connection: &Connection, id: &str, system: Option<&str>) -> Result<(), String> {
    let value = match system {
        None => None,
        Some(s) => Some(ArcadeSystem::parse(s).ok_or_else(|| format!("Unknown arcade emulator: {s}"))?.as_str()),
    };
    let changed = connection
        .execute("UPDATE installations SET arcade_system = ?2 WHERE id = ?1", params![id, value])
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Installation not found".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::CheckStatus;
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

        assert_eq!(rows[0].arcade_system, None, "arcade isn't guessed");
        set_arcade_system(&c, &id, Some("fbneo")).unwrap();
        assert_eq!(list(&c, &connected).unwrap()[0].arcade_system, Some(ArcadeSystem::Fbneo));
        assert!(set_arcade_system(&c, &id, Some("neogeo")).is_err());
        assert!(set_arcade_system(&c, "missing", Some("mame")).is_err());
        set_arcade_system(&c, &id, None).unwrap();
        assert_eq!(list(&c, &connected).unwrap()[0].arcade_system, None);

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
}
