//! Game groups on the system (owner 2026-11-07: "add to the different emulators a
//! 'Light Gun' group and put all light gun games in that group … as well as Vertical
//! Games"; ADR-070; docs/18 §42).
//!
//! RetroBat and Batocera already have automatic collections "lightgun games"
//! (`lightgun`) and "vertical arcade" (`vertical`), filled by EmulationStation from
//! the same lists RetroDrive uses (`gamesdb.xml`, `arcaderoms.xml`). They're switched on
//! by naming them in the `CollectionSystemsAuto` setting of es_settings.cfg, a comma
//! list (`CollectionSystemManager.cpp` in batocera-emulationstation, which RetroBat's
//! EmulationStation shares). RetroDrive adds or removes only those two names; the
//! other collections the user turned on stay as they are.

use crate::customizer::{self, ChangeRow, PresenceFn, Preview, RunningFn};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const KEY: &str = "CollectionSystemsAuto";
/// (EmulationStation's collection name, what RetroDrive calls it).
pub const GROUPS: &[(&str, &str)] = &[("lightgun", "Light gun games"), ("vertical", "Vertical arcade games")];

#[derive(Debug, Serialize)]
pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    pub on: bool,
    /// Matching games in the user's library.
    pub in_library: usize,
    /// Matching games RetroDrive copied to this system (RetroPie, ES-DE).
    pub copied: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct GroupsState {
    pub groups: Vec<Group>,
    pub file_sha: String,
    pub network: bool,
    /// Something to tell the user (e.g. an assumed path on a RetroPie).
    pub note: Option<String>,
}

fn place(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<(customizer::Install, &'static str, PathBuf), String> {
    let install = customizer::install_for(connection, installation_id, presence, &["retrobat", "batocera"], "Game groups are for RetroBat and Batocera so far.")?;
    let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [installation_id], |r| r.get(0)).map_err(|e| e.to_string())?;
    let relative = if adapter == "batocera" { crate::batocera_settings::ES_SETTINGS } else { customizer::SETTINGS };
    let path = relative.split('/').fold(install.root.clone(), |p, part| p.join(part));
    if !path.is_file() {
        return Err("The system's interface settings (es_settings.cfg) are missing. Start it once so it creates them.".into());
    }
    Ok((install, relative, path))
}

fn names(value: Option<&String>) -> Vec<String> {
    value.map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// RetroPie and ES-DE get custom collection files instead (ADR-072).
fn uses_files(connection: &Connection, installation_id: &str) -> bool {
    let adapter: String = connection.query_row("SELECT adapter_id FROM installations WHERE id = ?1", [installation_id], |r| r.get(0)).unwrap_or_default();
    adapter == "retropie" || adapter == "esde"
}

/// Which groups are on, with how many library games each would show (reads only).
pub fn state(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<GroupsState, String> {
    if uses_files(connection, installation_id) {
        return crate::custom_collections::state(connection, installation_id, presence);
    }
    let (install, _, path) = place(connection, installation_id, presence)?;
    let text = customizer::read_text(&path)?;
    let on = names(customizer::values(&customizer::entries(&text)?)?.get(KEY));
    let counts = [crate::lightguns::game_ids(connection)?.len(), crate::vertical::game_ids(connection)?.len()];
    Ok(GroupsState {
        groups: GROUPS.iter().zip(counts).map(|((id, label), n)| Group { id, label, on: on.iter().any(|x| x == id), in_library: n, copied: None }).collect(),
        file_sha: customizer::hex_sha(text.as_bytes()),
        network: install.root_text.starts_with(r"\\"),
        note: None,
    })
}

/// The new `CollectionSystemsAuto` value: the user's other collections untouched,
/// RetroDrive's two added (at the end) or removed as asked.
fn new_value(current: Option<&String>, wanted: &BTreeMap<String, bool>) -> Result<String, String> {
    let mut list = names(current);
    for (id, on) in wanted {
        if !GROUPS.iter().any(|(g, _)| g == id) {
            return Err(format!("RetroDrive doesn't manage a group called {id}."));
        }
        list.retain(|x| x != id);
        if *on {
            list.push(id.clone());
        }
    }
    Ok(list.join(","))
}

/// The es_settings.cfg edit and the rows shown in the review.
type Change = (BTreeMap<String, Option<String>>, Vec<ChangeRow>);

fn change(text: &str, wanted: &BTreeMap<String, bool>) -> Result<Change, String> {
    let current = customizer::values(&customizer::entries(text)?)?;
    let before = current.get(KEY).cloned();
    let after = new_value(before.as_ref(), wanted)?;
    if before.as_deref().unwrap_or("") == after {
        return Err("Nothing to change: those groups are already set that way.".into());
    }
    let label = |v: &str| {
        let shown: Vec<&str> = names(Some(&v.to_string())).iter().filter_map(|n| GROUPS.iter().find(|(g, _)| g == n).map(|(_, l)| *l)).collect();
        if shown.is_empty() { "none of RetroDrive's groups".to_string() } else { shown.join(", ") }
    };
    let row = ChangeRow { system: "interface".into(), key: "game groups".into(), before: Some(label(before.as_deref().unwrap_or(""))), after: Some(label(&after)) };
    Ok(([(KEY.to_string(), Some(after))].into_iter().collect(), vec![row]))
}

/// What turning groups on or off would change (reads only).
pub fn preview(connection: &Connection, installation_id: &str, wanted: &BTreeMap<String, bool>, presence: PresenceFn) -> Result<Preview, String> {
    if uses_files(connection, installation_id) {
        return crate::custom_collections::preview(connection, installation_id, wanted, presence);
    }
    let (_, _, path) = place(connection, installation_id, presence)?;
    let text = customizer::read_text(&path)?;
    let (edit, rows) = change(&text, wanted)?;
    customizer::edit_typed(&text, &edit, &BTreeMap::new())?;
    Ok(Preview { changes: rows, file_sha: customizer::hex_sha(text.as_bytes()) })
}

/// Turn the groups on or off: only if es_settings.cfg is unchanged since the review
/// and RetroBat isn't running; journaled, backed up and undoable.
pub fn apply(connection: &Connection, installation_id: &str, wanted: &BTreeMap<String, bool>, expected_sha: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    if uses_files(connection, installation_id) {
        return crate::custom_collections::apply(connection, installation_id, wanted, expected_sha, presence, running);
    }
    let (install, relative, path) = place(connection, installation_id, presence)?;
    if let Some(reason) = customizer::running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let text = customizer::read_text(&path)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("The interface settings changed since you reviewed this, so nothing was changed. Review it again.".into());
    }
    let (edit, rows) = change(&text, wanted)?;
    let new_text = customizer::edit_typed(&text, &edit, &BTreeMap::new())?;
    customizer::write_file(connection, &install, relative, &path, &text, &new_text, rows)?;
    Ok(if relative == customizer::SETTINGS {
        "Saved. RetroBat shows the groups the next time it starts. Undo is under \"Show every settings change\".".into()
    } else if install.root_text.starts_with(r"\\") {
        "Saved. Restart Batocera now (Start → Quit → Restart system) to see the groups, before changing other settings in its menu, or Batocera may save over this.".into()
    } else {
        "Saved. Batocera shows the groups the next time it starts. Undo is under \"Show every settings change\".".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::{db, installations};
    use std::fs;

    const ES: &str = "<?xml version=\"1.0\"?>\n<config>\n\t<string name=\"CollectionSystemsAuto\" value=\"favorites,recent\" />\n\t<string name=\"ThemeSet\" value=\"carbon\" />\n</config>\n";

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn groups_are_added_and_removed_without_touching_the_users_collections() {
        let w = |pairs: &[(&str, bool)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
        let cur = "favorites,recent".to_string();
        assert_eq!(new_value(Some(&cur), &w(&[("lightgun", true), ("vertical", true)])).unwrap(), "favorites,recent,lightgun,vertical");
        assert_eq!(new_value(Some(&"favorites, lightgun ,recent".to_string()), &w(&[("lightgun", false)])).unwrap(), "favorites,recent");
        assert_eq!(new_value(None, &w(&[("vertical", true)])).unwrap(), "vertical");
        assert!(new_value(Some(&cur), &w(&[("all", true)])).is_err(), "only RetroDrive's two groups");

        let dir = tempfile::tempdir().unwrap();
        let rb = dir.path().join("RetroBat");
        fs::create_dir_all(rb.join("emulationstation/.emulationstation")).unwrap();
        fs::create_dir_all(rb.join("roms/snes")).unwrap();
        fs::write(rb.join("retrobat.exe"), b"x").unwrap();
        let settings = rb.join("emulationstation/.emulationstation/es_settings.cfg");
        fs::write(&settings, ES).unwrap();
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &rb, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();

        let s = state(&c, &id, &connected).unwrap();
        assert!(s.groups.iter().all(|g| !g.on));
        let on = w(&[("lightgun", true), ("vertical", true)]);
        let p = preview(&c, &id, &on, &connected).unwrap();
        assert_eq!(p.changes[0].after.as_deref(), Some("Light gun games, Vertical arcade games"));
        assert!(apply(&c, &id, &on, &p.file_sha, &connected, &|| true).unwrap_err().contains("running"), "not while RetroBat runs");
        apply(&c, &id, &on, &p.file_sha, &connected, &customizer::never_running).unwrap();
        let text = fs::read_to_string(&settings).unwrap();
        assert!(text.contains("value=\"favorites,recent,lightgun,vertical\"") && text.contains("name=\"ThemeSet\" value=\"carbon\""), "{text}");
        assert!(state(&c, &id, &connected).unwrap().groups.iter().all(|g| g.on));
        assert!(preview(&c, &id, &on, &connected).unwrap_err().contains("Nothing to change"));
        let change: i64 = c.query_row("SELECT id FROM settings_changes ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        customizer::undo(&c, change, &connected, &customizer::never_running).unwrap();
        assert_eq!(fs::read_to_string(&settings).unwrap(), ES, "undo puts the file back");
    }
}

