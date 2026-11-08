//! Batocera settings and theme (RDS-024, step 3 of the owner-approved Batocera plan;
//! docs/18 §23).
//!
//! Facts checked 2026-11-07:
//! - Settings live in `userdata/system/batocera.conf`, lines `<key>=<value>`
//!   (https://wiki.batocera.org/batocera_conf_syntax). `global.<key>` applies to every
//!   system, `<system>.<key>` to one system; `#` starts a disabled line, `##` a comment;
//!   a missing key means AUTO. Batocera reads the file each time a game starts.
//! - Batocera's own menu saves the file by re-reading it and changing only the keys it
//!   changed (an active or `#`-disabled `key=` line is replaced; "auto" removes it; a
//!   key with no line is appended): `SystemConf::saveSystemConf` in
//!   batocera-emulationstation. RetroDrive edits the same way, so edits made over the
//!   network while Batocera runs aren't lost.
//! - The options and their values are Batocera's own (`_shared.emulator.yml` in
//!   batocera.linux: ratio, rewind, autosave, smooth, integerscale, shaderset, bezel;
//!   shader sets and `audio.bgmusic` from the default `batocera.conf`).
//! - Screen rotation is `display.rotate` (0, 1 = 90°, 2 = 180°, 3 = 270°; ADR-071), the
//!   key Batocera keeps in step with its boot settings (`S65values4boot`); the menu
//!   offers the same values for extra screens (`display.rotate2`/`3`, `GuiMenu.cpp`).
//! - The theme is `ThemeSet` in `userdata/system/configs/emulationstation/es_settings.cfg`
//!   (`Settings.cpp`); user themes are folders in `userdata/themes`, the built-in one
//!   is `es-theme-carbon` (inside the system image, not visible from Windows). Batocera
//!   rewrites es_settings.cfg when it quits if a menu setting was changed in that
//!   session (`Settings::saveFile`), so after a theme change over the network it should
//!   be restarted before other menu changes.
//! - Emulator and core lists are in the system image (`es_systems.cfg`), which Windows
//!   can't read, so they aren't offered yet.

use crate::customizer::{self, ChangeRow, Choice, PresenceFn, Preview};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const CONF: &str = "system/batocera.conf";
pub const ES_SETTINGS: &str = "system/configs/emulationstation/es_settings.cfg";
pub const BUILTIN_THEME: &str = "es-theme-carbon";

/// One setting RetroDrive offers, with Batocera's own values.
#[derive(Debug, Clone, Serialize)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// `global` + systems; otherwise the key's fixed prefix is `scope`.
    pub per_system: bool,
    /// The prefix of a setting that isn't per system (`audio`, `display`).
    pub scope: &'static str,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize)]
pub struct BatoceraOptions {
    pub settings: Vec<Setting>,
    /// System folders with games on this Batocera.
    pub systems: Vec<String>,
    /// Current active values, by full key (`global.ratio`, `snes.ratio`, `audio.bgmusic`).
    pub values: BTreeMap<String, String>,
    pub conf_sha: String,
    /// Theme folders the user added, plus the built-in one first.
    pub themes: Vec<String>,
    pub theme: Option<String>,
    /// `None` if es_settings.cfg doesn't exist yet (Batocera never started).
    pub es_sha: Option<String>,
    pub network: bool,
}

/// A change: full key and the new value (`None` = AUTO: Batocera decides).
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

pub fn settings(root: &Path) -> Vec<Setting> {
    let mut bezels = vec![
        ("Default", "default".to_string()),
        ("None", "none".to_string()),
    ];
    for folder in folders(&root.join("decorations")) {
        if folder != "default" && folder != "none" {
            bezels.push(("", folder));
        }
    }
    let bezel_choices = bezels
        .iter()
        .map(|(label, value)| Choice {
            label: if label.is_empty() {
                value.clone()
            } else {
                label.to_string()
            },
            value: value.clone(),
        })
        .collect();
    vec![
        Setting {
            key: "ratio",
            label: "Game aspect ratio",
            help: "Force games to this shape. Not every system supports every ratio.",
            per_system: true,
            scope: "global",
            choices: choices(&[
                ("4/3", "4/3"),
                ("16/9", "16/9"),
                ("Full", "full"),
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
                ("Config", "config"),
                ("Square pixel", "squarepixel"),
                ("Core provided", "core"),
                ("Custom", "custom"),
            ]),
        },
        Setting {
            key: "shaderset",
            label: "Shader set",
            help: "A look for the picture, such as scanlines or a CRT curve.",
            per_system: true,
            scope: "global",
            choices: choices(&[
                ("Default", "default"),
                ("None", "none"),
                ("Curvature", "curvature"),
                ("Enhanced", "enhanced"),
                ("Flatten glow", "flatten-glow"),
                ("Mega bezel", "mega-bezel"),
                ("Retro", "retro"),
                ("Scanlines", "scanlines"),
                ("Zfast", "zfast"),
            ]),
        },
        Setting { key: "bezel", label: "Decorations", help: "The frame (bezel) shown around games.", per_system: true, scope: "global", choices: bezel_choices },
        Setting { key: "smooth", label: "Smooth games", help: "Bilinear filtering; off when a shader set is used.", per_system: true, scope: "global", choices: choices(ON_OFF) },
        Setting { key: "integerscale", label: "Integer scaling", help: "Pixel perfect: only whole-number enlargements.", per_system: true, scope: "global", choices: choices(ON_OFF) },
        Setting { key: "rewind", label: "Rewind", help: "Go back in time while playing, where the emulator supports it.", per_system: true, scope: "global", choices: choices(ON_OFF) },
        Setting { key: "autosave", label: "Auto save/load", help: "Save when you quit a game and pick up there next time.", per_system: true, scope: "global", choices: choices(ON_OFF) },
        Setting {
            key: "rotate",
            label: "Screen rotation",
            help: "Turn the whole picture, e.g. for a TV or monitor stood on its side for vertical games. Takes effect after Batocera restarts.",
            per_system: false,
            scope: "display",
            choices: choices(&[("Normal (0°)", "0"), ("90° (vertical)", "1"), ("180° (upside down)", "2"), ("270° (vertical)", "3")]),
        },
        Setting { key: "bgmusic", label: "Menu music", help: "Background music and sounds in Batocera's menus.", per_system: false, scope: "audio", choices: choices(ON_OFF) },
    ]
}

pub(crate) fn folders(dir: &Path) -> Vec<String> {
    let mut list: Vec<String> = fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|s| s.to_lowercase());
    list
}

/// System folders under `roms` with at least one game file.
pub(crate) fn systems(root: &Path) -> Vec<String> {
    let skip = |name: &str| {
        name.eq_ignore_ascii_case("_info.txt")
            || name.eq_ignore_ascii_case("gamelist.xml")
            || name.eq_ignore_ascii_case("images")
            || name.starts_with('.')
    };
    folders(&root.join("roms"))
        .into_iter()
        .filter(|s| {
            fs::read_dir(root.join("roms").join(s))
                .map(|r| {
                    r.filter_map(Result::ok)
                        .any(|e| !skip(&e.file_name().to_string_lossy()))
                })
                .unwrap_or(false)
        })
        .collect()
}

/// The key on an active `key=value` line (not disabled with `#` or `;`).
fn active_key(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    (!key.is_empty() && !key.contains(char::is_whitespace)).then_some(key)
}

/// Active values in a batocera.conf text.
pub fn conf_values(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.split_inclusive('\n') {
        if let Some(key) = active_key(line) {
            let value = line.trim_end_matches(['\r', '\n'])[key.len() + 1..].to_string();
            out.insert(key.to_string(), value);
        }
    }
    out
}

/// Change only the given keys, the way Batocera's own menu does: an active (or
/// `#`/`;`-disabled) `key=` line gets the new value, AUTO removes the active lines, a
/// key without a line is appended. Every other line stays byte for byte.
pub fn conf_edit(text: &str, wanted: &BTreeMap<String, Option<String>>) -> Result<String, String> {
    edit_conf(text, wanted, &['#', ';'], "batocera.conf")
}

/// `conf_edit` for any `key=value` file: `disabled` are the marks of a disabled
/// `key=` line that a new value reactivates in place (Batocera `#`/`;`; Recalbox `;`).
pub(crate) fn edit_conf(
    text: &str,
    wanted: &BTreeMap<String, Option<String>>,
    disabled: &[char],
    file: &str,
) -> Result<String, String> {
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    for (key, value) in wanted {
        if let Some(v) = value {
            if v.contains(['\r', '\n']) {
                return Err(format!(
                    "The value for {key} has a line break, so nothing was changed."
                ));
            }
        }
        let prefix = format!("{key}=");
        let line_end = |l: &str| {
            if l.ends_with("\r\n") {
                "\r\n"
            } else if l.ends_with('\n') {
                "\n"
            } else {
                ""
            }
            .to_string()
        };
        let active: Vec<usize> = (0..lines.len())
            .filter(|&i| active_key(&lines[i]) == Some(key.as_str()))
            .collect();
        match value {
            Some(v) if !active.is_empty() => {
                for &i in &active {
                    let end = line_end(&lines[i]);
                    lines[i] = format!("{prefix}{v}{end}");
                }
            }
            Some(v) => {
                if let Some(i) = lines.iter().position(|l| {
                    l.strip_prefix(disabled)
                        .is_some_and(|rest| rest.starts_with(&prefix))
                }) {
                    let end = line_end(&lines[i]);
                    lines[i] = format!("{prefix}{v}{end}");
                } else {
                    if let Some(last) = lines.last_mut() {
                        if !last.ends_with('\n') {
                            last.push_str(ending);
                        }
                    }
                    lines.push(format!("{prefix}{v}{ending}"));
                }
            }
            None => {
                for &i in active.iter().rev() {
                    lines.remove(i);
                }
            }
        }
    }
    let out: String = lines.concat();
    // Check: exactly the wanted keys changed.
    let (before, after) = (conf_values(text), conf_values(&out));
    for (key, value) in wanted {
        if after.get(key) != value.as_ref() {
            return Err(format!(
                "Editing {file} didn't set {key} as expected; nothing was changed."
            ));
        }
    }
    let others = |m: &BTreeMap<String, String>| {
        m.iter()
            .filter(|(k, _)| !wanted.contains_key(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    if others(&before) != others(&after) {
        return Err(format!(
            "Editing {file} would have changed other settings; nothing was changed."
        ));
    }
    Ok(out)
}

fn install(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
) -> Result<customizer::Install, String> {
    customizer::install_for(
        connection,
        installation_id,
        presence,
        &["batocera"],
        "These settings are for Batocera installations.",
    )
}

fn read(root: &Path, relative: &str, missing: &str) -> Result<String, String> {
    let path = root.join(relative.replace('/', "\\"));
    if !path.is_file() {
        return Err(missing.into());
    }
    customizer::read_text(&path)
}

const NO_CONF: &str = "system\\batocera.conf is missing. Start Batocera once with this drive or share so it creates it.";
const NO_ES: &str = "Batocera's interface settings (system\\configs\\emulationstation\\es_settings.cfg) are missing. Start Batocera once so it creates them.";

/// What can be set, with the current values (reads only).
pub fn options(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
) -> Result<BatoceraOptions, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root, CONF, NO_CONF)?;
    let es = read(&install.root, ES_SETTINGS, NO_ES).ok();
    let theme = es
        .as_deref()
        .and_then(|t| customizer::entries(t).ok())
        .and_then(|e| customizer::values(&e).ok())
        .and_then(|v| v.get("ThemeSet").cloned())
        .filter(|t| !t.is_empty());
    let mut themes = vec![BUILTIN_THEME.to_string()];
    themes.extend(
        folders(&install.root.join("themes"))
            .into_iter()
            .filter(|t| t != BUILTIN_THEME),
    );
    Ok(BatoceraOptions {
        settings: settings(&install.root),
        systems: systems(&install.root),
        values: conf_values(&text),
        conf_sha: customizer::hex_sha(text.as_bytes()),
        themes,
        theme,
        es_sha: es.map(|t| customizer::hex_sha(t.as_bytes())),
        network: install.root_text.starts_with(r"\\"),
    })
}

/// Checked changes: known settings, known scopes, offered values.
fn validate(
    root: &Path,
    changes: &[ConfChange],
) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("Nothing to change.".into());
    }
    let offered = settings(root);
    let systems = systems(root);
    let mut wanted = BTreeMap::new();
    for c in changes {
        let (scope, key) = c
            .key
            .split_once('.')
            .ok_or_else(|| format!("Unknown setting {}.", c.key))?;
        let setting = offered
            .iter()
            .find(|s| s.key == key)
            .ok_or_else(|| format!("RetroDrive doesn't change {key} on Batocera."))?;
        let scope_ok = if setting.per_system {
            scope == "global" || systems.iter().any(|s| s == scope)
        } else {
            scope == setting.scope
        };
        if !scope_ok {
            return Err(format!("{} can't be set for {scope}.", setting.label));
        }
        if let Some(v) = &c.value {
            if !setting.choices.iter().any(|ch| &ch.value == v) {
                return Err(format!(
                    "{v} isn't one of Batocera's choices for {}.",
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
            let (scope, name) = key.split_once('.').unwrap_or(("global", key));
            ChangeRow {
                system: if scope == "global" {
                    "all systems".into()
                } else {
                    scope.into()
                },
                key: name.into(),
                before: current.get(key).cloned(),
                after: value.clone(),
            }
        })
        .collect()
}

/// What the changes would do (reads only).
pub fn preview(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    presence: PresenceFn,
) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root, CONF, NO_CONF)?;
    let wanted = validate(&install.root, changes)?;
    conf_edit(&text, &wanted)?;
    Ok(Preview {
        changes: rows(&wanted, &conf_values(&text)),
        file_sha: customizer::hex_sha(text.as_bytes()),
    })
}

/// Save reviewed changes to batocera.conf: only if it's unchanged since the review;
/// journaled, backed up and undoable (`customizer::write_file`).
pub fn apply(
    connection: &Connection,
    installation_id: &str,
    changes: &[ConfChange],
    expected_sha: &str,
    presence: PresenceFn,
) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read(&install.root, CONF, NO_CONF)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("batocera.conf changed since you reviewed it (perhaps from Batocera's menu), so nothing was changed. Review the change again.".into());
    }
    let wanted = validate(&install.root, changes)?;
    let new_text = conf_edit(&text, &wanted)?;
    let path = install.root.join(CONF.replace('/', "\\"));
    customizer::write_file(
        connection,
        &install,
        CONF,
        &path,
        &text,
        &new_text,
        rows(&wanted, &conf_values(&text)),
    )?;
    Ok("Saved to batocera.conf. Batocera uses these settings the next time a game starts. Undo is under \"Show every settings change\".".into())
}

type ThemeChange = (
    std::path::PathBuf,
    String,
    BTreeMap<String, String>,
    BTreeMap<String, Option<String>>,
);

fn theme_change(install: &customizer::Install, theme: &str) -> Result<ThemeChange, String> {
    if theme != BUILTIN_THEME
        && !folders(&install.root.join("themes"))
            .iter()
            .any(|t| t == theme)
    {
        return Err(format!("There's no theme called {theme} on this Batocera."));
    }
    let text = read(&install.root, ES_SETTINGS, NO_ES)?;
    let current = customizer::values(&customizer::entries(&text)?)?;
    if current.get("ThemeSet").map(String::as_str) == Some(theme) {
        return Err(format!("{theme} is already Batocera's theme."));
    }
    let wanted: BTreeMap<String, Option<String>> =
        [("ThemeSet".to_string(), Some(theme.to_string()))]
            .into_iter()
            .collect();
    Ok((
        install.root.join(ES_SETTINGS.replace('/', "\\")),
        text,
        current,
        wanted,
    ))
}

/// What switching Batocera's theme would change (reads only).
pub fn preview_theme(
    connection: &Connection,
    installation_id: &str,
    theme: &str,
    presence: PresenceFn,
) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let (_, text, current, wanted) = theme_change(&install, theme)?;
    customizer::edit_typed(&text, &wanted, &BTreeMap::new())?;
    Ok(Preview {
        changes: customizer::rows(&wanted, &current),
        file_sha: customizer::hex_sha(text.as_bytes()),
    })
}

/// Make `theme` Batocera's theme (`ThemeSet`); journaled, backed up and undoable.
pub fn apply_theme(
    connection: &Connection,
    installation_id: &str,
    theme: &str,
    expected_sha: &str,
    presence: PresenceFn,
) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let (path, text, current, wanted) = theme_change(&install, theme)?;
    if customizer::hex_sha(text.as_bytes()) != expected_sha {
        return Err("Batocera's interface settings changed since you reviewed this, so nothing was changed. Review it again.".into());
    }
    let new_text = customizer::edit_typed(&text, &wanted, &BTreeMap::new())?;
    customizer::write_file(
        connection,
        &install,
        ES_SETTINGS,
        &path,
        &text,
        &new_text,
        customizer::rows(&wanted, &current),
    )?;
    Ok(if install.root_text.starts_with(r"\\") {
        format!("{theme} is set. Restart Batocera now (Start → Quit → Restart system) to see it, before changing other settings in its menu, or Batocera may save over this.")
    } else {
        format!("{theme} is set. Batocera shows it the next time it starts.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::{db, installations};

    const CONF_TEXT: &str = "## Batocera settings\nsystem.hostname=BATOCERA\naudio.bgmusic=1\n#global.ratio=auto\n#global.shaderset=none\nsnes.smooth=1\nsnes[\"Mario.sfc\"].ratio=4/3\n";

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn edits_change_only_the_wanted_keys_like_batoceras_menu() {
        let w = |pairs: &[(&str, Option<&str>)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
                .collect::<BTreeMap<_, _>>()
        };
        // A disabled default becomes active in place; an active line changes; AUTO removes; new keys are appended.
        let out = conf_edit(
            CONF_TEXT,
            &w(&[
                ("global.ratio", Some("16/9")),
                ("snes.smooth", None),
                ("audio.bgmusic", Some("0")),
                ("nes.rewind", Some("1")),
            ]),
        )
        .unwrap();
        assert_eq!(out, "## Batocera settings\nsystem.hostname=BATOCERA\naudio.bgmusic=0\nglobal.ratio=16/9\n#global.shaderset=none\nsnes[\"Mario.sfc\"].ratio=4/3\nnes.rewind=1\n");
        // CRLF files keep CRLF; a file without a final newline gets one before an added line.
        let crlf = conf_edit("a=1\r\nb=2", &w(&[("c", Some("3"))])).unwrap();
        assert_eq!(crlf, "a=1\r\nb=2\r\nc=3\r\n");
        assert!(
            conf_edit(
                CONF_TEXT,
                &w(&[("global.ratio", Some("4/3\nsystem.hostname=X"))])
            )
            .is_err(),
            "no line breaks in values"
        );
        assert_eq!(
            conf_values(CONF_TEXT)
                .get("snes.smooth")
                .map(String::as_str),
            Some("1")
        );
        assert!(
            !conf_values(CONF_TEXT).contains_key("global.ratio"),
            "disabled lines aren't values"
        );
    }

    #[test]
    fn settings_and_theme_are_reviewed_saved_and_undone() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        for d in [
            "roms/snes",
            "roms/nes",
            "bios",
            "system/configs/emulationstation",
            "themes/my-theme",
            "decorations/arcade",
        ] {
            fs::create_dir_all(share.join(d)).unwrap();
        }
        fs::write(share.join("roms/snes/Mario.sfc"), b"m").unwrap();
        fs::write(share.join("roms/nes/_info.txt"), b"").unwrap();
        fs::write(share.join("system/batocera.conf"), CONF_TEXT).unwrap();
        let es = "<?xml version=\"1.0\"?>\n<config>\n\t<string name=\"ThemeSet\" value=\"es-theme-carbon\" />\n</config>\n";
        fs::write(
            share.join("system/configs/emulationstation/es_settings.cfg"),
            es,
        )
        .unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let id = installations::add(&catalog, &share, "batocera", &|_: &str| {
            VolumeProbe::unknown()
        })
        .unwrap();

        let o = options(&catalog, &id, &connected).unwrap();
        assert_eq!(o.systems, ["snes"], "only systems with games");
        assert_eq!(
            (o.theme.as_deref(), o.themes.clone()),
            (
                Some("es-theme-carbon"),
                vec!["es-theme-carbon".to_string(), "my-theme".to_string()]
            )
        );
        assert!(
            o.settings
                .iter()
                .find(|s| s.key == "bezel")
                .unwrap()
                .choices
                .iter()
                .any(|c| c.value == "arcade"),
            "user decorations offered"
        );

        let change = |k: &str, v: Option<&str>| ConfChange {
            key: k.into(),
            value: v.map(str::to_string),
        };
        assert!(
            preview(
                &catalog,
                &id,
                &[change("nes.ratio", Some("4/3"))],
                &connected
            )
            .is_err(),
            "no games, no system"
        );
        assert!(
            preview(
                &catalog,
                &id,
                &[change("snes.ratio", Some("5/3"))],
                &connected
            )
            .is_err(),
            "not a Batocera value"
        );
        assert!(
            preview(
                &catalog,
                &id,
                &[change("global.hostname", Some("x"))],
                &connected
            )
            .is_err(),
            "not an offered setting"
        );
        assert!(
            preview(
                &catalog,
                &id,
                &[change("global.bgmusic", Some("0"))],
                &connected
            )
            .is_err(),
            "menu music is audio.bgmusic"
        );
        assert!(
            preview(
                &catalog,
                &id,
                &[change("display.rotate", Some("1"))],
                &connected
            )
            .is_ok(),
            "screen rotation is display.rotate"
        );
        assert!(preview(
            &catalog,
            &id,
            &[change("display.rotate", Some("4"))],
            &connected
        )
        .is_err());
        assert!(preview(
            &catalog,
            &id,
            &[change("global.rotate", Some("1"))],
            &connected
        )
        .is_err());
        let changes = [
            change("global.shaderset", Some("scanlines")),
            change("snes.smooth", None),
        ];
        let p = preview(&catalog, &id, &changes, &connected).unwrap();
        assert_eq!(
            fs::read_to_string(share.join("system/batocera.conf")).unwrap(),
            CONF_TEXT,
            "previewing changes nothing"
        );
        assert!(apply(&catalog, &id, &changes, "stale", &connected)
            .unwrap_err()
            .contains("changed since"));
        apply(&catalog, &id, &changes, &p.file_sha, &connected).unwrap();
        let now = fs::read_to_string(share.join("system/batocera.conf")).unwrap();
        assert!(
            now.contains("\nglobal.shaderset=scanlines\n")
                && !now.contains("snes.smooth")
                && now.contains("system.hostname=BATOCERA")
        );

        let t = preview_theme(&catalog, &id, "my-theme", &connected).unwrap();
        assert!(preview_theme(&catalog, &id, "missing", &connected).is_err());
        let msg = apply_theme(&catalog, &id, "my-theme", &t.file_sha, &connected).unwrap();
        assert!(msg.contains("next time it starts"), "{msg}");
        assert_eq!(
            options(&catalog, &id, &connected).unwrap().theme.as_deref(),
            Some("my-theme")
        );

        // Undo both, newest first: files are back exactly.
        for h in customizer::history(&catalog, &id, &connected).unwrap() {
            customizer::undo(&catalog, h.id, &connected, &customizer::never_running).unwrap();
        }
        assert_eq!(
            fs::read_to_string(share.join("system/batocera.conf")).unwrap(),
            CONF_TEXT
        );
        assert_eq!(
            fs::read_to_string(share.join("system/configs/emulationstation/es_settings.cfg"))
                .unwrap(),
            es
        );
        // Not for RetroBat installations.
        assert!(customizer::install(&catalog, &id, &connected).is_err());
    }
}
