//! Controllers, step 2 (RDS-028; owner 2026-11-07 "Yes, start step 2"; ADR-064;
//! docs/18 §38): set a confirmed controller up on the user's systems. Adds files
//! only; never edits or replaces a setup that is already there. Every write goes
//! through `customizer::write_file` (journaled, backed up, undoable).
//!
//! - **RetroBat**: its EmulationStation loads `emulationstation\.emulationstation\
//!   gamecontrollerdb.txt` (a user file RetroBat updates never overwrite) into SDL and
//!   then sets the controller up itself (RetroBat-Official/emulationstation,
//!   `InputManager.cpp`). RetroDrive appends the SDL GameControllerDB line(s) for the
//!   controller, only when RetroBat doesn't know it already.
//! - **RetroPie** (added by its network share): RetroArch's ready-made udev setup file
//!   into `\\host\configs\all\retroarch-joypads\`, unless one is there for it.
//! - **ES-DE**: ES-DE itself uses any controller SDL knows; for games, RetroArch's
//!   ready-made setup file for its controller driver into its autoconfig folder.
//! - **Batocera, Recalbox**, and controllers with no ready-made setup: plain steps in
//!   the system's own menu (Batocera's setups need the Linux device name, which can't
//!   be known from Windows).
//!
//! Xbox-style (XInput) controllers already work in RetroBat, ES-DE and RetroArch on
//! Windows, so nothing is written for them there.

use crate::customizer::{self, ChangeRow, Install, PresenceFn};
use crate::volumes::Presence;
use crate::{bios_step, installations, retropie_settings};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SDL_WINDOWS: &str = include_str!("../resources/sdl_windows.txt");
const RA_JOYPADS: &str = include_str!("../resources/retroarch_joypads.txt");

/// One installation's state for a controller.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SetupTarget {
    pub installation_id: String,
    pub adapter_id: String,
    pub location: String,
    /// `ready` (works already), `add` (RetroDrive can set it up), `manual` (steps in
    /// the system's menu), `unavailable` (e.g. not connected).
    pub status: &'static str,
    pub message: String,
    /// Files RetroDrive would add.
    pub files: Vec<String>,
}

/// What to do for one installation.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Ready(String),
    Manual(String),
    Unavailable(String),
    /// Files to add under `root` (relative path, full path, old text, new text).
    Add { root: PathBuf, stable: Option<String>, files: Vec<(String, PathBuf, String, String)>, message: String },
}

// ---------- bundled data ----------

/// `vid:pid` (lower-case hex) from a Windows SDL GUID with the usual USB/Bluetooth layout.
fn guid_usb(guid: &str) -> Option<(String, String)> {
    let g = guid.to_lowercase();
    if g.len() != 32 || !g.chars().all(|c| c.is_ascii_hexdigit()) || !matches!(&g[0..4], "0300" | "0500") || &g[12..16] != "0000" || &g[20..24] != "0000" {
        return None;
    }
    Some((format!("{}{}", &g[10..12], &g[8..10]), format!("{}{}", &g[18..20], &g[16..18])))
}

/// SDL GameControllerDB Windows lines for this controller.
fn sdl_lines_for(vid: &str, pid: &str) -> Vec<&'static str> {
    SDL_WINDOWS.lines().filter(|l| !l.starts_with('#') && line_is_for(l, vid, pid)).collect()
}

fn line_is_for(line: &str, vid: &str, pid: &str) -> bool {
    line.split(',').next().and_then(guid_usb).is_some_and(|(v, p)| v == vid && p == pid)
}

/// One RetroArch ready-made setup file.
struct RaFile {
    driver: &'static str,
    name: &'static str,
    text: &'static str,
    usb: Option<(String, String)>,
    device: Option<String>,
}

fn ra_files() -> &'static [RaFile] {
    static FILES: OnceLock<Vec<RaFile>> = OnceLock::new();
    FILES.get_or_init(|| {
        // "@@ driver/file name" starts each file; the first chunk is the header comment.
        RA_JOYPADS
            .split("\n@@ ")
            .skip(1)
            .filter_map(|chunk| {
                let (header, text) = chunk.split_once('\n')?;
                let (driver, name) = header.split_once('/')?;
                let (usb, device) = ra_fields(text);
                Some(RaFile { driver, name, text, usb, device })
            })
            .collect()
    })
}

/// A USB ID as RetroArch writes it (decimal, or `0x…`) → 4 hex digits.
fn ra_id(value: &str) -> Option<String> {
    let v = value.trim().trim_matches('"').trim();
    let n = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => v.parse::<u32>().ok()?,
    };
    (n <= 0xffff).then(|| format!("{n:04x}"))
}

/// (`vid:pid`, `input_device`) from a RetroArch setup file's text.
fn ra_fields(text: &str) -> (Option<(String, String)>, Option<String>) {
    let mut vid = None;
    let mut pid = None;
    let mut device = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "input_vendor_id" => vid = ra_id(v),
            "input_product_id" => pid = ra_id(v),
            "input_device" => device = Some(v.trim().trim_matches('"').to_string()).filter(|d| !d.is_empty()),
            _ => {}
        }
    }
    (vid.zip(pid), device)
}

fn ra_for(driver: &str, vid: &str, pid: &str) -> Vec<&'static RaFile> {
    ra_files().iter().filter(|f| f.driver == driver && f.usb.as_ref().is_some_and(|(v, p)| v == vid && p == pid)).collect()
}

/// Whether a folder of RetroArch setup files already has one for this controller
/// (same USB ID, or the same device name as one of `candidates`).
fn folder_has(dir: &Path, vid: &str, pid: &str, candidates: &[&RaFile]) -> bool {
    let Ok(entries) = fs::read_dir(dir) else { return false };
    entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("cfg"))).any(|e| {
        let text = fs::read_to_string(e.path()).unwrap_or_default();
        let (usb, device) = ra_fields(&text);
        usb.is_some_and(|(v, p)| v == vid && p == pid) || device.is_some_and(|d| candidates.iter().any(|c| c.device.as_deref() == Some(d.as_str())))
    })
}

/// New RetroArch files for `dir` (skipping names already taken: those are kept).
fn ra_additions(dir: &Path, prefix: &str, candidates: &[&RaFile]) -> Vec<(String, PathBuf, String, String)> {
    candidates
        .iter()
        .filter(|c| !dir.join(c.name).exists())
        .map(|c| (format!("{prefix}{}", c.name), dir.join(c.name), String::new(), format!("{}\n", c.text.trim_end())))
        .collect()
}

// ---------- per platform ----------

const RETROBAT_SHIPPED: &str = "emulationstation/gamecontrollerdb.txt";
const RETROBAT_USER: &str = "emulationstation/.emulationstation/gamecontrollerdb.txt";
const RETROBAT_INPUT: &str = "emulationstation/.emulationstation/es_input.cfg";

fn plan_retrobat(root: &Path, stable: Option<String>, vid: &str, pid: &str, xinput: bool) -> Plan {
    if xinput {
        return Plan::Ready("Works already: RetroBat sets up Xbox-style controllers by itself.".into());
    }
    let known_in = |rel: &str| fs::read_to_string(root.join(rel)).unwrap_or_default().lines().any(|l| line_is_for(l.trim(), vid, pid));
    if known_in(RETROBAT_SHIPPED) || known_in(RETROBAT_USER) {
        return Plan::Ready("Works already: RetroBat knows this controller and sets it up when you plug it in.".into());
    }
    let input = fs::read_to_string(root.join(RETROBAT_INPUT)).unwrap_or_default();
    if input.split("deviceGUID=\"").skip(1).filter_map(|s| s.split('"').next()).any(|g| guid_usb(g).is_some_and(|(v, p)| v == vid && p == pid)) {
        return Plan::Ready("Already set up in RetroBat.".into());
    }
    let lines = sdl_lines_for(vid, pid);
    if lines.is_empty() {
        return Plan::Manual("RetroDrive has no ready-made setup for this controller. In RetroBat, press Start → Controller settings → Configure a controller, and follow the prompts.".into());
    }
    let user = root.join(RETROBAT_USER);
    if !user.parent().is_some_and(Path::is_dir) {
        return Plan::Unavailable("Start RetroBat once first, then try again.".into());
    }
    let old = fs::read_to_string(&user).unwrap_or_default();
    let nl = if old.contains("\r\n") { "\r\n" } else { "\n" };
    let mut new = old.clone();
    if !new.is_empty() && !new.ends_with('\n') {
        new.push_str(nl);
    }
    new.push_str(&format!("# Added by RetroDrive (from SDL GameControllerDB) for USB {vid}:{pid}{nl}"));
    for l in &lines {
        new.push_str(l);
        new.push_str(nl);
    }
    Plan::Add {
        root: root.to_path_buf(),
        stable,
        files: vec![(RETROBAT_USER.into(), user, old, new)],
        message: "RetroBat sets it up the next time it starts (or when you plug the controller in).".into(),
    }
}

fn plan_retropie(configs: &Path, vid: &str, pid: &str) -> Plan {
    let dir = configs.join("all").join("retroarch-joypads");
    let candidates = ra_for("udev", vid, pid);
    if !dir.is_dir() {
        return Plan::Unavailable(format!("{} can't be reached. Make sure the Pi is on and connected.", dir.display()));
    }
    if folder_has(&dir, vid, pid, &candidates) {
        return Plan::Ready("Already set up: RetroPie has a setup file for this controller.".into());
    }
    let files = ra_additions(&dir, "all/retroarch-joypads/", &candidates);
    if files.is_empty() {
        return Plan::Manual("RetroDrive has no ready-made setup for this controller. On the Pi, press Start → Configure Input, and follow the prompts.".into());
    }
    Plan::Add { root: configs.to_path_buf(), stable: None, files, message: "Games use it from the next one you start. RetroPie's menus use the controller you set up when it first started.".into() }
}

/// RetroArch's autoconfig folder and controller driver, from `retroarch.cfg`.
fn retroarch_settings(ra: &Path) -> (PathBuf, String) {
    let text = fs::read_to_string(ra.join("retroarch.cfg")).unwrap_or_default();
    let get = |key: &str| text.lines().find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().trim_matches('"').to_string()));
    let dir = match get("joypad_autoconfig_dir").as_deref() {
        None | Some("") | Some("default") => ra.join("autoconfig"),
        Some(v) if v.starts_with(":\\") || v.starts_with(":/") => ra.join(v[2..].trim_start_matches(['\\', '/'])),
        Some(v) if v.len() > 2 && (v.as_bytes()[1] == b':' || v.starts_with(r"\\")) => PathBuf::from(v),
        Some(_) => ra.join("autoconfig"),
    };
    (dir, get("input_joypad_driver").filter(|d| !d.is_empty()).unwrap_or_else(|| "xinput".into()))
}

fn plan_retroarch(ra: &Path, vid: &str, pid: &str, xinput: bool) -> Plan {
    let (autoconfig, driver) = retroarch_settings(ra);
    if xinput && driver != "sdl2" {
        return Plan::Ready("Works already: ES-DE and RetroArch set up Xbox-style controllers by themselves.".into());
    }
    // RetroArch's xinput driver hands other controllers to DirectInput.
    let files_driver = match driver.as_str() {
        "xinput" | "dinput" => "dinput",
        "sdl2" => "sdl2",
        _ => return Plan::Manual(format!("RetroArch uses the {driver} controller driver here. In RetroArch: Settings → Input → Port 1 Controls → Set All Controls.")),
    };
    if !autoconfig.is_dir() {
        return Plan::Unavailable(format!("RetroArch's controller setup folder ({}) isn't there yet. Start RetroArch once, then try again.", autoconfig.display()));
    }
    let dir = autoconfig.join(files_driver);
    let candidates = ra_for(files_driver, vid, pid);
    if folder_has(&dir, vid, pid, &candidates) || folder_has(&autoconfig, vid, pid, &candidates) {
        return Plan::Ready("Already set up: RetroArch has a setup file for this controller.".into());
    }
    let prefix = format!("{}/{files_driver}/", autoconfig.strip_prefix(ra).map(|p| p.display().to_string().replace('\\', "/")).unwrap_or_else(|_| "autoconfig".into()));
    let files = ra_additions(&dir, &prefix, &candidates);
    if files.is_empty() {
        return Plan::Manual("RetroDrive has no ready-made setup for this controller in RetroArch. In RetroArch: Settings → Input → Port 1 Controls → Set All Controls. ES-DE's own menus use any controller SDL knows.".into());
    }
    Plan::Add { root: ra.to_path_buf(), stable: None, files, message: "RetroArch uses it from the next game you start.".into() }
}

/// The plan for one installation.
fn plan_for(connection: &Connection, located: &installations::Located, vid: &str, pid: &str, xinput: bool) -> Plan {
    if located.presence != Presence::Connected {
        return Plan::Unavailable("Not connected.".into());
    }
    let root_text = located.root.display().to_string();
    match located.adapter_id.as_str() {
        "retrobat" => {
            let stable: Option<String> = connection
                .query_row("SELECT s.stable_id FROM installations i JOIN storage_locations s ON s.id = i.storage_location_id WHERE i.id = ?1", [&located.id], |r| r.get(0))
                .ok()
                .flatten();
            plan_retrobat(&located.root, stable, vid, pid, xinput)
        }
        "retropie" => match retropie_settings::configs_root(&root_text) {
            Some(configs) => plan_retropie(Path::new(&configs), vid, pid),
            None => Plan::Manual("On a RetroPie USB stick, set the controller up on the Pi: press Start → Configure Input, and follow the prompts.".into()),
        },
        "esde" => match bios_step::esde_retroarch(&located.root) {
            Some(ra) => plan_retroarch(&ra, vid, pid, xinput),
            None if xinput => Plan::Ready("Works already: ES-DE sets up Xbox-style controllers by itself.".into()),
            None => Plan::Manual("ES-DE's menus use any controller SDL knows. For games, put RetroArch in ES-DE's Emulators folder first; then RetroDrive can set the controller up there.".into()),
        },
        "batocera" => Plan::Manual("Batocera knows most controllers and sets them up when you plug them in. If yours doesn't work there: press Start → Controller & Bluetooth settings → Controller mapping, and follow the prompts.".into()),
        "recalbox" => Plan::Manual("Recalbox sets most controllers up by itself. If yours doesn't work there: press Start → Controllers settings → Configure a controller, and follow the prompts.".into()),
        "rocknix" => Plan::Manual("ROCKNIX sets most controllers up by itself. If yours doesn't work there, set it up in ROCKNIX's own controller settings menu.".into()),
        other => Plan::Manual(format!("RetroDrive can't set controllers up on {other} yet.")),
    }
}

fn target(located: &installations::Located, plan: &Plan) -> SetupTarget {
    let (status, message, files) = match plan {
        Plan::Ready(m) => ("ready", m.clone(), Vec::new()),
        Plan::Manual(m) => ("manual", m.clone(), Vec::new()),
        Plan::Unavailable(m) => ("unavailable", m.clone(), Vec::new()),
        Plan::Add { files, message, .. } => ("add", message.clone(), files.iter().map(|f| f.0.clone()).collect()),
    };
    SetupTarget { installation_id: located.id.clone(), adapter_id: located.adapter_id.clone(), location: located.root.display().to_string(), status, message, files }
}

fn usb(vid: &str, pid: &str) -> Result<(String, String), String> {
    let ok = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit());
    if !ok(vid) || !ok(pid) {
        return Err("Not a USB ID".into());
    }
    Ok((vid.to_lowercase(), pid.to_lowercase()))
}

/// Each installation's state for this controller (reads only).
pub fn preview(connection: &Connection, vid: &str, pid: &str, xinput: bool, presence: PresenceFn) -> Result<Vec<SetupTarget>, String> {
    let (vid, pid) = usb(vid, pid)?;
    Ok(installations::located(connection, presence, None)?.iter().map(|l| target(l, &plan_for(connection, l, &vid, &pid, xinput))).collect())
}

/// Add the setup files on the chosen installations. Each file is new, or (RetroBat's
/// user gamecontrollerdb.txt) the same file with lines added at the end; journaled
/// and undoable. If one fails, the ones already written are undone.
pub fn apply(connection: &Connection, installation_ids: &[String], vid: &str, pid: &str, xinput: bool, presence: PresenceFn) -> Result<String, String> {
    let (vid, pid) = usb(vid, pid)?;
    let mut ids = Vec::new();
    let mut done = Vec::new();
    for installation_id in installation_ids {
        let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
        let Plan::Add { root, stable, files, message } = plan_for(connection, &located, &vid, &pid, xinput) else { continue };
        let install = Install { id: located.id.clone(), root_text: root.display().to_string(), root, stable };
        for (relative, path, old, new) in files {
            if path.parent().is_some_and(|p| !p.exists()) {
                if let Err(e) = fs::create_dir_all(path.parent().unwrap_or(&path)) {
                    return fail(connection, presence, &ids, format!("Couldn't create {}: {e}.", path.display()));
                }
            }
            let name = relative.rsplit('/').next().unwrap_or(&relative).to_string();
            let row = ChangeRow { system: "controllers".into(), key: name.clone(), before: (!old.is_empty()).then(|| "(existing file)".into()), after: Some(format!("controller {vid}:{pid} added")) };
            match customizer::write_file(connection, &install, &relative, &path, &old, &new, vec![row]) {
                Ok((_, id)) => ids.push(id),
                Err(e) => return fail(connection, presence, &ids, e),
            }
        }
        done.push(format!("{}: {message}", label(&located.adapter_id)));
    }
    if done.is_empty() {
        return Ok("Nothing needed setting up.".into());
    }
    Ok(format!("Set up. {} Undo is under Customize → Show every settings change.", done.join(" ")))
}

/// What Plan and Sync tell the user about controllers for one installation (ADR-065).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InstallationControllers {
    /// Controllers the user confirmed on the Controllers page.
    pub confirmed: usize,
    /// Names of confirmed controllers RetroDrive can set up here now.
    pub to_add: Vec<String>,
    /// Names of confirmed controllers to set up in the system's own menu, and how.
    pub manual: Vec<String>,
    pub manual_message: Option<String>,
}

/// Confirmed controllers' state on one installation (reads only).
pub fn for_installation(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<InstallationControllers, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    let confirmed = crate::controllers::confirmed(connection)?;
    let mut out = InstallationControllers { confirmed: confirmed.len(), to_add: Vec::new(), manual: Vec::new(), manual_message: None };
    for (vid, pid, xinput) in &confirmed {
        match plan_for(connection, &located, vid, pid, *xinput) {
            Plan::Add { .. } => out.to_add.push(crate::controllers::display_name(vid, pid)),
            Plan::Manual(m) => {
                out.manual.push(crate::controllers::display_name(vid, pid));
                out.manual_message.get_or_insert(m);
            }
            Plan::Ready(_) | Plan::Unavailable(_) => {}
        }
    }
    Ok(out)
}

/// After a copy (the user ticked it): set up every confirmed controller RetroDrive can
/// set up on this installation. `None` when there was nothing to do.
pub fn apply_for_installation(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Option<String>, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    let mut names = Vec::new();
    for (vid, pid, xinput) in crate::controllers::confirmed(connection)? {
        if matches!(plan_for(connection, &located, &vid, &pid, xinput), Plan::Add { .. }) {
            apply(connection, std::slice::from_ref(&located.id), &vid, &pid, xinput, presence)?;
            names.push(crate::controllers::display_name(&vid, &pid));
        }
    }
    Ok((!names.is_empty()).then(|| format!("{} set up on {}. Undo is under Customize → Show every settings change.", names.join(", "), label(&located.adapter_id))))
}

fn fail(connection: &Connection, presence: PresenceFn, ids: &[i64], e: String) -> Result<String, String> {
    let undone = ids.iter().rev().all(|id| customizer::undo(connection, *id, presence, &customizer::never_running).is_ok());
    Err(if ids.is_empty() || undone { format!("{e} Nothing was set up.") } else { format!("{e} Some files were added; undo them under \"Show every settings change\".") })
}

fn label(adapter: &str) -> &'static str {
    match adapter {
        "retrobat" => "RetroBat",
        "retropie" => "RetroPie",
        "esde" => "ES-DE",
        "batocera" => "Batocera",
        "recalbox" => "Recalbox",
        "rocknix" => "ROCKNIX",
        _ => "System",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_setups_are_found_by_usb_id() {
        assert_eq!(guid_usb("03000000c82d00000a31000000000000"), Some(("2dc8".into(), "310a".into())));
        assert_eq!(guid_usb("78696e70757401000000000000000000"), None, "XInput-style GUIDs carry no USB ID");
        assert!(!sdl_lines_for("2dc8", "310a").is_empty(), "the owner's 8BitDo Ultimate 2C");
        assert!(sdl_lines_for("abcd", "dcba").is_empty());
        assert!(ra_files().len() > 700, "{}", ra_files().len());
        let udev = ra_for("udev", "2dc8", "310a");
        assert!(udev.iter().any(|f| f.name == "8BitDo_Ultimate_2C_Wireless_Controller_USB.cfg"), "{:?}", udev.iter().map(|f| f.name).collect::<Vec<_>>());
        assert!(udev[0].text.contains("input_vendor_id") && !udev[0].text.contains("@@ "));
        assert!(ra_files().iter().all(|f| !f.name.is_empty() && !f.text.trim().is_empty()));
        assert_eq!(ra_id("\"11720\""), Some("2dc8".into()));
        assert_eq!(ra_id("0x2DC8"), Some("2dc8".into()));
        assert_eq!(ra_id("x"), None);
    }

    #[test]
    fn retrobat_gets_the_sdl_line_only_when_it_needs_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(matches!(plan_retrobat(root, None, "2dc8", "310a", true), Plan::Ready(_)), "Xbox-style: nothing to do");
        assert!(matches!(plan_retrobat(root, None, "2dc8", "310a", false), Plan::Unavailable(_)), "never started");
        fs::create_dir_all(root.join("emulationstation/.emulationstation")).unwrap();
        fs::write(root.join(RETROBAT_USER), "# mine\r\nabc,Custom,a:b0,platform:Windows,").unwrap();
        let Plan::Add { files, .. } = plan_retrobat(root, None, "2dc8", "310a", false) else { panic!("expected add") };
        let (_, _, old, new) = &files[0];
        assert!(new.starts_with(old.as_str()), "the user's lines stay as they are");
        assert!(new.contains("\r\n03000000c82d00000a31000000000000,8BitDo Ultimate 2C,") && new.ends_with("\r\n"));
        // Known in RetroBat's own list, or already in es_input.cfg: nothing to do.
        fs::write(root.join(RETROBAT_SHIPPED), "03000000c82d00000a31000000000000,8BitDo Ultimate 2C,a:b0,platform:Windows,\n").unwrap();
        assert!(matches!(plan_retrobat(root, None, "2dc8", "310a", false), Plan::Ready(_)));
        fs::remove_file(root.join(RETROBAT_SHIPPED)).unwrap();
        fs::write(root.join(RETROBAT_INPUT), "<inputList><inputConfig type=\"joystick\" deviceName=\"x\" deviceGUID=\"03000000c82d00000a31000000000000\"/></inputList>").unwrap();
        assert!(matches!(plan_retrobat(root, None, "2dc8", "310a", false), Plan::Ready(_)));
        // No ready-made setup: steps in RetroBat's menu.
        assert!(matches!(plan_retrobat(root, None, "abcd", "dcba", false), Plan::Manual(_)));
    }

    #[test]
    fn retropie_and_retroarch_get_new_files_and_keep_existing_ones() {
        let dir = tempfile::tempdir().unwrap();
        let configs = dir.path().join("configs");
        assert!(matches!(plan_retropie(&configs, "2dc8", "310a"), Plan::Unavailable(_)));
        let joypads = configs.join("all/retroarch-joypads");
        fs::create_dir_all(&joypads).unwrap();
        let Plan::Add { files, .. } = plan_retropie(&configs, "2dc8", "310a") else { panic!("expected add") };
        assert!(files.iter().all(|(rel, path, old, new)| rel.starts_with("all/retroarch-joypads/") && !path.exists() && old.is_empty() && new.contains("11720")));
        // A setup the user made on the Pi (same USB ID, any file name) is respected.
        fs::write(joypads.join("Mine.cfg"), "input_device = \"Pad\"\ninput_vendor_id = \"11720\"\ninput_product_id = \"12554\"\n").unwrap();
        assert!(matches!(plan_retropie(&configs, "2dc8", "310a"), Plan::Ready(_)));
        assert!(matches!(plan_retropie(&configs, "abcd", "dcba"), Plan::Manual(_)));

        // RetroArch (ES-DE): a dinput file for a controller the bundle has.
        let ra = dir.path().join("RetroArch");
        let sample = ra_files().iter().find(|f| f.driver == "dinput" && f.usb.is_some()).unwrap();
        let (v, p) = sample.usb.clone().unwrap();
        fs::create_dir_all(&ra).unwrap();
        assert!(matches!(plan_retroarch(&ra, &v, &p, false), Plan::Unavailable(_)), "no autoconfig folder yet");
        fs::create_dir_all(ra.join("autoconfig")).unwrap();
        assert!(matches!(plan_retroarch(&ra, &v, &p, true), Plan::Ready(_)), "Xbox-style with the xinput driver");
        let Plan::Add { files, .. } = plan_retroarch(&ra, &v, &p, false) else { panic!("expected add") };
        assert!(files.iter().all(|(rel, ..)| rel.starts_with("autoconfig/dinput/")), "{files:?}");
        fs::write(ra.join("retroarch.cfg"), "input_joypad_driver = \"sdl2\"\njoypad_autoconfig_dir = \":\\autoconfig\"\n").unwrap();
        assert_eq!(retroarch_settings(&ra), (ra.join("autoconfig"), "sdl2".to_string()));
        fs::write(ra.join("retroarch.cfg"), "input_joypad_driver = \"hid\"\n").unwrap();
        assert!(matches!(plan_retroarch(&ra, &v, &p, false), Plan::Manual(_)));
    }

    #[test]
    fn a_setup_is_added_journaled_and_undone() {
        use crate::{db, volumes::VolumeProbe};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        fs::create_dir_all(root.join("emulationstation/.emulationstation")).unwrap();
        fs::create_dir_all(root.join("roms/snes")).unwrap();
        fs::write(root.join("retrobat.exe"), b"x").unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let id = installations::add(&catalog, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let connected = |_: &str, _: Option<&str>| Presence::Connected;
        let before = preview(&catalog, "2DC8", "310A", false, &connected).unwrap();
        assert_eq!((before.len(), before[0].status), (1, "add"), "{before:?}");
        let message = apply(&catalog, std::slice::from_ref(&id), "2dc8", "310a", false, &connected).unwrap();
        assert!(message.contains("RetroBat"), "{message}");
        let user = root.join(RETROBAT_USER);
        assert!(fs::read_to_string(&user).unwrap().contains("8BitDo Ultimate 2C"));
        assert_eq!(preview(&catalog, "2dc8", "310a", false, &connected).unwrap()[0].status, "ready", "a second time: nothing to do");
        assert_eq!(apply(&catalog, std::slice::from_ref(&id), "2dc8", "310a", false, &connected).unwrap(), "Nothing needed setting up.");
        let change: i64 = catalog.query_row("SELECT id FROM settings_changes ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        customizer::undo(&catalog, change, &connected, &customizer::never_running).unwrap();
        assert!(!user.exists(), "undo takes the new file away (into RetroDrive's backup folder)");
        assert!(preview(&catalog, "zz", "310a", false, &connected).is_err());

        // Plan and Sync: nothing confirmed yet → the reminder; confirmed → set up after the copy.
        assert_eq!(for_installation(&catalog, &id, &connected).unwrap().confirmed, 0);
        crate::controllers::remember(&catalog, "2dc8", "310a", Some("CT07"), false).unwrap();
        crate::controllers::remember(&catalog, "abcd", "dcba", Some("CT06"), false).unwrap();
        let summary = for_installation(&catalog, &id, &connected).unwrap();
        assert_eq!((summary.confirmed, summary.to_add.len(), summary.manual.len()), (2, 1, 1), "{summary:?}");
        assert!(summary.to_add[0].contains("8BitDo") && summary.manual_message.unwrap().contains("Configure a controller"));
        let message = apply_for_installation(&catalog, &id, &connected).unwrap().unwrap();
        assert!(message.contains("8BitDo") && message.contains("RetroBat"), "{message}");
        assert!(fs::read_to_string(&user).unwrap().contains("8BitDo Ultimate 2C"));
        assert_eq!(apply_for_installation(&catalog, &id, &connected).unwrap(), None, "nothing left to do");
    }
}


