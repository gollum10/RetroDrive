//! Maintenance checks (FR-10; owner 2026-11-09 "great let's do it"; ADR-093; docs/18 §59):
//! one **Check health** per system. Reads only, and reuses what RetroDrive already has:
//! the Sync plan (space, games gone or different, new games), the adapter's layout checks,
//! the BIOS checker, the backup list, and a look for unfinished temporary files and the
//! size of RetroDrive's own backup folder. Each finding names the existing feature that
//! fixes it (Sync, Backup, the BIOS checker, Open backup folder); nothing here writes.

use crate::adapters::{self, CheckStatus};
use crate::customizer::PresenceFn;
use crate::deployment::{self, SpaceFn};
use crate::volumes::Presence;
use crate::{bios_check, copier, installations, system_backup};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

/// A backup older than this many days is reported.
const BACKUP_AGE_DAYS: f64 = 30.0;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HealthItem {
    pub id: &'static str,
    pub label: &'static str,
    /// `ok`, `info`, `warning` or `problem`.
    pub status: &'static str,
    pub summary: String,
    /// The feature that helps: `sync`, `plan`, `backup`, `bios`, `open_backup_folder`.
    pub action: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct Health {
    pub system: String,
    pub location: String,
    pub items: Vec<HealthItem>,
}

fn item(id: &'static str, label: &'static str, status: &'static str, summary: String, action: Option<&'static str>) -> HealthItem {
    HealthItem { id, label, status, summary, action }
}

fn size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else {
        format!("{:.0} KB", (b / 1e3).max(1.0))
    }
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

/// Check one system (reads only). `backups`: the folder backups go into.
pub fn check(connection: &Connection, installation_id: &str, backups: &Path, presence: PresenceFn, space: SpaceFn) -> Result<Health, String> {
    let l = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if l.presence != Presence::Connected {
        return Err("The drive isn't connected, so it can't be checked.".into());
    }
    let adapter = adapters::find(&l.adapter_id).ok_or("This system isn't supported.")?;
    let name = adapter.display_name().to_string();
    let mut items = Vec::new();

    // 1–2. Space and games, from the Sync plan (what Sync would do right now).
    let choices = installations::saved_choices(connection, installation_id).unwrap_or_else(|_| deployment::Choices::new());
    match deployment::plan_sync(connection, installation_id, &choices, presence, space) {
        Ok(plan) => {
            let c = &plan.capacity;
            let free = if c.measured { format!("{} free of {}", size(c.free_bytes), size(c.total_bytes)) } else { "free space unknown".to_string() };
            let (status, summary) = match c.fits {
                Some(false) => ("problem", format!("{free}. The next Sync needs about {}, which doesn't fit.", size(c.required_bytes))),
                _ if c.measured && c.total_bytes > 0 && c.free_bytes * 20 < c.total_bytes => ("warning", format!("{free}: the drive is almost full.")),
                _ if plan.files_to_copy > 0 => ("ok", format!("{free}. The next Sync needs about {} and fits.", size(c.required_bytes))),
                _ => ("ok", format!("{free}.")),
            };
            items.push(item("space", "Space", status, summary, None));

            let g = &plan.games;
            let mut notes = Vec::new();
            let mut status = "ok";
            if g.removed > 0 {
                status = "warning";
                notes.push(format!("{} copied before {} gone from the drive", plural(g.removed, "game", "games"), if g.removed == 1 { "is" } else { "are" }));
            }
            // The plan's own counts (its list of games needing attention is capped).
            if g.conflict > 0 {
                status = "warning";
                notes.push(format!("{} on the drive {} from your library", plural(g.conflict, "game", "games"), if g.conflict == 1 { "differs" } else { "differ" }));
            }
            if g.blocked > 0 {
                status = "warning";
                notes.push(format!("{} can't be copied here", plural(g.blocked, "game", "games")));
            }
            if g.copy > 0 {
                if status == "ok" {
                    status = "info";
                }
                notes.push(format!("{} in your library {} not on the drive yet", plural(g.copy, "new game", "new games"), if g.copy == 1 { "is" } else { "are" }));
            }
            let summary = if notes.is_empty() { format!("All {} games RetroDrive keeps in sync are on the drive.", (g.already_there + g.in_place).max(0)) } else { format!("{}. Sync shows each one and lets you choose.", notes.join("; ")) };
            items.push(item("games", "Games", status, summary, (status != "ok").then_some("sync")));
        }
        Err(e) if e == deployment::NOTHING_SYNCED => {
            let free = space(&l.root).map(|s| format!("{} free of {}.", size(s.free_bytes as i64), size(s.total_bytes as i64))).unwrap_or_else(|| "Free space unknown.".into());
            items.push(item("space", "Space", "ok", free, None));
            items.push(item("games", "Games", "info", "Nothing is kept in sync on this system yet, so there's nothing to compare. Use Plan deployment to choose what goes on it.".into(), Some("plan")));
        }
        Err(e) => items.push(item("games", "Games", "warning", format!("Couldn't compare with your library: {e}"), None)),
    }

    // 3. Folder layout: the adapter's own checks.
    let checks = adapter.validate_layout(&l.root);
    let bad: Vec<String> = checks.iter().filter(|c| c.status != CheckStatus::Ok).map(|c| format!("{}: {}", c.label, c.detail)).collect();
    let status = if checks.iter().any(|c| c.status == CheckStatus::Problem) { "problem" } else if bad.is_empty() { "ok" } else { "warning" };
    items.push(item("layout", "Folders", status, if bad.is_empty() { format!("All {} layout checks passed.", checks.len()) } else { bad.join(" · ") }, None));

    // 4. BIOS files.
    match bios_check::check(connection, Some(installation_id), presence) {
        Ok(b) => {
            let missing = b.systems.iter().filter(|s| s.status == "missing").count() as i64;
            let not_on = b.systems.iter().filter(|s| s.status == "not_on_system").count() as i64;
            let (status, summary) = if missing + not_on > 0 {
                let mut parts = Vec::new();
                if missing > 0 {
                    parts.push(format!("{} BIOS missing", plural(missing, "system has its", "systems have their")));
                }
                if not_on > 0 {
                    parts.push(format!("{} in your library but not on {name}", plural(not_on, "system's BIOS is", "systems' BIOS are")));
                }
                ("warning", format!("{}. The BIOS checker in Customize lists them.", parts.join("; ")))
            } else if b.systems.is_empty() {
                ("ok", "None of the systems in your library need BIOS files.".to_string())
            } else {
                ("ok", format!("Every system that needs a BIOS has one ({} checked).", b.systems.len()))
            };
            let summary = match &b.system_note {
                Some(note) if status == "ok" => format!("{summary} {note}"),
                _ => summary,
            };
            items.push(item("bios", "BIOS", status, summary, (status != "ok").then_some("bios")));
        }
        Err(e) => items.push(item("bios", "BIOS", "info", format!("Couldn't check: {e}"), None)),
    }

    // 5. Backups.
    if !system_backup::rules(&l.adapter_id).is_empty() {
        let newest = system_backup::list(backups).into_iter().find(|b| b.installation_id.as_deref() == Some(installation_id) && b.complete);
        let backup = match newest.and_then(|b| b.created_at) {
            None => item("backup", "Backup", "warning", format!("No backup of {name}'s saves and settings yet."), Some("backup")),
            Some(when) => {
                let days: f64 = connection.query_row("SELECT julianday('now', 'localtime') - julianday(?1)", [&when], |r| r.get(0)).unwrap_or(0.0);
                let ago = if days < 1.0 { "today".to_string() } else { format!("{} ago", plural(days.floor() as i64, "day", "days")) };
                if days > BACKUP_AGE_DAYS {
                    item("backup", "Backup", "warning", format!("The last backup was {ago}."), Some("backup"))
                } else {
                    item("backup", "Backup", "ok", format!("The last backup was {ago}."), None)
                }
            }
        };
        items.push(backup);
    }

    // 6. Leftovers: unfinished temporary files, and RetroDrive's own backup folder.
    let mut temps = 0i64;
    let mut kept_bytes = 0i64;
    let backup_dir = l.root.join(".retrodrive").join("backup");
    for entry in walkdir::WalkDir::new(&l.root).follow_links(false).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy().contains(copier::TEMP_MARKER) {
            temps += 1;
        } else if entry.path().starts_with(&backup_dir) {
            kept_bytes += entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        }
    }
    let mut parts = Vec::new();
    if temps > 0 {
        parts.push(format!("{} from an interrupted copy (RetroDrive tidies its own up on the next copy here)", plural(temps, "unfinished temporary file", "unfinished temporary files")));
    }
    if kept_bytes > 0 {
        parts.push(format!("RetroDrive's backup folder on this drive holds {} of files it moved aside (replaced or restored); you can delete them yourself if you don't need them", size(kept_bytes)));
    }
    items.push(if parts.is_empty() {
        item("leftovers", "Leftovers", "ok", "No unfinished copies or files moved aside.".into(), None)
    } else {
        item("leftovers", "Leftovers", "info", format!("{}.", parts.join("; ")), (kept_bytes > 0).then_some("open_backup_folder"))
    });

    Ok(Health { system: name, location: l.root.display().to_string(), items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{VolumeProbe, VolumeSpace};
    use std::fs;

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn roomy(_: &Path) -> Option<VolumeSpace> {
        Some(VolumeSpace { free_bytes: 500_000_000_000, total_bytes: 1_000_000_000_000, filesystem: Some("NTFS".into()), cluster_bytes: 4096 })
    }
    fn full(_: &Path) -> Option<VolumeSpace> {
        Some(VolumeSpace { free_bytes: 10_000_000, total_bytes: 1_000_000_000_000, filesystem: Some("NTFS".into()), cluster_bytes: 4096 })
    }
    fn listing(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| (e.path().display().to_string(), fs::read(e.path()).unwrap())).collect();
        out.sort();
        out
    }
    fn retrobat(root: &Path) {
        for f in ["RetroBat.exe", "retrobat.ini"] {
            put(root, f, "x");
        }
        put(root, "emulationstation/.emulationstation/es_settings.cfg", "<config />");
        put(root, "emulationstation/emulationstation.exe", "x");
        put(root, "saves/snes/F-Zero.srm", "save");
        put(root, "roms/snes/F-Zero.zip", "game");
        put(root, "bios/x.txt", "x");
        put(root, "emulators/x.txt", "x");
    }
    fn get<'a>(h: &'a Health, id: &str) -> &'a HealthItem {
        h.items.iter().find(|i| i.id == id).unwrap()
    }

    #[test]
    fn a_new_system_gets_every_check_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        retrobat(&root);
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let before = listing(dir.path());
        let h = check(&c, &id, &dir.path().join("Backups"), &connected, &roomy).unwrap();
        assert_eq!(h.items.iter().map(|i| i.id).collect::<Vec<_>>(), ["space", "games", "layout", "bios", "backup", "leftovers"]);
        assert_eq!(get(&h, "space").status, "ok");
        assert_eq!((get(&h, "games").status, get(&h, "games").action), ("info", Some("plan")), "nothing synced yet");
        assert_eq!((get(&h, "backup").status, get(&h, "backup").action), ("warning", Some("backup")));
        assert_eq!(get(&h, "leftovers").status, "ok");
        assert_eq!(listing(dir.path()), before, "checking writes nothing");
        assert!(check(&c, &id, dir.path(), &connected, &full).is_ok());
    }

    #[test]
    fn backups_and_leftovers_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        retrobat(&root);
        put(&root, "roms/snes/Half.zip.rdpart-1234", "partial");
        put(&root, ".retrodrive/backup/restore-1/saves/snes/F-Zero.srm", "older save");
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let backups = dir.path().join("Backups");
        system_backup::run(&c, &id, &[], &backups, &connected, &|| false, &|_, _, _| {}).unwrap();
        let h = check(&c, &id, &backups, &connected, &roomy).unwrap();
        assert_eq!((get(&h, "backup").status, get(&h, "backup").summary.as_str()), ("ok", "The last backup was today."));
        let left = get(&h, "leftovers");
        assert_eq!((left.status, left.action), ("info", Some("open_backup_folder")));
        assert!(left.summary.contains("1 unfinished temporary file") && left.summary.contains("backup folder"), "{}", left.summary);
    }
}
