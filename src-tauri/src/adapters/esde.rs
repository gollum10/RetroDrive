//! ES-DE adapter (RDS-025, 2026-11-07): recognition, layout checks, placement and
//! copying (step 2, owner go-ahead 2026-11-07; docs/18 §30). ES-DE's ROMs folder is a
//! normal Windows folder (local, USB or NAS), so the reviewed copier applies as for
//! RetroBat; a NAS path gets the share id (ADR-046).
//!
//! Facts checked 2026-11-07 (ES-DE source and user guide on GitLab):
//! - Games go in a ROMs folder with one folder per system, named after ES-DE's systems
//!   (`resources/systems/windows/es_systems.xml`: `%ROMPATH%\<system>`). The default
//!   ROMs folder is `%USERPROFILE%\ROMs` unless the `ROMDirectory` setting says
//!   otherwise (`FileData::getROMDirectory`, `Settings.cpp`).
//! - When ES-DE creates the folders it writes `systeminfo.txt` into each system folder
//!   and `systems.txt` into the ROMs folder (`SystemData::createSystemDirectories`).
//! - ES-DE's own data (settings, game lists, downloaded media) is in a separate `ES-DE`
//!   folder (`%USERPROFILE%\ES-DE`, or `ES-DE` next to `ES-DE.exe` when portable), with
//!   `settings\es_settings.xml`.
//! - ES-DE launches emulators installed separately, so nothing about emulators is here.

use super::retrobat::system_folder;
use super::{has_entry, plan_in, AdapterKind, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct EsDe;

const ID: &str = "esde";
const NAME: &str = "ES-DE";

/// Common ES-DE system folders (from es_systems.xml), used as evidence.
const SYSTEMS: &[&str] = &[
    "nes", "snes", "megadrive", "genesis", "mastersystem", "gamegear", "psx", "ps2", "psp", "n64", "gb", "gbc", "gba", "nds", "atari2600", "atarilynx", "pcengine", "segacd", "megacd", "sega32x", "neogeo", "arcade", "mame", "fbneo", "dreamcast", "saturn", "c64", "amiga", "msx", "colecovision", "sg-1000", "wonderswan",
];

/// Whether ES-DE created this ROMs folder: its `systems.txt`, or `systeminfo.txt` in system folders.
pub(crate) fn has_esde_markers(roms: &Path) -> bool {
    has_entry(roms, "systems.txt", false) || SYSTEMS.iter().filter(|s| has_entry(&roms.join(s), "systeminfo.txt", false)).take(2).count() >= 2
}

/// The ROMs folder ES-DE's settings point at (`ROMDirectory`), if set.
fn rom_directory_setting(data: &Path) -> Option<String> {
    let text = std::fs::read_to_string(data.join("settings").join("es_settings.xml")).ok()?;
    let at = text.find("name=\"ROMDirectory\"")?;
    let rest = &text[at..text[at..].find("/>").map(|e| at + e).unwrap_or(text.len())];
    let value = rest.split("value=\"").nth(1)?.split('"').next()?.trim();
    (!value.is_empty() && value.len() < 400).then(|| value.to_string())
}

fn is_data_folder(root: &Path) -> bool {
    has_entry(root, "settings", true) && has_entry(&root.join("settings"), "es_settings.xml", false)
}

fn esde_folder(platform: &str, extension: &str, request: &PlacementRequest) -> Result<&'static str, String> {
    let folder = system_folder(platform, extension, request.options, NAME)?;
    Ok(match folder {
        "lynx" => "atarilynx",
        "jaguar" => "atarijaguar",
        "amiga500" => "amiga",
        "sg1000" => "sg-1000",
        "wswan" => "wonderswan",
        "wswanc" => "wonderswancolor",
        same => same,
    })
}

impl PlatformAdapter for EsDe {
    fn id(&self) -> &'static str {
        ID
    }
    fn display_name(&self) -> &'static str {
        NAME
    }
    fn kind(&self) -> AdapterKind {
        AdapterKind::Frontend
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: true, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let detection = |confidence, evidence: Vec<String>, hint: Option<String>| Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::Frontend, confidence, evidence, version: None, hint };
        if has_entry(root, "RetroBat.exe", false) || (has_entry(root, "system", true) && has_entry(&root.join("system"), "batocera.conf", false)) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        // ES-DE's own data folder: point at the ROMs folder.
        if is_data_folder(root) {
            let roms = rom_directory_setting(root).unwrap_or_else(|| std::env::var("USERPROFILE").map(|h| format!("{h}\\ROMs")).unwrap_or_else(|_| "your ROMs folder".into()));
            return detection(Confidence::Possible, vec!["ES-DE settings found".into()], Some(format!("This is ES-DE's settings and media folder; games don't go here. Choose ES-DE's ROMs folder instead: {roms}.")));
        }
        let count = SYSTEMS.iter().filter(|s| has_entry(root, s, true)).count();
        let marked = has_esde_markers(root);
        let mut evidence = Vec::new();
        if count > 0 {
            evidence.push(format!("{count} ES-DE system folders found"));
        }
        if marked {
            evidence.push("ES-DE's systems.txt / systeminfo.txt found".into());
        }
        if marked && count >= 1 {
            return detection(Confidence::Detected, evidence, None);
        }
        if count >= 3 {
            return detection(Confidence::Possible, evidence, Some("This looks like a ROMs folder, but ES-DE's own files (systems.txt, systeminfo.txt) aren't in it. If it's ES-DE's, add it; ES-DE can also create them (Utilities → Create/update system directories).".into()));
        }
        detection(Confidence::NotDetected, evidence, None)
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let count = SYSTEMS.iter().filter(|s| has_entry(root, s, true)).count();
        let marked = has_esde_markers(root);
        vec![
            LayoutCheck {
                id: "systems",
                label: "System folders".into(),
                status: if count > 0 { CheckStatus::Ok } else { CheckStatus::Problem },
                detail: if count > 0 { format!("Found {count} common system folders.") } else { "No system folders found. In ES-DE: Utilities → Create/update system directories.".into() },
            },
            LayoutCheck {
                id: "markers",
                label: "ES-DE files".into(),
                status: if marked { CheckStatus::Ok } else { CheckStatus::Warning },
                detail: if marked { "Found ES-DE's systems.txt / systeminfo.txt.".into() } else { "ES-DE's systems.txt isn't here; ES-DE writes it when it creates the system folders.".into() },
            },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: CheckStatus::Ok,
                detail: "RetroDrive copies games here, checks every file after it's written, and never overwrites or deletes anything. ES-DE finds new games the next time it starts.".into(),
            },
            LayoutCheck { id: "emulators", label: "Emulators".into(), status: CheckStatus::Ok, detail: "ES-DE starts emulators you install separately; RetroDrive only places games.".into() },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        match esde_folder(&request.platform, &request.extension, request) {
            Ok(system) => plan_in("", system, request),
            Err(reason) => Placement::Unsupported { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{ArcadeSystem, PlacementFile, PlacementOptions};
    use std::fs;

    fn make(dir: &Path, entries: &[&str]) {
        for entry in entries {
            let p = dir.join(entry);
            if entry.ends_with('/') {
                fs::create_dir_all(&p).unwrap();
            } else {
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, b"").unwrap();
            }
        }
    }

    #[test]
    fn an_esde_roms_folder_is_recognized_and_other_folders_point_to_it() {
        let roms = tempfile::tempdir().unwrap();
        make(roms.path(), &["systems.txt", "nes/systeminfo.txt", "snes/systeminfo.txt", "ports/", "coleco/"]);
        assert_eq!(EsDe.detect(roms.path()).confidence, Confidence::Detected);
        assert_eq!(crate::adapters::retropie::RetroPie.detect(roms.path()).confidence, Confidence::NotDetected, "RetroPie doesn't claim ES-DE's folder");
        // ES-DE's data folder points at the ROMs folder from its settings.
        let data = tempfile::tempdir().unwrap();
        make(data.path(), &["gamelists/", "downloaded_media/"]);
        fs::create_dir_all(data.path().join("settings")).unwrap();
        fs::write(data.path().join("settings/es_settings.xml"), "<?xml version=\"1.0\"?>\n<string name=\"ROMDirectory\" value=\"D:\\Games\\ROMs\" />\n").unwrap();
        let d = EsDe.detect(data.path());
        assert_eq!(d.confidence, Confidence::Possible);
        assert!(d.hint.unwrap().contains("D:\\Games\\ROMs"));
        // Without ES-DE's files, three system folders are only possible; RetroBat is never claimed.
        let plain = tempfile::tempdir().unwrap();
        make(plain.path(), &["nes/", "snes/", "gba/"]);
        assert_eq!(EsDe.detect(plain.path()).confidence, Confidence::Possible);
        let retrobat = tempfile::tempdir().unwrap();
        make(retrobat.path(), &["RetroBat.exe", "systems.txt", "nes/systeminfo.txt"]);
        assert_eq!(EsDe.detect(retrobat.path()).confidence, Confidence::NotDetected);
        assert!(EsDe.capabilities().copy_games, "copies since the owner's go-ahead (docs/18 §30)");
    }

    #[test]
    fn placement_uses_esde_folder_names() {
        let req = |platform: &str, ext: &str, options: PlacementOptions| PlacementRequest { title: "Saga".into(), platform: platform.into(), extension: ext.into(), files: vec![PlacementFile { relative_to_primary: format!("Saga.{ext}"), size: 1 }], options };
        let dest = |p: Placement| match p {
            Placement::Planned { destinations, .. } => destinations[0].clone(),
            Placement::Unsupported { reason } => format!("unsupported: {reason}"),
        };
        let o = PlacementOptions::default();
        assert_eq!(dest(EsDe.plan_placement(&req("SNES", "sfc", o))), "snes/Saga.sfc", "system folders at the top of the ROMs folder");
        assert_eq!(dest(EsDe.plan_placement(&req("Atari Lynx", "lnx", o))), "atarilynx/Saga.lnx");
        assert_eq!(dest(EsDe.plan_placement(&req("SG-1000", "sg", o))), "sg-1000/Saga.sg");
        assert_eq!(dest(EsDe.plan_placement(&req("WonderSwan Color", "wsc", o))), "wonderswancolor/Saga.wsc");
        assert_eq!(dest(EsDe.plan_placement(&req("ColecoVision", "col", o))), "colecovision/Saga.col");
        let fbneo = PlacementOptions { arcade: Some(ArcadeSystem::Fbneo), ..o };
        assert_eq!(dest(EsDe.plan_placement(&req("Arcade", "zip", fbneo))), "fbneo/Saga.zip");
    }
}
