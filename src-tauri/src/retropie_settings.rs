//! RetroPie settings (RDS-023 step 4, 2026-11-07; docs/18 §28): the emulator each
//! system uses by default.
//!
//! Facts checked 2026-11-07 (RetroPie-Setup on GitHub):
//! - RetroPie shares `configs` (`/opt/retropie/configs`) next to `roms` over Samba
//!   (`scriptmodules/supplementary/samba.sh`).
//! - Each system's `configs/<system>/emulators.cfg` lists its installed emulators as
//!   `<id> = "<command>"` and the default as `default = "<id>"` (`addEmulator` in
//!   `scriptmodules/helpers.sh`, written with `iniConfig " = " '"'`). runcommand uses
//!   `default` when a game starts.
//! - Only the `default` line is changed, to one of the ids already in the file, so the
//!   choices are exactly what's installed on that Pi.
//! - Before writing, the installation (the roms share) must be connected with its share
//!   id (ADR-046), so the configs share beside it belongs to the same Pi; undo also
//!   requires the file to be exactly as RetroDrive left it.

use crate::customizer::{self, ChangeRow, PresenceFn};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct SystemEmulators {
    pub system: String,
    /// Emulator ids installed for this system, in file order.
    pub emulators: Vec<String>,
    pub default: Option<String>,
    pub sha: String,
}

#[derive(Debug, Serialize)]
pub struct RetroPieOptions {
    /// `None` when settings can't be reached (e.g. a USB stick), with the reason.
    pub problem: Option<String>,
    pub configs_path: Option<String>,
    pub systems: Vec<SystemEmulators>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmulatorChange {
    pub system: String,
    pub emulator: String,
}

#[derive(Debug, Serialize)]
pub struct EmulatorPreview {
    pub changes: Vec<ChangeRow>,
    /// SHA-256 of each file as reviewed, by system.
    pub shas: BTreeMap<String, String>,
}

/// `\\host\roms\…` → `\\host\configs\` (RetroPie's settings share on the same Pi).
pub fn configs_root(installation: &str) -> Option<String> {
    let rest = installation.strip_prefix(r"\\")?;
    let host = rest.split(['\\', '/']).next().filter(|h| !h.is_empty())?;
    Some(format!(r"\\{host}\configs\"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.+".contains(c))
}

/// Emulator ids and the default from an emulators.cfg text.
pub fn parse(text: &str) -> (Vec<String>, Option<String>) {
    let mut ids = Vec::new();
    let mut default = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim().trim_matches('"'));
        if key == "default" {
            default = Some(value.to_string()).filter(|v| !v.is_empty());
        } else if valid_id(key) && !ids.iter().any(|i| i == key) {
            ids.push(key.to_string());
        }
    }
    (ids, default)
}

/// Set `default = "<id>"`, changing only that line (or adding it).
pub fn set_default(text: &str, id: &str) -> Result<String, String> {
    let (ids, _) = parse(text);
    if !valid_id(id) || !ids.iter().any(|i| i == id) {
        return Err(format!("{id} isn't one of the emulators installed for this system."));
    }
    let mut out = String::new();
    let mut done = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let end = &line[body.len()..];
        if !done && body.split_once('=').is_some_and(|(k, _)| k.trim() == "default") && !body.trim_start().starts_with('#') {
            out.push_str(&format!("default = \"{id}\"{end}"));
            done = true;
        } else {
            out.push_str(line);
        }
    }
    if !done {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("default = \"{id}\"\n"));
    }
    // Check: the emulator list is unchanged and the default is the new one.
    let (after_ids, after_default) = parse(&out);
    if after_ids != ids || after_default.as_deref() != Some(id) {
        return Err("Editing emulators.cfg didn't come out as expected; nothing was changed.".into());
    }
    Ok(out)
}

fn has_games(dir: &Path) -> bool {
    fs::read_dir(dir).map(|r| r.filter_map(Result::ok).any(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        !(n == "gamelist.xml" || n == "images" || n.starts_with('.') || n.ends_with(".txt"))
    })).unwrap_or(false)
}

struct Context {
    install: customizer::Install,
    configs: PathBuf,
}

fn context(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Context, String> {
    let install = customizer::install_for(connection, installation_id, presence, &["retropie"], "These settings are for RetroPie installations.")?;
    let configs = configs_root(&install.root_text).ok_or("RetroPie's settings are reached over the network (\\\\RETROPIE\\configs). Add RetroPie by its network share to change them; a USB stick has no settings.")?;
    let configs = PathBuf::from(configs);
    if !configs.is_dir() {
        return Err("RetroPie's settings share (configs) can't be reached. Make sure RetroPie is on and its network shares are turned on.".into());
    }
    Ok(Context { install, configs })
}

/// Systems with games and their installed emulators (reads only).
pub fn options(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<RetroPieOptions, String> {
    let ctx = match context(connection, installation_id, presence) {
        Ok(c) => c,
        Err(problem) => return Ok(RetroPieOptions { problem: Some(problem), configs_path: None, systems: Vec::new() }),
    };
    let games = if ctx.install.root.join("roms").is_dir() { ctx.install.root.join("roms") } else { ctx.install.root.clone() };
    let mut systems = Vec::new();
    let mut names: Vec<String> = fs::read_dir(&ctx.configs).map(|r| r.filter_map(Result::ok).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    names.sort_by_key(|s| s.to_lowercase());
    for system in names {
        let file = ctx.configs.join(&system).join("emulators.cfg");
        if system == "all" || !file.is_file() || !has_games(&games.join(&system)) {
            continue;
        }
        let text = customizer::read_text(&file)?;
        let (emulators, default) = parse(&text);
        if emulators.is_empty() {
            continue;
        }
        systems.push(SystemEmulators { system, emulators, default, sha: customizer::hex_sha(text.as_bytes()) });
    }
    Ok(RetroPieOptions { problem: None, configs_path: Some(ctx.configs.display().to_string()), systems })
}

type Planned = Vec<(String, PathBuf, String, String, ChangeRow)>;

fn plan(ctx: &Context, changes: &[EmulatorChange]) -> Result<Planned, String> {
    if changes.is_empty() {
        return Err("Nothing to change.".into());
    }
    let mut out = Vec::new();
    for c in changes {
        if c.system.is_empty() || c.system.contains(['\\', '/', ':', '.']) {
            return Err(format!("{} isn't a system folder.", c.system));
        }
        let file = ctx.configs.join(&c.system).join("emulators.cfg");
        let text = customizer::read_text(&file).map_err(|_| format!("{} has no emulators.cfg on this RetroPie.", c.system))?;
        let new_text = set_default(&text, &c.emulator)?;
        let (_, before) = parse(&text);
        out.push((c.system.clone(), file, text, new_text, ChangeRow { system: c.system.clone(), key: "emulator".into(), before, after: Some(c.emulator.clone()) }));
    }
    Ok(out)
}

/// What the changes would do (reads only).
pub fn preview(connection: &Connection, installation_id: &str, changes: &[EmulatorChange], presence: PresenceFn) -> Result<EmulatorPreview, String> {
    let ctx = context(connection, installation_id, presence)?;
    let planned = plan(&ctx, changes)?;
    Ok(EmulatorPreview { shas: planned.iter().map(|(s, _, t, _, _)| (s.clone(), customizer::hex_sha(t.as_bytes()))).collect(), changes: planned.into_iter().map(|p| p.4).collect() })
}

/// Save reviewed changes: each file unchanged since the review; journaled, backed up
/// (in the configs share's `.retrodrive\backup`) and undoable. If one fails, the ones
/// already written are undone.
pub fn apply(connection: &Connection, installation_id: &str, changes: &[EmulatorChange], expected: &BTreeMap<String, String>, presence: PresenceFn) -> Result<String, String> {
    let ctx = context(connection, installation_id, presence)?;
    let planned = plan(&ctx, changes)?;
    for (system, _, text, _, _) in &planned {
        if expected.get(system) != Some(&customizer::hex_sha(text.as_bytes())) {
            return Err(format!("{system}'s emulators.cfg changed since you reviewed it, so nothing was changed. Review it again."));
        }
    }
    // The journal and backups live on the configs share; no share id there (the roms
    // share's id was checked above), and undo requires the exact file RetroDrive wrote.
    let configs_text = ctx.configs.display().to_string();
    let target = customizer::Install { id: ctx.install.id.clone(), root: ctx.configs.clone(), root_text: configs_text, stable: None };
    let mut ids = Vec::new();
    for (system, file, text, new_text, row) in planned {
        match customizer::write_file(connection, &target, &format!("{system}/emulators.cfg"), &file, &text, &new_text, vec![row]) {
            Ok((_, id)) => ids.push(id),
            Err(e) => {
                let undone = ids.iter().rev().all(|id| customizer::undo(connection, *id, presence, &customizer::never_running).is_ok());
                return Err(if undone { format!("{e} The other changes were undone.") } else { format!("{e} Some changes were saved; undo them under \"Show every settings change\".") });
            }
        }
    }
    Ok(format!("Saved. RetroPie uses the new emulator{} the next time a game starts. Undo is under \"Show every settings change\".", if ids.len() == 1 { "" } else { "s" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{Presence, VolumeProbe};
    use crate::{db, installations};

    const CFG: &str = "lr-snes9x2005 = \"/opt/retropie/emulators/retroarch/bin/retroarch -L x.so %ROM%\"\nlr-snes9x = \"/opt/retropie/emulators/retroarch/bin/retroarch -L y.so %ROM%\"\ndefault = \"lr-snes9x2005\"\n";

    #[test]
    fn only_the_default_line_changes_to_an_installed_emulator() {
        assert_eq!(parse(CFG), (vec!["lr-snes9x2005".to_string(), "lr-snes9x".to_string()], Some("lr-snes9x2005".to_string())));
        assert_eq!(set_default(CFG, "lr-snes9x").unwrap(), CFG.replace("default = \"lr-snes9x2005\"", "default = \"lr-snes9x\""));
        let no_default = "lr-fceumm = \"cmd\"\r\nlr-nestopia = \"cmd\"";
        assert_eq!(set_default(no_default, "lr-nestopia").unwrap(), "lr-fceumm = \"cmd\"\r\nlr-nestopia = \"cmd\"\ndefault = \"lr-nestopia\"\n");
        assert!(set_default(CFG, "lr-not-installed").is_err());
        assert!(set_default(CFG, "x\"; rm").is_err());
        assert_eq!(configs_root(r"\\RETROPIE\roms\"), Some(r"\\RETROPIE\configs\".into()));
        assert_eq!(configs_root(r"E:\retropie"), None);
    }

    #[test]
    fn emulator_choices_are_reviewed_saved_and_undone() {
        // A configs folder stands in for \\RETROPIE\configs: the test points the code at it.
        let dir = tempfile::tempdir().unwrap();
        let roms = dir.path().join("roms");
        let configs = dir.path().join("configs");
        for d in ["snes", "nes", "ports"] {
            fs::create_dir_all(roms.join(d)).unwrap();
        }
        fs::write(roms.join("snes/Mario.sfc"), b"m").unwrap();
        for d in ["snes", "nes", "all"] {
            fs::create_dir_all(configs.join(d)).unwrap();
            fs::write(configs.join(d).join("emulators.cfg"), CFG).unwrap();
        }
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let id = installations::add(&catalog, &roms, "retropie", &|_: &str| VolumeProbe::unknown()).unwrap();
        let connected = |_: &str, _: Option<&str>| Presence::Connected;
        // Local folders aren't a network share: settings explain why they're unavailable.
        assert!(options(&catalog, &id, &connected).unwrap().problem.unwrap().contains("network"));

        let ctx = Context { install: customizer::install_for(&catalog, &id, &connected, &["retropie"], "").unwrap(), configs: configs.clone() };
        let planned = plan(&ctx, &[EmulatorChange { system: "snes".into(), emulator: "lr-snes9x".into() }]).unwrap();
        assert_eq!(planned[0].4.before.as_deref(), Some("lr-snes9x2005"));
        assert!(plan(&ctx, &[EmulatorChange { system: "../snes".into(), emulator: "lr-snes9x".into() }]).is_err());
        let target = customizer::Install { id: id.clone(), root: configs.clone(), root_text: configs.display().to_string(), stable: None };
        let (system, file, text, new_text, row) = planned.into_iter().next().unwrap();
        customizer::write_file(&catalog, &target, &format!("{system}/emulators.cfg"), &file, &text, &new_text, vec![row]).unwrap();
        assert_eq!(parse(&fs::read_to_string(configs.join("snes/emulators.cfg")).unwrap()).1.as_deref(), Some("lr-snes9x"));
        assert_eq!(fs::read_to_string(configs.join("nes/emulators.cfg")).unwrap(), CFG, "other systems untouched");
        let h = customizer::history(&catalog, &id, &connected).unwrap();
        customizer::undo(&catalog, h[0].id, &connected, &customizer::never_running).unwrap();
        assert_eq!(fs::read_to_string(configs.join("snes/emulators.cfg")).unwrap(), CFG, "undo restores the file exactly");
    }
}
