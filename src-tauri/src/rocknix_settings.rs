//! ROCKNIX settings (ADR-082; docs/18 §50).
//!
//! Facts checked 2026-11-08 in ROCKNIX's sources (GitHub `ROCKNIX/emulationstation`,
//! main; `ROCKNIX/distribution`, next):
//! - Settings live in `/storage/.config/system/configs/system.cfg` (`SystemConf.cpp`),
//!   lines `key=value`; `#` comments. From Windows that is the `config` share
//!   (`/storage/.config`, smb.conf), so `\\<device>\config\system\configs\system.cfg`.
//!   A ROCKNIX SD card in a PC can't reach it (/storage is ext4), so these settings
//!   are offered for network installations only.
//! - ROCKNIX's menu saves like Batocera's (`SystemConf::saveSystemConf`): the line of
//!   each changed key (active, or disabled with `#`/`;`) is replaced; a value that is
//!   empty or `default` removes the line; a key without a line is appended. RetroDrive
//!   edits the same way (`batocera_settings::edit_conf`), so menu changes aren't lost.
//! - Keys (`GuiMenu.cpp`): `global.`/`<system>.` `ratio` (`LibretroRatio.cpp`),
//!   `smooth`, `rewind`, `integerscale` (1/0), `autosave` (1/0/2/3), `shaderset`
//!   (`none` or a set from the device's `/usr/bin/getshaders`; RetroDrive offers
//!   only `none` and keeps any other current value). No line = ROCKNIX's default for
//!   all systems, or the all-systems value for one system.
//! - Backups and Undo stay on the `config` share (`\\<device>\config\.retrodrive\backup`),
//!   since a file can't be moved between shares.

use crate::batocera_settings::{conf_values, edit_conf};
use crate::customizer::{self, ChangeRow, Choice, Install, PresenceFn, Preview};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONF: &str = "system/configs/system.cfg";

#[derive(Debug, Clone, Serialize)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize)]
pub struct RocknixOptions {
    pub settings: Vec<Setting>,
    pub systems: Vec<String>,
    /// Current active values, by full key (`global.ratio`, `snes.ratio`, …).
    pub values: BTreeMap<String, String>,
    pub conf_sha: String,
    /// Where the file is, for the card's note.
    pub conf_path: String,
}

/// A change: full key and the new value (`None` = no line: ROCKNIX's default, or for
/// one system the all-systems value).
#[derive(Debug, Clone, Deserialize)]
pub struct ConfChange {
    pub key: String,
    pub value: Option<String>,
}

fn choices(list: &[(&str, &str)]) -> Vec<Choice> {
    list.iter()
        .map(|(label, value)| Choice {
            label: label.to_string(),
            value: value.to_string(),
        })
        .collect()
}

const ON_OFF: &[(&str, &str)] = &[("On", "1"), ("Off", "0")];

pub fn settings() -> Vec<Setting> {
    vec![
        Setting {
            key: "ratio",
            label: "Game aspect ratio",
            help: "Force games to this shape. Not every system supports every ratio.",
            choices: choices(&[
                ("Core provided", "core"),
                ("4/3", "4/3"),
                ("16/9", "16/9"),
                ("16/10", "16/10"),
                ("16/15", "16/15"),
                ("21/9", "21/9"),
                ("1/1", "1/1"),
                ("2/1", "2/1"),
                ("3/2", "3/2"),
                ("3/4", "3/4"),
                ("3/5", "3/5"),
                ("9/16", "9/16"),
                ("5/4", "5/4"),
                ("6/5", "6/5"),
                ("7/9", "7/9"),
                ("8/3", "8/3"),
                ("8/7", "8/7"),
                ("19/12", "19/12"),
                ("19/14", "19/14"),
                ("30/17", "30/17"),
                ("32/9", "32/9"),
                ("Square pixel", "squarepixel"),
                ("Full screen", "full"),
                ("RetroArch config", "config"),
                ("Custom", "custom"),
            ]),
        },
        Setting { key: "shaderset", label: "Shader set", help: "Turn shaders off here; pick a shader set on the device (the sets depend on the device).", choices: choices(&[("None", "none")]) },
        Setting { key: "smooth", label: "Smooth games", help: "Bilinear filtering: softer pixels.", choices: choices(ON_OFF) },
        Setting { key: "rewind", label: "Rewind", help: "Go back in time while playing; may slow some systems down.", choices: choices(ON_OFF) },
        Setting {
            key: "autosave",
            label: "Auto save/load",
            help: "Save when you quit a game and pick up there next time, or choose a save when a game starts.",
            choices: choices(&[("On", "1"), ("Off", "0"), ("Show save states", "2"), ("Show save states if any", "3")]),
        },
        Setting { key: "integerscale", label: "Integer scaling", help: "Pixel perfect: only whole-number enlargements.", choices: choices(ON_OFF) },
    ]
}

/// `\\<device>\config` for an installation on `\\<device>\<share>…`.
pub(crate) fn network_config(root: &Path) -> Result<PathBuf, String> {
    let text = root.display().to_string().replace('/', "\\");
    let host = text
        .strip_prefix(r"\\")
        .and_then(|rest| rest.split('\\').next())
        .filter(|h| !h.is_empty() && *h != "?" && *h != ".");
    match host {
        Some(host) => Ok(PathBuf::from(format!(r"\\{host}\config"))),
        None => Err("ROCKNIX keeps its settings in a part of the card Windows can't read, so they can only be changed over the network. Add the ROCKNIX device as a network share (\\\\ROCKNIX\\games-roms) to change them here.".into()),
    }
}

type Locate<'a> = &'a dyn Fn(&Path) -> Result<PathBuf, String>;

/// The installation, with its root moved to the `config` share so the backup and
/// Undo stay on the same share as the file.
fn install(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
    locate: Locate,
) -> Result<(Install, PathBuf), String> {
    let games = customizer::install_for(
        connection,
        installation_id,
        presence,
        &["rocknix"],
        "These settings are for ROCKNIX installations.",
    )?;
    let config = locate(&games.root)?;
    if !config.is_dir() {
        return Err(format!("{} can't be reached. Check that the ROCKNIX device is on and its network sharing is switched on.", config.display()));
    }
    let root_text = config.display().to_string();
    let stable = Some(crate::volumes::share_stable_id(&root_text, None));
    Ok((
        Install {
            id: games.id,
            root: config.clone(),
            root_text,
            stable,
        },
        games.root,
    ))
}

fn conf_path(config: &Path) -> PathBuf {
    config.join("system").join("configs").join("system.cfg")
}

fn read(config: &Path) -> Result<String, String> {
    let path = conf_path(config);
    if !path.is_file() {
        return Err(format!(
            "{} is missing. Start ROCKNIX once so it creates it.",
            path.display()
        ));
    }
    customizer::read_text(&path)
}

/// ROCKNIX system folders with at least one game.
fn systems(games_root: &Path) -> Vec<String> {
    let folder = crate::adapters::rocknix::games_folder(games_root);
    let skip = |name: &str| {
        name.eq_ignore_ascii_case("gamelist.xml")
            || name.eq_ignore_ascii_case("images")
            || name.eq_ignore_ascii_case("videos")
            || name.starts_with('.')
    };
    crate::batocera_settings::folders(&folder)
        .into_iter()
        .filter(|s| crate::adapters::rocknix::SYSTEMS.contains(&s.as_str()))
        .filter(|s| {
            fs::read_dir(folder.join(s))
                .map(|r| {
                    r.filter_map(Result::ok)
                        .any(|e| !skip(&e.file_name().to_string_lossy()))
                })
                .unwrap_or(false)
        })
        .collect()
}

/// The games in one of ROCKNIX's system folders, for settings of one game (ADR-083).
pub fn games(connection: &Connection, installation_id: &str, system: &str, presence: PresenceFn) -> Result<Vec<String>, String> {
    let games = customizer::install_for(connection, installation_id, presence, &["rocknix"], "These settings are for ROCKNIX installations.")?;
    if !systems(&games.root).iter().any(|s| s == system) {
        return Err(format!("{system} isn't a system with games on this ROCKNIX."));
    }
    Ok(crate::batocera_settings::game_names(&crate::adapters::rocknix::games_folder(&games.root).join(system)))
}

pub fn options(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
) -> Result<RocknixOptions, String> {
    options_at(connection, installation_id, presence, &network_config)
}

fn options_at(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
    locate: Locate,
) -> Result<RocknixOptions, String> {
    let (install, games) = install(connection, installation_id, presence, locate)?;
    let text = read(&install.root)?;
    Ok(RocknixOptions {
        settings: settings(),
        systems: systems(&games),
        conf_sha: customizer::hex_sha(text.as_bytes()),
        values: conf_values(&text),
        conf_path: conf_path(&install.root).display().to_string(),
    })
}

/// Checked changes: known settings, known systems, ROCKNIX's values (a shader set
/// already chosen on the device may stay).
fn validate(
    games: &Path,
    current: &BTreeMap<String, String>,
    changes: &[ConfChange],
) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("Nothing to change.".into());
    }
    let offered = settings();
    let systems = systems(games);
    let mut wanted = BTreeMap::new();
    for c in changes {
        let (scope, game, key) = crate::batocera_settings::split_key(&c.key).ok_or_else(|| format!("Unknown setting {}.", c.key))?;
        let setting = offered
            .iter()
            .find(|s| s.key == key)
            .ok_or_else(|| format!("RetroDrive doesn't change {key} on ROCKNIX."))?;
        if (scope != "global" || game.is_some()) && !systems.iter().any(|s| s == scope) {
            return Err(format!("{} can't be set for {scope}.", setting.label));
        }
        if let Some(g) = game {
            if !crate::batocera_settings::game_names(&crate::adapters::rocknix::games_folder(games).join(scope)).iter().any(|n| n == g) {
                return Err(format!("{g} isn't in ROCKNIX's {scope} folder."));
            }
        }
        if let Some(v) = &c.value {
            let kept = current.get(&c.key) == Some(v);
            if !kept && !setting.choices.iter().any(|ch| &ch.value == v) {
                return Err(format!(
                    "{v} isn't one of ROCKNIX's choices for {}.",
                    setting.label
                ));
            }
        }
        wanted.insert(c.key.clone(), c.value.clone());
    }
    Ok(wanted)
}

fn rows(
    wanted: &BTreeMap<String, Option<String>>,
    current: &BTreeMap<String, String>,
) -> Vec<ChangeRow> {
    wanted
        .iter()
        .map(|(key, value)| {
            let (scope, game, name) = crate::batocera_settings::split_key(key).unwrap_or(("global", None, key));
            ChangeRow {
                system: crate::batocera_settings::scope_label(scope, game),
                key: name.into(),
                before: current.get(key).cloned(),
                after: value.clone(),
            }
        })
        .collect()
}

fn conf_edit(text: &str, wanted: &BTreeMap<String, Option<String>>) -> Result<String, String> {
    edit_conf(text, wanted, &['#', ';'], "system.cfg")
}

pub fn preview(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    presence: PresenceFn,
) -> Result<Preview, String> {
    preview_at(
        connection,
        installation_id,
        changes,
        presence,
        &network_config,
    )
}

fn preview_at(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    presence: PresenceFn,
    locate: Locate,
) -> Result<Preview, String> {
    let (install, games) = install(connection, installation_id, presence, locate)?;
    let text = read(&install.root)?;
    let current = conf_values(&text);
    let wanted = validate(&games, &current, changes)?;
    conf_edit(&text, &wanted)?;
    Ok(Preview {
        changes: rows(&wanted, &current),
        file_sha: customizer::hex_sha(text.as_bytes()),
    })
}

/// Save reviewed changes: only if system.cfg is unchanged since the review;
/// journaled, backed up and undoable (`customizer::write_file`).
pub fn apply(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    expected_sha: &str,
    presence: PresenceFn,
) -> Result<String, String> {
    apply_at(
        connection,
        installation_id,
        changes,
        expected_sha,
        presence,
        &network_config,
    )
}

fn apply_at(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    expected_sha: &str,
    presence: PresenceFn,
    locate: Locate,
) -> Result<String, String> {
    let (install, games) = install(connection, installation_id, presence, locate)?;
    let text = read(&install.root)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("system.cfg changed since you reviewed it (perhaps from ROCKNIX's menu), so nothing was changed. Review the change again.".into());
    }
    let current = conf_values(&text);
    let wanted = validate(&games, &current, changes)?;
    let new_text = conf_edit(&text, &wanted)?;
    customizer::write_file(
        connection,
        &install,
        CONF,
        &conf_path(&install.root),
        &text,
        &new_text,
        rows(&wanted, &current),
    )?;
    Ok("Saved to ROCKNIX's system.cfg. ROCKNIX uses these settings the next time a game starts. Undo is under \"Show every settings change\".".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::{db, installations};

    const CFG: &str = "3do.ratio=4/3\n#global.smooth=1\nglobal.autosave=1\nglobal.ratio=core\nglobal.shaderset=crt-easymode\nsystem.hostname=RG35XX\n";

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn the_config_share_is_found_from_the_games_share() {
        assert_eq!(
            network_config(Path::new(r"\\ROCKNIX\games-roms")).unwrap(),
            PathBuf::from(r"\\ROCKNIX\config")
        );
        assert_eq!(
            network_config(Path::new(r"\\192.168.1.40\games-roms\")).unwrap(),
            PathBuf::from(r"\\192.168.1.40\config")
        );
        assert!(
            network_config(Path::new(r"E:\"))
                .unwrap_err()
                .contains("over the network"),
            "an SD card can't reach /storage/.config"
        );
        assert!(network_config(Path::new(r"\\?\E:\roms")).is_err());
    }

    #[test]
    fn settings_are_reviewed_saved_like_rocknixs_menu_and_undone() {
        let dir = tempfile::tempdir().unwrap();
        let games = dir.path().join("games-roms");
        let config = dir.path().join("config");
        for s in ["snes", "nes", "gba", "savestates", "bezels", "fbn"] {
            fs::create_dir_all(games.join(s)).unwrap();
        }
        fs::write(games.join("snes/Mario.sfc"), b"m").unwrap();
        fs::write(games.join("fbn/1942.zip"), b"m").unwrap();
        fs::create_dir_all(config.join("system/configs")).unwrap();
        fs::write(config.join("system/configs/system.cfg"), CFG).unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id =
            installations::add(&c, &games, "rocknix", &|_: &str| VolumeProbe::unknown()).unwrap();
        let cfg_dir = config.clone();
        let locate = move |_: &Path| -> Result<PathBuf, String> { Ok(cfg_dir.clone()) };

        let o = options_at(&c, &id, &connected, &locate).unwrap();
        assert_eq!(
            o.systems,
            vec!["fbn".to_string(), "snes".to_string()],
            "systems with games; savestates/bezels aren't systems"
        );
        assert_eq!(
            o.values.get("global.ratio").map(String::as_str),
            Some("core")
        );
        assert!(
            !o.values.contains_key("global.smooth"),
            "a # line isn't active"
        );

        let ch = |k: &str, v: Option<&str>| ConfChange {
            key: k.into(),
            value: v.map(str::to_string),
        };
        assert!(preview_at(
            &c,
            &id,
            &[ch("global.ratio", Some("99/1"))],
            &connected,
            &locate
        )
        .is_err());
        assert!(
            preview_at(
                &c,
                &id,
                &[ch("nes.ratio", Some("4/3"))],
                &connected,
                &locate
            )
            .is_err(),
            "an empty nes folder isn't offered"
        );
        assert!(preview_at(
            &c,
            &id,
            &[ch("global.hostname", Some("x"))],
            &connected,
            &locate
        )
        .is_err());
        assert!(
            preview_at(
                &c,
                &id,
                &[ch("snes.shaderset", Some("crt-easymode"))],
                &connected,
                &locate
            )
            .is_err(),
            "only a shader set already chosen there may stay"
        );
        assert!(preview_at(
            &c,
            &id,
            &[ch("global.shaderset", Some("crt-easymode"))],
            &connected,
            &locate
        )
        .is_ok());

        let changes = [
            ch("global.smooth", Some("0")),
            ch("snes.ratio", Some("8/7")),
            ch("global.autosave", None),
            ch("global.shaderset", Some("none")),
        ];
        let p = preview_at(&c, &id, &changes, &connected, &locate).unwrap();
        assert_eq!(p.changes.len(), 4);
        assert!(apply_at(&c, &id, &changes, "stale", &connected, &locate)
            .unwrap_err()
            .contains("changed since"));
        apply_at(&c, &id, &changes, &p.file_sha, &connected, &locate).unwrap();
        let text = fs::read_to_string(config.join("system/configs/system.cfg")).unwrap();
        assert_eq!(
            text,
            "3do.ratio=4/3\nglobal.smooth=0\nglobal.ratio=core\nglobal.shaderset=none\nsystem.hostname=RG35XX\nsnes.ratio=8/7\n",
            "a disabled line is reactivated in place, \"default\" removes the line, new keys are appended"
        );
        let backups = config.join(".retrodrive").join("backup");
        assert!(backups.is_dir(), "the backup stays on the config share");
        assert!(!games.join(".retrodrive").join("backup").exists());

        let change: i64 = c
            .query_row(
                "SELECT id FROM settings_changes ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        customizer::undo(&c, change, &connected, &customizer::never_running).unwrap();
        assert_eq!(
            fs::read_to_string(config.join("system/configs/system.cfg")).unwrap(),
            CFG,
            "undo puts the file back"
        );

        // One game (ADR-083): its own line, named the way EmulationStation names it.
        assert_eq!(super::games(&c, &id, "snes", &connected).unwrap(), ["Mario.sfc"]);
        let game = [ch("snes[\"Mario.sfc\"].ratio", Some("8/7"))];
        assert!(preview_at(&c, &id, &[ch("snes[\"Nope.sfc\"].ratio", Some("8/7"))], &connected, &locate).is_err());
        let p = preview_at(&c, &id, &game, &connected, &locate).unwrap();
        assert_eq!(p.changes[0].system, "snes: Mario.sfc");
        apply_at(&c, &id, &game, &p.file_sha, &connected, &locate).unwrap();
        assert!(fs::read_to_string(config.join("system/configs/system.cfg")).unwrap().ends_with("snes[\"Mario.sfc\"].ratio=8/7\n"));
    }

    #[test]
    fn other_systems_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        fs::create_dir_all(dir.path().join("roms/nes")).unwrap();
        fs::create_dir_all(dir.path().join("system")).unwrap();
        fs::write(dir.path().join("system/batocera.conf"), "").unwrap();
        let id = installations::add(&c, dir.path(), "batocera", &|_: &str| {
            VolumeProbe::unknown()
        })
        .unwrap();
        assert!(options(&c, &id, &connected)
            .unwrap_err()
            .contains("ROCKNIX"));
    }
}
