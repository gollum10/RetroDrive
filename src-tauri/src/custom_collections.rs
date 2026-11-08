//! Light gun and vertical groups on RetroPie and ES-DE, filled from the library
//! (owner 2026-11-07: "in RetroPie you can … add a platform called Light Gun games, the
//! pain is that I then have to select which games go in there … we should be able to
//! add them from the library. Same for vertical."; ADR-072; docs/18 §43).
//!
//! Both use EmulationStation custom collections: a file `custom-<name>.cfg` in the
//! collections folder with one game per line, shown when `<name>` is in the
//! `CollectionSystemsCustom` setting (comma list).
//! - RetroPie (`RetroPie/EmulationStation`, `CollectionSystemManager.cpp`): files in
//!   `~/.emulationstation/collections`, which RetroPie keeps in
//!   `/opt/retropie/configs/all/emulationstation` (the `configs` Samba share). Each line is
//!   the game's full path on the Pi, matched exactly, so lines end with LF only.
//! - ES-DE (`es-de/emulationstation-de`, `CollectionSystemsManager.cpp`): files in
//!   `<ES-DE data>/collections`; lines start with `%ROMPATH%/`, which ES-DE expands to its
//!   ROM folder.
//!
//! RetroDrive lists the games it copied to that system that are still there and are
//! light gun or vertical games. An existing collection file only gets lines added
//! (nothing is removed); turning a group off only takes its name out of the setting.

use crate::customizer::{self, ChangeRow, Install, PresenceFn, Preview, RunningFn};
use crate::game_groups::{Group, GroupsState, GROUPS};
use crate::volumes::Presence;
use crate::{gamelists, installations, retropie_settings};
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const KEY: &str = "CollectionSystemsCustom";
/// RetroPie's usual ROM folder, used when the Pi's own files don't show another.
const PI_DEFAULT_ROMS: &str = "/home/pi/RetroPie/roms";

/// The collection name RetroDrive uses for each group.
pub fn collection_name(group: &str) -> &'static str {
    if group == "lightgun" { "Light Gun Games" } else { "Vertical Games" }
}

struct Place {
    adapter: String,
    /// Journal root (RetroPie's configs share, or ES-DE's data folder).
    install: Install,
    settings_rel: &'static str,
    collections_rel: &'static str,
    /// Where the copied games are on the drive or share.
    games_root: PathBuf,
    /// The start of each line: `%ROMPATH%` (ES-DE) or the Pi's ROM folder.
    line_prefix: String,
    /// Told to the user when the Pi's ROM folder is assumed.
    note: Option<String>,
}

impl Place {
    fn path(&self, relative: &str) -> PathBuf {
        relative.split('/').fold(self.install.root.clone(), |p, part| p.join(part))
    }
    fn collection_rel(&self, group: &str) -> String {
        format!("{}custom-{}.cfg", self.collections_rel, collection_name(group))
    }
}

/// The Pi's ROM folder as RetroPie's own files show it (es_systems.cfg, other
/// collections, the log), or RetroPie's default.
fn pi_roms(es_dir: &Path) -> (String, bool) {
    let mut texts: Vec<String> = vec![fs::read_to_string(es_dir.join("es_systems.cfg")).unwrap_or_default()];
    if let Ok(entries) = fs::read_dir(es_dir.join("collections")) {
        texts.extend(entries.flatten().filter_map(|e| fs::read_to_string(e.path()).ok()));
    }
    texts.push(fs::read_to_string(es_dir.join("es_log.txt")).unwrap_or_default());
    for text in texts {
        if let Some(at) = text.find("/RetroPie/roms") {
            let start = text[..at].rfind(|c: char| c.is_whitespace() || "\"'>=".contains(c)).map(|i| i + 1).unwrap_or(0);
            let prefix = &text[start..at + "/RetroPie/roms".len()];
            if prefix.starts_with('/') && !prefix.contains(['\\', '<']) {
                return (prefix.to_string(), true);
            }
        }
    }
    (PI_DEFAULT_ROMS.to_string(), false)
}

fn place(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Place, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if located.presence != Presence::Connected {
        return Err("The installation isn't connected.".into());
    }
    let root_text = located.root.display().to_string();
    match located.adapter_id.as_str() {
        "retropie" => {
            let configs = retropie_settings::configs_root(&root_text).ok_or("On a RetroPie USB stick, RetroPie's collections are on the Pi itself, which RetroDrive can't reach. Add RetroPie by its network share (\\\\RETROPIE\\roms) for this.")?;
            let configs = PathBuf::from(configs);
            let es_dir = configs.join("all").join("emulationstation");
            if !es_dir.join("es_settings.cfg").is_file() {
                return Err(format!("{} can't be reached, or RetroPie hasn't created its settings yet. Make sure the Pi is on (start EmulationStation once).", es_dir.join("es_settings.cfg").display()));
            }
            let (line_prefix, found) = pi_roms(&es_dir);
            let note = (!found).then(|| format!("Games are listed as {PI_DEFAULT_ROMS}/…, RetroPie's usual ROM folder. If your Pi uses another user name than pi, they won't show."));
            Ok(Place {
                adapter: located.adapter_id,
                install: Install { id: located.id, root_text: configs.display().to_string(), root: configs, stable: None },
                settings_rel: "all/emulationstation/es_settings.cfg",
                collections_rel: "all/emulationstation/collections/",
                games_root: located.root,
                line_prefix,
                note,
            })
        }
        "esde" => {
            let data = gamelists::esde_data_dir(&located.root).ok_or("ES-DE's data folder wasn't found next to this ROMs folder. Start ES-DE once, then try again.")?;
            if !data.join("settings").join("es_settings.xml").is_file() {
                return Err("ES-DE's settings (settings\\es_settings.xml) are missing. Start ES-DE once so it creates them.".into());
            }
            Ok(Place {
                adapter: located.adapter_id,
                install: Install { id: located.id, root_text: data.display().to_string(), root: data, stable: None },
                settings_rel: "settings/es_settings.xml",
                collections_rel: "collections/",
                games_root: located.root,
                line_prefix: "%ROMPATH%".into(),
                note: None,
            })
        }
        _ => Err("Custom collections are for RetroPie and ES-DE.".into()),
    }
}

/// Games RetroDrive copied there that are still there: (game id, system folder, path below it).
fn copied(connection: &Connection, place: &Place) -> Result<Vec<(String, String, String)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.destination_path, r.game_id FROM deployment_items i
             JOIN deployments d ON d.id = i.deployment_id JOIN game_files gf ON gf.id = i.game_file_id
             JOIN game_releases r ON r.id = gf.release_id
             WHERE d.installation_id = ?1 AND gf.file_role = 'primary' AND i.state IN ('verified', 'identical', 'replaced') ORDER BY i.id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = statement
        .query_map(params![place.install.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let roms = format!("{}\\", gamelists::games_dir(&place.games_root).0.display().to_string().replace('/', "\\").trim_end_matches('\\'));
    let mut seen = BTreeMap::new();
    for (destination, game) in rows {
        let d = destination.replace('/', "\\");
        if d.len() <= roms.len() || !d[..roms.len()].eq_ignore_ascii_case(&roms) || !Path::new(&destination).is_file() {
            continue;
        }
        let inside = d[roms.len()..].replace('\\', "/");
        if let Some((system, rel)) = inside.split_once('/') {
            seen.insert(inside.to_lowercase(), (game, system.to_string(), rel.to_string()));
        }
    }
    Ok(seen.into_values().collect())
}

fn group_ids(connection: &Connection, group: &str) -> Result<std::sync::Arc<std::collections::HashSet<String>>, String> {
    if group == "lightgun" { crate::lightguns::game_ids(connection) } else { crate::vertical::game_ids(connection) }
}

fn enabled(text: &str) -> Result<Vec<String>, String> {
    Ok(customizer::values(&customizer::entries(text)?)?.get(KEY).map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default())
}

pub fn state(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<GroupsState, String> {
    let place = place(connection, installation_id, presence)?;
    let settings = customizer::read_text(&place.path(place.settings_rel))?;
    let on = enabled(&settings)?;
    let games = copied(connection, &place)?;
    let mut groups = Vec::new();
    for (id, label) in GROUPS {
        let ids = group_ids(connection, id)?;
        groups.push(Group { id, label, on: on.iter().any(|n| n == collection_name(id)), in_library: ids.len(), copied: Some(games.iter().filter(|(g, _, _)| ids.contains(g)).count()) });
    }
    Ok(GroupsState { groups, file_sha: fingerprint(&place)?, network: place.install.root_text.starts_with(r"\\"), note: place.note.clone() })
}

/// One hash over the settings file and RetroDrive's two collection files.
fn fingerprint(place: &Place) -> Result<String, String> {
    let mut all = customizer::read_text(&place.path(place.settings_rel))?;
    for (id, _) in GROUPS {
        all.push('\u{1}');
        all.push_str(&fs::read_to_string(place.path(&place.collection_rel(id))).unwrap_or_default());
    }
    Ok(customizer::hex_sha(all.as_bytes()))
}

/// A planned file write: relative path, full path, old text, new text, review row.
type Write = (String, PathBuf, String, String, ChangeRow);

fn plan(connection: &Connection, place: &Place, wanted: &BTreeMap<String, bool>) -> Result<Vec<Write>, String> {
    if wanted.is_empty() || wanted.keys().any(|k| !GROUPS.iter().any(|(g, _)| g == k)) {
        return Err("Choose light gun or vertical games.".into());
    }
    let games = copied(connection, place)?;
    let mut writes = Vec::new();
    for group in wanted.iter().filter(|(_, on)| **on).map(|(g, _)| g) {
        let ids = group_ids(connection, group)?;
        let lines: Vec<String> = games.iter().filter(|(g, _, _)| ids.contains(g)).map(|(_, system, rel)| format!("{}/{system}/{rel}", place.line_prefix)).collect();
        let rel = place.collection_rel(group);
        let path = place.path(&rel);
        let old = if path.is_file() { customizer::read_text(&path)? } else { String::new() };
        let have: BTreeSet<String> = old.lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
        let add: Vec<&String> = lines.iter().filter(|l| !have.contains(*l)).collect();
        if add.is_empty() {
            if have.is_empty() {
                return Err(format!("None of your {} are on this {} yet. Copy them first (Plan, or a collection of them from the Game Library).", collection_name(group).to_lowercase(), label(&place.adapter)));
            }
            continue;
        }
        let mut new = old.clone();
        if !new.is_empty() && !new.ends_with('\n') {
            new.push('\n');
        }
        for l in &add {
            new.push_str(l);
            new.push('\n');
        }
        let row = ChangeRow { system: collection_name(group).into(), key: "games".into(), before: Some(have.len().to_string()), after: Some((have.len() + add.len()).to_string()) };
        writes.push((rel, path, old, new, row));
    }
    let settings_path = place.path(place.settings_rel);
    let settings = customizer::read_text(&settings_path)?;
    let mut list = enabled(&settings)?;
    for (group, on) in wanted {
        let name = collection_name(group).to_string();
        list.retain(|n| n != &name);
        if *on {
            list.push(name);
        }
    }
    let before = enabled(&settings)?.join(",");
    let after = list.join(",");
    if before != after {
        let edit: BTreeMap<String, Option<String>> = [(KEY.to_string(), Some(after.clone()))].into_iter().collect();
        let new = customizer::edit_typed(&settings, &edit, &BTreeMap::new())?;
        let row = ChangeRow { system: "interface".into(), key: "collections shown".into(), before: Some(before), after: Some(after) };
        writes.push((place.settings_rel.to_string(), settings_path, settings, new, row));
    }
    if writes.is_empty() {
        return Err("Nothing to change: those groups are already set up with every game RetroDrive copied there.".into());
    }
    Ok(writes)
}

fn label(adapter: &str) -> &'static str {
    if adapter == "esde" { "ES-DE" } else { "RetroPie" }
}

pub fn preview(connection: &Connection, installation_id: &str, wanted: &BTreeMap<String, bool>, presence: PresenceFn) -> Result<Preview, String> {
    let place = place(connection, installation_id, presence)?;
    let mut changes: Vec<ChangeRow> = plan(connection, &place, wanted)?.into_iter().map(|w| w.4).collect();
    if let Some(note) = &place.note {
        changes.push(ChangeRow { system: "note".into(), key: note.clone(), before: None, after: None });
    }
    Ok(Preview { changes, file_sha: fingerprint(&place)? })
}

/// Write the collection files and the setting: only if nothing changed since the
/// review and ES-DE isn't running; journaled, undoable; a failure undoes earlier writes.
pub fn apply(connection: &Connection, installation_id: &str, wanted: &BTreeMap<String, bool>, expected: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let place = place(connection, installation_id, presence)?;
    if place.adapter == "esde" && running() {
        return Err("ES-DE is running. Close it first: it saves its settings when it closes, which would undo this. Nothing was changed.".into());
    }
    if fingerprint(&place)? != expected {
        return Err("The collections or settings changed since you reviewed this, so nothing was changed. Review it again.".into());
    }
    let mut ids = Vec::new();
    for (rel, path, old, new, row) in plan(connection, &place, wanted)? {
        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
        }
        match customizer::write_file(connection, &place.install, &rel, &path, &old, &new, vec![row]) {
            Ok((_, id)) => ids.push(id),
            Err(e) => {
                let undone = ids.iter().rev().all(|id| customizer::undo(connection, *id, presence, &customizer::never_running).is_ok());
                return Err(if undone { format!("{e} The other changes were undone.") } else { format!("{e} Some changes were saved; undo them under \"Show every settings change\".") });
            }
        }
    }
    Ok(if place.adapter == "esde" {
        "Saved. ES-DE shows the groups under Collections the next time it starts. Undo is under \"Show every settings change\".".into()
    } else {
        "Saved. Restart EmulationStation on the Pi (Start → Quit → Restart EmulationStation) to see the groups, before changing its settings, or it may save over this. Undo is under \"Show every settings change\".".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, scanner};
    use crate::volumes::VolumeProbe;

    #[test]
    fn es_des_flat_settings_file_gets_the_setting_added_at_the_end() {
        // ES-DE writes es_settings.xml as a flat list with no <config> wrapper.
        let flat = "<?xml version=\"1.0\"?>
<bool name=\"ShowHelpPrompts\" value=\"true\" />
<string name=\"ThemeSet\" value=\"linear\" />
";
        let edit: BTreeMap<String, Option<String>> = [(KEY.to_string(), Some("Light Gun Games".to_string()))].into_iter().collect();
        let out = customizer::edit_typed(flat, &edit, &BTreeMap::new()).unwrap();
        assert_eq!(out, format!("{flat}<string name=\"CollectionSystemsCustom\" value=\"Light Gun Games\" />
"));
        assert!(customizer::edit_typed("<config>
<string name=\"A\" value=\"1\" />
", &edit, &BTreeMap::new()).is_err(), "a cut-off <config> file is still refused");
    }

    #[test]
    fn the_pis_rom_folder_is_read_from_its_own_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(pi_roms(dir.path()), (PI_DEFAULT_ROMS.to_string(), false));
        fs::write(dir.path().join("es_log.txt"), "lvl2: Couldn't find game referenced at '/home/arcade/RetroPie/roms/nes/X.nes' for system config").unwrap();
        assert_eq!(pi_roms(dir.path()), ("/home/arcade/RetroPie/roms".to_string(), true));
        fs::write(dir.path().join("es_systems.cfg"), "<system><path>/home/retro/RetroPie/roms/snes</path></system>").unwrap();
        assert_eq!(pi_roms(dir.path()).0, "/home/retro/RetroPie/roms", "es_systems.cfg first");
    }

    #[test]
    fn es_de_groups_list_the_copied_games_and_keep_existing_lines() {
        let dir = tempfile::tempdir().unwrap();
        // Library with a light gun game, a vertical game and an ordinary one.
        let lib = dir.path().join("lib");
        for (folder, name) in [("nes", "Duck Hunt (World).nes"), ("mame", "1941.zip"), ("nes", "Tetris (USA).nes")] {
            fs::create_dir_all(lib.join(folder)).unwrap();
            fs::write(lib.join(folder).join(name), vec![0u8; 64]).unwrap();
        }
        // A portable ES-DE: ES-DE.exe, ROMs, ES-DE data folder with settings.
        let es = dir.path().join("ES-DE");
        fs::create_dir_all(es.join("ES-DE/settings")).unwrap();
        fs::write(es.join("ES-DE.exe"), b"x").unwrap();
        fs::write(es.join("ES-DE/settings/es_settings.xml"), "<?xml version=\"1.0\"?>\n<string name=\"CollectionSystemsCustom\" value=\"My Faves\" />\n<string name=\"ThemeSet\" value=\"linear\" />\n").unwrap();
        let roms = es.join("ROMs");
        for (system, name) in [("nes", "Duck Hunt (World).nes"), ("fbneo", "1941.zip"), ("nes", "Tetris (USA).nes")] {
            fs::create_dir_all(roms.join(system)).unwrap();
            fs::write(roms.join(system).join(name), vec![0u8; 64]).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &lib, &|_: &str| VolumeProbe::unknown()).unwrap();
        let id = installations::add(&c, &roms, "esde", &|_: &str| VolumeProbe::unknown()).unwrap();
        // Record the copies as RetroDrive's copier would.
        c.execute("INSERT INTO deployments (id, installation_id, adapter_id, adapter_contract, destination_root, plan_fingerprint, status) VALUES ('d1', ?1, 'esde', 3, ?2, 'f', 'completed')", params![id, roms.display().to_string()]).unwrap();
        for (lib_name, system) in [("Duck Hunt (World).nes", "nes"), ("1941.zip", "fbneo"), ("Tetris (USA).nes", "nes")] {
            let file_id: String = c.query_row("SELECT id FROM game_files WHERE filename = ?1", [lib_name], |r| r.get(0)).unwrap();
            let dest = roms.join(system).join(lib_name).display().to_string();
            c.execute("INSERT INTO deployment_items (deployment_id, game_index, game_title, game_file_id, source_path, destination_path, size_bytes, planned_action, state) VALUES ('d1', 0, 't', ?1, 's', ?2, 64, 'copy', 'verified')", params![file_id, dest]).unwrap();
        }
        let connected = |_: &str, _: Option<&str>| Presence::Connected;
        let s = state(&c, &id, &connected).unwrap();
        assert_eq!(s.groups.iter().map(|g| (g.on, g.copied)).collect::<Vec<_>>(), vec![(false, Some(1)), (false, Some(1))]);
        // An existing collection of the user's with the same name keeps its lines.
        fs::create_dir_all(es.join("ES-DE/collections")).unwrap();
        fs::write(es.join("ES-DE/collections/custom-Light Gun Games.cfg"), "%ROMPATH%/nes/Mine.nes\r\n").unwrap();
        let both: BTreeMap<String, bool> = [("lightgun".to_string(), true), ("vertical".to_string(), true)].into_iter().collect();
        let p = preview(&c, &id, &both, &connected).unwrap();
        assert_eq!(p.changes.len(), 3, "{:?}", p.changes);
        assert!(apply(&c, &id, &both, &p.file_sha, &connected, &|| true).unwrap_err().contains("running"));
        apply(&c, &id, &both, &p.file_sha, &connected, &customizer::never_running).unwrap();
        assert_eq!(fs::read_to_string(es.join("ES-DE/collections/custom-Light Gun Games.cfg")).unwrap(), "%ROMPATH%/nes/Mine.nes\r\n%ROMPATH%/nes/Duck Hunt (World).nes\n");
        assert_eq!(fs::read_to_string(es.join("ES-DE/collections/custom-Vertical Games.cfg")).unwrap(), "%ROMPATH%/fbneo/1941.zip\n");
        let settings = fs::read_to_string(es.join("ES-DE/settings/es_settings.xml")).unwrap();
        assert!(settings.contains("value=\"My Faves,Light Gun Games,Vertical Games\"") && settings.contains("value=\"linear\""), "{settings}");
        assert!(preview(&c, &id, &both, &connected).unwrap_err().contains("Nothing to change"));
        // Turning a group off only hides it; its file stays.
        let off: BTreeMap<String, bool> = [("vertical".to_string(), false)].into_iter().collect();
        let p = preview(&c, &id, &off, &connected).unwrap();
        apply(&c, &id, &off, &p.file_sha, &connected, &customizer::never_running).unwrap();
        assert!(fs::read_to_string(es.join("ES-DE/settings/es_settings.xml")).unwrap().contains("value=\"My Faves,Light Gun Games\""));
        assert!(es.join("ES-DE/collections/custom-Vertical Games.cfg").is_file());
    }
}

