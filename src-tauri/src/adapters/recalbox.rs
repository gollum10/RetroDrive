//! Recalbox adapter (RDS-025, 2026-11-07): recognition, layout checks, placement and
//! copying (step 5, owner go-ahead "go Recalbox" 2026-11-07; docs/18 §34) with the
//! same copier as Batocera; `\RECALBOX\share` gets the share id (ADR-046).
//!
//! Facts checked 2026-11-07 (Recalbox source on GitLab, `recalbox/recalbox`):
//! - Recalbox's games live in `/recalbox/share` (`roms/<system>`, `bios`, `saves`,
//!   `system/recalbox.conf`). Windows reaches it as the Samba share `\\RECALBOX\share`
//!   (`board/recalbox/fsoverlay/etc/samba/smb.conf`: `[share] path = /recalbox/share`).
//! - When an external drive is chosen as storage, Recalbox uses a `recalbox` folder at
//!   that drive's root as the share (`S11share`: `mount "${RECALBOXFULLFS}/recalbox"
//!   "/recalbox/share"`). The SD card's own SHARE partition is exFAT, so Windows can
//!   read it too.
//! - The boot partition has `recalbox-boot.conf`; games never go there.
//! - System folder names come from `package/recalbox-romfs2/systems`. Most match
//!   RetroBat's; Recalbox uses `amiga600` (Amiga 500 games), `o2em` (Odyssey 2),
//!   `segacd`, `gw` (Game & Watch), and has no CPS folders (CPS games are arcade sets,
//!   placed by the arcade choice: `mame` or `fbneo`).
//! - The version isn't in the share, so it's reported as unknown.

use super::retrobat::{arcade_folder, system_folder};
use super::{has_entry, plan_bios, plan_in_roms, AdapterKind, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct Recalbox;

const ID: &str = "recalbox";
const NAME: &str = "Recalbox";

/// `system\recalbox.conf`: the mark of a Recalbox share (other adapters step aside for it).
pub(crate) fn has_recalbox_conf(root: &Path) -> bool {
    has_entry(root, "system", true) && has_entry(&root.join("system"), "recalbox.conf", false)
}

fn is_boot_partition(root: &Path) -> bool {
    has_entry(root, "recalbox-boot.conf", false)
}

fn is_network_path(path: &str) -> bool {
    path.starts_with(r"\\") || path.starts_with("//")
}

fn recalbox_folder(platform: &str, extension: &str, request: &PlacementRequest) -> Result<&'static str, String> {
    let folder = match platform {
        // No CPS folders on Recalbox: CPS games are arcade romsets.
        "CPS1" | "CPS2" | "CPS3" => arcade_folder(extension, request.options)?,
        platform => system_folder(platform, extension, request.options, NAME)?,
    };
    Ok(match folder {
        "amiga500" => "amiga600",
        "odyssey2" => "o2em",
        "megacd" => "segacd",
        "gameandwatch" => "gw",
        same => same,
    })
}

impl PlatformAdapter for Recalbox {
    fn id(&self) -> &'static str {
        ID
    }
    fn display_name(&self) -> &'static str {
        NAME
    }
    fn kind(&self) -> AdapterKind {
        AdapterKind::BootableOs
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: true, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let detection = |confidence, evidence: Vec<String>, hint: Option<String>| Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::BootableOs, confidence, evidence, version: None, hint };
        if is_boot_partition(root) {
            return detection(
                Confidence::Possible,
                vec!["recalbox-boot.conf found".into()],
                Some("This is Recalbox's boot partition; games don't go here. Choose Recalbox's share instead: \\\\RECALBOX\\share over the network, the recalbox folder on an external drive, or the SHARE partition of its SD card.".into()),
            );
        }
        if has_entry(root, "RetroBat.exe", false) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let roms = has_entry(root, "roms", true);
        let conf = has_recalbox_conf(root);
        let mut evidence = Vec::new();
        if roms {
            evidence.push("roms folder found".to_string());
        }
        if conf {
            evidence.push("system\\recalbox.conf found".to_string());
        }
        if roms && conf {
            return detection(Confidence::Detected, evidence, None);
        }
        // An external drive: Recalbox keeps everything in <drive>\recalbox.
        let nested = root.join("recalbox");
        if has_entry(root, "recalbox", true) && (has_entry(&nested, "roms", true) || has_recalbox_conf(&nested)) {
            return detection(Confidence::Possible, vec!["recalbox folder found".into()], Some(format!("Recalbox keeps its games in the recalbox folder on this drive. Choose {} instead.", nested.display())));
        }
        detection(Confidence::NotDetected, evidence, None)
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let check = |id: &'static str, label: &str, ok: bool, missing: CheckStatus, found: &str, why: &str| LayoutCheck { id, label: label.into(), status: if ok { CheckStatus::Ok } else { missing }, detail: if ok { found.into() } else { why.into() } };
        let network = is_network_path(&root.display().to_string());
        vec![
            check("roms", "roms folder", has_entry(root, "roms", true), CheckStatus::Problem, "Found roms\\", "roms\\ is missing. Start Recalbox once so it creates its folders."),
            check("bios", "bios folder", has_entry(root, "bios", true), CheckStatus::Warning, "Found bios\\", "bios\\ is missing. Some systems need BIOS files here."),
            check("config", "Recalbox settings", has_recalbox_conf(root), CheckStatus::Warning, "Found system\\recalbox.conf", "system\\recalbox.conf is missing; Recalbox creates it when it starts."),
            LayoutCheck {
                id: "access",
                label: "Access".into(),
                status: CheckStatus::Ok,
                detail: if network { "Network share. It's only reachable while Recalbox is running and on the network.".into() } else { "Folder on a drive Windows can read (an external drive's recalbox folder, or the SD card's SHARE partition).".into() },
            },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: if network { CheckStatus::Warning } else { CheckStatus::Ok },
                detail: if network {
                    "Copies over the network are new: keep Recalbox on and connected until the copy finishes. Every file is checked after it's written; nothing is overwritten or deleted. Update the game lists in Recalbox (or restart it) to see new games.".into()
                } else {
                    "RetroDrive copies games here, checks every file after it's written, and never overwrites or deletes anything.".into()
                },
            },
            LayoutCheck { id: "version", label: "Version".into(), status: CheckStatus::Warning, detail: "Recalbox's version isn't kept in the share; its menu shows it.".into() },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        if request.platform == crate::classify::BIOS {
            return plan_bios("bios", request);
        }
        match recalbox_folder(&request.platform, &request.extension, request) {
            Ok(system) => plan_in_roms(system, request),
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

    const SHARE: &[&str] = &["roms/nes/", "roms/snes/", "roms/fba/", "roms/megadrive/", "bios/", "saves/", "screenshots/", "music/", "system/recalbox.conf"];

    #[test]
    fn a_recalbox_share_is_recognized_and_other_adapters_step_aside() {
        let share = tempfile::tempdir().unwrap();
        make(share.path(), SHARE);
        assert_eq!(Recalbox.detect(share.path()).confidence, Confidence::Detected);
        assert_eq!(crate::adapters::batocera::Batocera.detect(share.path()).confidence, Confidence::NotDetected, "not Batocera");
        assert_eq!(crate::adapters::retropie::RetroPie.detect(share.path()).confidence, Confidence::NotDetected, "not RetroPie (fba, megadrive…)");
        assert_eq!(crate::adapters::esde::EsDe.detect(share.path()).confidence, Confidence::NotDetected, "not ES-DE");
        // External drive root points at <drive>\recalbox; the boot partition explains.
        let drive = tempfile::tempdir().unwrap();
        make(drive.path(), &["recalbox/roms/nes/", "recalbox/system/recalbox.conf", "Movies/"]);
        let d = Recalbox.detect(drive.path());
        assert_eq!(d.confidence, Confidence::Possible);
        assert!(d.hint.unwrap().contains("recalbox"));
        let boot = tempfile::tempdir().unwrap();
        make(boot.path(), &["recalbox-boot.conf", "boot/"]);
        assert!(Recalbox.detect(boot.path()).hint.unwrap().contains("boot partition"));
        assert!(Recalbox.validate_layout(share.path()).iter().all(|c| c.status != CheckStatus::Problem));
        assert!(Recalbox.capabilities().copy_games, "copies since the owner's go-ahead (docs/18 §34)");
    }

    #[test]
    fn placement_uses_recalbox_folder_names() {
        let req = |platform: &str, ext: &str, options: PlacementOptions| PlacementRequest { title: "Saga".into(), platform: platform.into(), extension: ext.into(), files: vec![PlacementFile { relative_to_primary: format!("Saga.{ext}"), size: 1 }], options, bios_path: None };
        let dest = |p: Placement| match p {
            Placement::Planned { destinations, .. } => destinations[0].clone(),
            Placement::Unsupported { reason } => format!("unsupported: {reason}"),
        };
        let o = PlacementOptions::default();
        assert_eq!(dest(Recalbox.plan_placement(&req("SNES", "sfc", o))), "roms/snes/Saga.sfc");
        assert_eq!(dest(Recalbox.plan_placement(&req("Amiga", "adf", o))), "roms/amiga600/Saga.adf");
        assert_eq!(dest(Recalbox.plan_placement(&req("Odyssey 2", "bin", o))), "roms/o2em/Saga.bin");
        assert_eq!(dest(Recalbox.plan_placement(&req("Sega CD", "chd", o))), "roms/segacd/Saga.chd");
        assert_eq!(dest(Recalbox.plan_placement(&req("Game & Watch", "mgw", o))), "roms/gw/Saga.mgw");
        assert_eq!(dest(Recalbox.plan_placement(&req("Atari Lynx", "lnx", o))), "roms/lynx/Saga.lnx");
        assert!(dest(Recalbox.plan_placement(&req("CPS2", "zip", o))).starts_with("unsupported"), "CPS follows the arcade choice");
        assert_eq!(dest(Recalbox.plan_placement(&req("CPS2", "zip", PlacementOptions { arcade: Some(ArcadeSystem::Fbneo), ..o }))), "roms/fbneo/Saga.zip");
        let mut bios = req("BIOS", "bin", o);
        bios.bios_path = Some("scph1001.bin".into());
        assert_eq!(dest(Recalbox.plan_placement(&bios)), "bios/scph1001.bin");
    }
}
