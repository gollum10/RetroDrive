//! Recalbox settings and theme (RDS-025 later step; ADR-068; docs/18 §41).
//!
//! Facts checked 2026-11-07 in Recalbox's EmulationStation (GitLab
//! `recalbox/recalbox-emulationstation`, master):
//! - Settings live in the share's `system/recalbox.conf`, lines `key=value`; `#` starts
//!   a comment and `;key=value` is a disabled key (`IniFile::IsValidKeyValue`).
//! - Recalbox's menu saves by re-reading the file and replacing the first `key=` line
//!   (active or `;`-disabled) of each key it changed, appending keys without a line
//!   (`IniFile::Save`). RetroDrive edits the same way (`batocera_settings::edit_conf`
//!   with `;` as the disabled mark), so edits made over the network while Recalbox
//!   runs aren't lost.
//! - Keys and defaults (`RecalboxConf.h`): `global.ratio` (auto), `global.smooth` (1),
//!   `global.rewind` (1), `global.autosave` (0), `global.integerscale` (0),
//!   `global.shaderset` (none), `global.showfps` (0), `global.quitpresstwice` (0);
//!   per system `<system>.ratio`, `.smooth`, `.rewind`, `.autosave`, `.shaderset`
//!   (a system without its own line uses the global one).
//! - Ratio values: `LibretroRatio.cpp`; shader sets offered by the menu: none,
//!   scanlines, retro (`GuiMenuGameSettings.cpp`).
//! - Theme: `emulationstation.theme.folder` (default `recalbox-next`, built in); user
//!   themes are folders in the share's `themes`.
//! - One game (ADR-085): `<rom file>.recalbox.conf` next to the game is loaded over
//!   recalbox.conf when it starts (configgen `buildOverrideChain` /
//!   `loadRecalboxSettings`), so any recalbox.conf key works there. RetroDrive writes
//!   `<system>.<key>` lines (a system's key beats `global.` in recalbox.conf), creating
//!   the file on the first change; Undo moves a created file into the backup folder.

use crate::batocera_settings::{edit_conf, folders, systems};
use crate::customizer::{self, ChangeRow, Choice, PresenceFn, Preview};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const CONF: &str = "system/recalbox.conf";
pub const THEME_KEY: &str = "emulationstation.theme.folder";
pub const BUILTIN_THEME: &str = "recalbox-next";

#[derive(Debug, Clone, Serialize)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// Can be set per system (otherwise only for all systems).
    pub per_system: bool,
    /// Recalbox's value when there's no line.
    pub default: &'static str,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize)]
pub struct RecalboxOptions {
    pub settings: Vec<Setting>,
    pub systems: Vec<String>,
    /// Current active values, by full key (`global.ratio`, `snes.ratio`, …).
    pub values: BTreeMap<String, String>,
    pub conf_sha: String,
    pub themes: Vec<String>,
    pub theme: String,
}

/// A change: full key and the new value (`None` = a system uses the all-systems value).
#[derive(Debug, Clone, Deserialize)]
pub struct ConfChange {
    pub key: String,
    pub value: Option<String>,
}

fn choices(list: &[(&str, &str)]) -> Vec<Choice> {
    list.iter().map(|(label, value)| Choice { label: label.to_string(), value: value.to_string() }).collect()
}

const ON_OFF: &[(&str, &str)] = &[("On", "1"), ("Off", "0")];

pub fn settings() -> Vec<Setting> {
    vec![
        Setting {
            key: "ratio",
            label: "Game aspect ratio",
            help: "Force games to this shape. Not every system supports every ratio.",
            per_system: true,
            default: "auto",
            choices: choices(&[
                ("Auto", "auto"),
                ("4/3", "4/3"),
                ("16/9", "16/9"),
                ("16/10", "16/10"),
                ("16/15", "16/15"),
                ("21/9", "21/9"),
                ("1/1", "1/1"),
                ("2/1", "2/1"),
                ("3/2", "3/2"),
                ("3/4", "3/4"),
                ("4/1", "4/1"),
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
                ("RetroArch config", "config"),
                ("RetroArch custom", "custom"),
                ("Core provided", "coreprovided"),
            ]),
        },
        Setting { key: "shaderset", label: "Shader set", help: "A look for the picture: scanlines, or a retro CRT look.", per_system: true, default: "none", choices: choices(&[("None", "none"), ("Scanlines", "scanlines"), ("Retro", "retro")]) },
        Setting { key: "smooth", label: "Smooth games", help: "Bilinear filtering: softer pixels.", per_system: true, default: "1", choices: choices(ON_OFF) },
        Setting { key: "rewind", label: "Rewind", help: "Go back in time while playing; may slow some systems down.", per_system: true, default: "1", choices: choices(ON_OFF) },
        Setting { key: "autosave", label: "Auto save/load", help: "Save when you quit a game and pick up there next time.", per_system: true, default: "0", choices: choices(ON_OFF) },
        Setting { key: "integerscale", label: "Integer scaling", help: "Pixel perfect: only whole-number enlargements.", per_system: false, default: "0", choices: choices(ON_OFF) },
        Setting { key: "showfps", label: "Show frame rate", help: "Show frames per second while playing.", per_system: false, default: "0", choices: choices(ON_OFF) },
        Setting { key: "quitpresstwice", label: "Press twice to quit", help: "Quitting a game needs the quit buttons pressed twice, so it isn't done by accident.", per_system: false, default: "0", choices: choices(ON_OFF) },
    ]
}

fn conf_edit(text: &str, wanted: &BTreeMap<String, Option<String>>) -> Result<String, String> {
    edit_conf(text, wanted, &[';'], "recalbox.conf")
}

fn install(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<customizer::Install, String> {
    customizer::install_for(connection, installation_id, presence, &["recalbox"], "These settings are for Recalbox installations.")
}

const NO_CONF: &str = "system\\recalbox.conf is missing. Start Recalbox once with this drive or share so it creates it.";

fn read(root: &Path) -> Result<String, String> {
    let path = root.join("system").join("recalbox.conf");
    if !path.is_file() {
        return Err(NO_CONF.into());
    }
    customizer::read_text(&path)
}

fn themes(root: &Path) -> Vec<String> {
    let mut list = vec![BUILTIN_THEME.to_string()];
    list.extend(folders(&root.join("themes")).into_iter().filter(|t| t != BUILTIN_THEME));
    list
}

/// What an override file next to a game is called.
pub const GAME_SUFFIX: &str = ".recalbox.conf";

/// The games in one of Recalbox's system folders, by file name (override files, game
/// lists and media folders left out).
fn game_files(root: &Path, system: &str) -> Vec<String> {
    const NOT_GAMES: &[&str] = &["gamelist.xml", "_info.txt", "images", "videos", "manuals", "media", "downloaded_images"];
    let lower_ends = |n: &str, s: &str| n.to_ascii_lowercase().ends_with(s);
    let mut list: Vec<String> = std::fs::read_dir(root.join("roms").join(system))
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.') && !NOT_GAMES.iter().any(|x| n.eq_ignore_ascii_case(x)))
                .filter(|n| !lower_ends(n, GAME_SUFFIX) && !lower_ends(n, ".retroarch.cfg") && !lower_ends(n, ".core.cfg"))
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|s| s.to_lowercase());
    list
}

/// The games of a system, for settings of one game (reads only).
pub fn games(connection: &Connection, installation_id: &str, system: &str, presence: PresenceFn) -> Result<Vec<String>, String> {
    let install = install(connection, installation_id, presence)?;
    if !systems(&install.root).iter().any(|s| s == system) {
        return Err(format!("{system} isn't a system with games on this Recalbox."));
    }
    Ok(game_files(&install.root, system))
}

/// A game's own settings (from its override file, if it has one).
#[derive(Debug, Serialize)]
pub struct GameOptions {
    pub values: BTreeMap<String, String>,
    pub exists: bool,
    pub file_sha: String,
}

/// The game's override file, checked: a game in that system's folder.
fn game_file(root: &Path, system: &str, game: &str) -> Result<(String, std::path::PathBuf), String> {
    if !systems(root).iter().any(|s| s == system) || !game_files(root, system).iter().any(|g| g == game) {
        return Err(format!("{game} isn't in Recalbox's {system} folder."));
    }
    let relative = format!("roms/{system}/{game}{GAME_SUFFIX}");
    let path = root.join("roms").join(system).join(format!("{game}{GAME_SUFFIX}"));
    Ok((relative, path))
}

fn read_game(path: &Path) -> Result<(String, bool), String> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok((String::new(), false));
    }
    if !path.is_file() {
        return Err(format!("{} isn't a file, so RetroDrive won't use it.", path.display()));
    }
    Ok((customizer::read_text(path)?, true))
}

pub fn game_options(connection: &Connection, installation_id: &str, system: &str, game: &str, presence: PresenceFn) -> Result<GameOptions, String> {
    let install = install(connection, installation_id, presence)?;
    let (_, path) = game_file(&install.root, system, game)?;
    let (text, exists) = read_game(&path)?;
    Ok(GameOptions { values: crate::batocera_settings::conf_values(&text), exists, file_sha: customizer::hex_sha(text.as_bytes()) })
}

/// Checked changes for one game: per-system settings of that system, Recalbox's values.
fn validate_game(system: &str, changes: &[ConfChange]) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("Nothing to change.".into());
    }
    let offered = settings();
    let mut wanted = BTreeMap::new();
    for c in changes {
        let (scope, key) = c.key.split_once('.').ok_or_else(|| format!("Unknown setting {}.", c.key))?;
        let setting = offered.iter().find(|s| s.key == key && s.per_system).ok_or_else(|| format!("{key} can't be set for one game here."))?;
        if scope != system {
            return Err(format!("{} for one {system} game must be a {system} setting.", setting.label));
        }
        if let Some(v) = &c.value {
            if !setting.choices.iter().any(|ch| &ch.value == v) {
                return Err(format!("{v} isn't one of Recalbox's choices for {}.", setting.label));
            }
        }
        wanted.insert(c.key.clone(), c.value.clone());
    }
    Ok(wanted)
}

fn game_rows(game: &str, wanted: &BTreeMap<String, Option<String>>, current: &BTreeMap<String, String>) -> Vec<ChangeRow> {
    wanted
        .iter()
        .map(|(key, value)| {
            let (scope, name) = key.split_once('.').unwrap_or(("", key));
            ChangeRow { system: format!("{scope}: {game}"), key: name.into(), before: current.get(key).cloned(), after: value.clone() }
        })
        .collect()
}

/// What changes to one game's settings would do (reads only).
pub fn preview_game(connection: &Connection, installation_id: &str, system: &str, game: &str, changes: &[ConfChange], presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let (_, path) = game_file(&install.root, system, game)?;
    let (text, _) = read_game(&path)?;
    let wanted = validate_game(system, changes)?;
    edit_conf(&text, &wanted, &[';'], "the game's settings file")?;
    Ok(Preview { changes: game_rows(game, &wanted, &crate::batocera_settings::conf_values(&text)), file_sha: customizer::hex_sha(text.as_bytes()) })
}

/// Save reviewed settings for one game in its override file (created on the first
/// change); journaled, backed up and undoable.
pub fn apply_game(connection: &Connection, installation_id: &str, system: &str, game: &str, changes: &[ConfChange], expected_sha: &str, presence: PresenceFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let (relative, path) = game_file(&install.root, system, game)?;
    let (text, exists) = read_game(&path)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("The game's settings file changed since you reviewed it, so nothing was changed. Review the change again.".into());
    }
    let wanted = validate_game(system, changes)?;
    let new_text = edit_conf(&text, &wanted, &[';'], "the game's settings file")?;
    if !exists && new_text.is_empty() {
        return Err("Nothing to change.".into());
    }
    customizer::write_file(connection, &install, &relative, &path, &text, &new_text, game_rows(game, &wanted, &crate::batocera_settings::conf_values(&text)))?;
    Ok(format!("Saved to {game}{GAME_SUFFIX} next to the game. Recalbox uses it the next time that game starts. Undo is under \"Show every settings change\"."))
}

/// What can be set, with the current values (reads only).
pub fn options(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<RecalboxOptions, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root)?;
    let values = crate::batocera_settings::conf_values(&text);
    Ok(RecalboxOptions {
        settings: settings(),
        systems: systems(&install.root),
        theme: values.get(THEME_KEY).cloned().filter(|t| !t.is_empty()).unwrap_or_else(|| BUILTIN_THEME.into()),
        themes: themes(&install.root),
        conf_sha: customizer::hex_sha(text.as_bytes()),
        values,
    })
}

/// Checked changes: known settings and scopes, Recalbox's values, installed themes.
fn validate(root: &Path, changes: &[ConfChange]) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("Nothing to change.".into());
    }
    let offered = settings();
    let systems = systems(root);
    let mut wanted = BTreeMap::new();
    for c in changes {
        if c.key == THEME_KEY {
            match &c.value {
                Some(t) if themes(root).contains(t) => {}
                Some(t) => return Err(format!("There's no theme called {t} on this Recalbox.")),
                None => return Err("Choose a theme.".into()),
            }
        } else {
            let (scope, key) = c.key.split_once('.').ok_or_else(|| format!("Unknown setting {}.", c.key))?;
            let setting = offered.iter().find(|s| s.key == key).ok_or_else(|| format!("RetroDrive doesn't change {key} on Recalbox."))?;
            let system_scope = scope != "global";
            if system_scope && (!setting.per_system || !systems.iter().any(|s| s == scope)) {
                return Err(format!("{} can't be set for {scope}.", setting.label));
            }
            match &c.value {
                Some(v) if !setting.choices.iter().any(|ch| &ch.value == v) => return Err(format!("{v} isn't one of Recalbox's choices for {}.", setting.label)),
                None if !system_scope => return Err(format!("Choose a value for {} for all systems.", setting.label)),
                _ => {}
            }
        }
        wanted.insert(c.key.clone(), c.value.clone());
    }
    Ok(wanted)
}

fn rows(wanted: &BTreeMap<String, Option<String>>, current: &BTreeMap<String, String>) -> Vec<ChangeRow> {
    wanted
        .iter()
        .map(|(key, value)| {
            if key == THEME_KEY {
                return ChangeRow { system: "interface".into(), key: "theme".into(), before: current.get(key).cloned(), after: value.clone() };
            }
            let (scope, name) = key.split_once('.').unwrap_or(("global", key));
            ChangeRow { system: if scope == "global" { "all systems".into() } else { scope.into() }, key: name.into(), before: current.get(key).cloned(), after: value.clone() }
        })
        .collect()
}

/// What the changes would do (reads only).
pub fn preview(connection: &Connection, installation_id: &str, changes: &[ConfChange], presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root)?;
    let wanted = validate(&install.root, changes)?;
    conf_edit(&text, &wanted)?;
    Ok(Preview { changes: rows(&wanted, &crate::batocera_settings::conf_values(&text)), file_sha: customizer::hex_sha(text.as_bytes()) })
}

/// Save reviewed changes: only if recalbox.conf is unchanged since the review;
/// journaled, backed up and undoable (`customizer::write_file`).
pub fn apply(connection: &Connection, installation_id: &str, changes: &[ConfChange], expected_sha: &str, presence: PresenceFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("recalbox.conf changed since you reviewed it (perhaps from Recalbox's menu), so nothing was changed. Review the change again.".into());
    }
    let wanted = validate(&install.root, changes)?;
    let new_text = conf_edit(&text, &wanted)?;
    let path = install.root.join("system").join("recalbox.conf");
    customizer::write_file(connection, &install, CONF, &path, &text, &new_text, rows(&wanted, &crate::batocera_settings::conf_values(&text)))?;
    let theme = if wanted.contains_key(THEME_KEY) { " The theme shows the next time Recalbox starts." } else { "" };
    Ok(format!("Saved to recalbox.conf. Recalbox uses these settings the next time a game starts.{theme} Undo is under \"Show every settings change\"."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::{db, installations};
    use std::fs;

    const CONF_TEXT: &str = "# ------------ Global options ------------\n## Set ratio for all emulators\nglobal.ratio=auto\n;global.shaderset=none\nglobal.smooth=1\n# snes.ratio=4/3\nemulationstation.theme.folder=recalbox-next\n";

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn edits_change_only_the_wanted_keys_like_recalboxs_menu() {
        let w = |pairs: &[(&str, Option<&str>)]| pairs.iter().map(|(k, v)| (k.to_string(), v.map(str::to_string))).collect::<BTreeMap<_, _>>();
        let out = conf_edit(CONF_TEXT, &w(&[("global.ratio", Some("4/3")), ("global.shaderset", Some("scanlines")), ("snes.ratio", Some("16/9"))])).unwrap();
        assert_eq!(
            out,
            "# ------------ Global options ------------\n## Set ratio for all emulators\nglobal.ratio=4/3\nglobal.shaderset=scanlines\nglobal.smooth=1\n# snes.ratio=4/3\nemulationstation.theme.folder=recalbox-next\nsnes.ratio=16/9\n",
            "a ;-disabled key is reactivated in place; a # comment stays a comment and the key is appended"
        );
        let back = conf_edit(&out, &w(&[("snes.ratio", None)])).unwrap();
        assert!(!back.contains("snes.ratio=16/9") && back.contains("# snes.ratio=4/3"));
    }

    #[test]
    fn settings_and_theme_are_reviewed_saved_and_undone() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        fs::create_dir_all(share.join("system")).unwrap();
        fs::create_dir_all(share.join("roms/snes")).unwrap();
        fs::write(share.join("roms/snes/Mario.sfc"), b"m").unwrap();
        fs::create_dir_all(share.join("themes/my-theme")).unwrap();
        fs::write(share.join("system/recalbox.conf"), CONF_TEXT).unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &share, "recalbox", &|_: &str| VolumeProbe::unknown()).unwrap();

        let o = options(&c, &id, &connected).unwrap();
        assert_eq!((o.systems.clone(), o.theme.clone(), o.themes.clone()), (vec!["snes".to_string()], "recalbox-next".to_string(), vec!["recalbox-next".to_string(), "my-theme".to_string()]));
        let ch = |k: &str, v: Option<&str>| ConfChange { key: k.into(), value: v.map(str::to_string) };
        // Refused: unknown values, global-only setting per system, unknown system or theme, "same as all systems" for all systems.
        assert!(preview(&c, &id, &[ch("global.ratio", Some("99/1"))], &connected).is_err());
        assert!(preview(&c, &id, &[ch("snes.integerscale", Some("1"))], &connected).is_err());
        assert!(preview(&c, &id, &[ch("nes.ratio", Some("4/3"))], &connected).is_err());
        assert!(preview(&c, &id, &[ch(THEME_KEY, Some("nope"))], &connected).is_err());
        assert!(preview(&c, &id, &[ch("global.ratio", None)], &connected).is_err());

        let changes = [ch("snes.ratio", Some("4/3")), ch("global.integerscale", Some("1")), ch(THEME_KEY, Some("my-theme"))];
        let p = preview(&c, &id, &changes, &connected).unwrap();
        assert_eq!(p.changes.len(), 3);
        assert!(apply(&c, &id, &changes, "stale", &connected).unwrap_err().contains("changed since"));
        let message = apply(&c, &id, &changes, &p.file_sha, &connected).unwrap();
        assert!(message.contains("theme"), "{message}");
        let text = fs::read_to_string(share.join("system/recalbox.conf")).unwrap();
        assert!(text.contains("snes.ratio=4/3\n") && text.contains("global.integerscale=1\n") && text.contains("emulationstation.theme.folder=my-theme\n"), "{text}");
        assert!(text.starts_with("# ------------ Global options ------------\n## Set ratio for all emulators\nglobal.ratio=auto\n;global.shaderset=none\n"), "other lines untouched");

        let change: i64 = c.query_row("SELECT id FROM settings_changes ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        customizer::undo(&c, change, &connected, &customizer::never_running).unwrap();
        assert_eq!(fs::read_to_string(share.join("system/recalbox.conf")).unwrap(), CONF_TEXT, "undo puts the file back");
    }

    #[test]
    fn one_game_gets_its_own_override_file_next_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        fs::create_dir_all(share.join("system")).unwrap();
        fs::create_dir_all(share.join("roms/snes/images")).unwrap();
        for g in ["F-Zero (USA).zip", "Mario.sfc", "Mario.sfc.recalbox.conf", "Mario.sfc.retroarch.cfg", "gamelist.xml"] {
            fs::write(share.join("roms/snes").join(g), b"m").unwrap();
        }
        fs::write(share.join("roms/snes/Mario.sfc.recalbox.conf"), "snes.smooth=0\n").unwrap();
        fs::write(share.join("system/recalbox.conf"), CONF_TEXT).unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &share, "recalbox", &|_: &str| VolumeProbe::unknown()).unwrap();

        assert_eq!(games(&c, &id, "snes", &connected).unwrap(), ["F-Zero (USA).zip", "Mario.sfc"], "override files, game lists and images aren't games");
        let mario = game_options(&c, &id, "snes", "Mario.sfc", &connected).unwrap();
        assert!(mario.exists && mario.values.get("snes.smooth").map(String::as_str) == Some("0"));
        let fzero = game_options(&c, &id, "snes", "F-Zero (USA).zip", &connected).unwrap();
        assert!(!fzero.exists && fzero.values.is_empty());

        let ch = |k: &str, v: Option<&str>| ConfChange { key: k.into(), value: v.map(str::to_string) };
        assert!(preview_game(&c, &id, "snes", "Nope.zip", &[ch("snes.ratio", Some("4/3"))], &connected).unwrap_err().contains("isn't in"));
        assert!(preview_game(&c, &id, "snes", "../system/recalbox.conf", &[ch("snes.ratio", Some("4/3"))], &connected).is_err());
        assert!(preview_game(&c, &id, "snes", "F-Zero (USA).zip", &[ch("global.ratio", Some("4/3"))], &connected).is_err(), "a system setting, not global");
        assert!(preview_game(&c, &id, "snes", "F-Zero (USA).zip", &[ch("snes.integerscale", Some("1"))], &connected).is_err(), "not a per-system setting");
        assert!(preview_game(&c, &id, "snes", "F-Zero (USA).zip", &[ch("snes.ratio", Some("99/1"))], &connected).is_err());

        // First change creates the file; undo moves it aside.
        let changes = [ch("snes.ratio", Some("8/7")), ch("snes.shaderset", Some("scanlines"))];
        let p = preview_game(&c, &id, "snes", "F-Zero (USA).zip", &changes, &connected).unwrap();
        assert!(p.changes.iter().all(|r| r.system == "snes: F-Zero (USA).zip"));
        assert!(apply_game(&c, &id, "snes", "F-Zero (USA).zip", &changes, "stale", &connected).unwrap_err().contains("changed since"));
        apply_game(&c, &id, "snes", "F-Zero (USA).zip", &changes, &p.file_sha, &connected).unwrap();
        let file = share.join("roms/snes/F-Zero (USA).zip.recalbox.conf");
        assert_eq!(fs::read_to_string(&file).unwrap(), "snes.ratio=8/7\nsnes.shaderset=scanlines\n");
        assert_eq!(fs::read_to_string(share.join("system/recalbox.conf")).unwrap(), CONF_TEXT, "recalbox.conf untouched");
        let change: i64 = c.query_row("SELECT id FROM settings_changes ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        customizer::undo(&c, change, &connected, &customizer::never_running).unwrap();
        assert!(!file.exists(), "undo moves the created file into the backup folder");
        assert!(walkdir::WalkDir::new(share.join(".retrodrive")).into_iter().filter_map(Result::ok).any(|e| e.file_name() == "F-Zero (USA).zip.recalbox.conf"), "kept, not deleted");

        // An existing file: only the changed line; "same as the system" removes it.
        let p = preview_game(&c, &id, "snes", "Mario.sfc", &[ch("snes.smooth", None), ch("snes.rewind", Some("0"))], &connected).unwrap();
        apply_game(&c, &id, "snes", "Mario.sfc", &[ch("snes.smooth", None), ch("snes.rewind", Some("0"))], &p.file_sha, &connected).unwrap();
        assert_eq!(fs::read_to_string(share.join("roms/snes/Mario.sfc.recalbox.conf")).unwrap(), "snes.rewind=0\n");
    }
}
