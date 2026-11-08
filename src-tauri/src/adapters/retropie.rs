//! RetroPie adapter (RDS-023, 2026-11-07): recognition, layout checks, placement and
//! copying (step 2, owner go-ahead 2026-11-07; docs/18 §26) to the RetroPie network share
//! or a USB `retropie`/`retropie-mount` folder, with the same copier as RetroBat.
//! A network share is known by the id RetroDrive leaves on it (ADR-046), since every
//! RetroPie is `RETROPIE` by default.
//!
//! Facts checked 2026-11-07 (RetroPie-Setup and RetroPie-Docs on GitHub):
//! - Games live in `~/RetroPie/roms/<system>` on the Pi's Linux partition, which
//!   Windows can't read. From Windows they're reached through the Samba share
//!   `\\RETROPIE\roms`, whose root *is* the roms folder (`Transferring-Roms.md`).
//! - USB sticks: a `retropie` folder on the stick gets `retropie/roms/<system>` (the
//!   transfer method), and a `retropie-mount` folder is used as storage by the USB ROM
//!   service; both have a `roms` folder inside.
//! - System folder names come from `platforms.cfg`. Most match RetroBat's; RetroPie
//!   uses `atarilynx`, `atarijaguar`, `amiga`, `msx`, `segacd`, `sg-1000`, `coleco`,
//!   `videopac`, `wonderswan`, `wonderswancolor`; PC Engine CD and SuperGrafx go in
//!   `pcengine`. Arcade: lr-fbneo reads `fba` (and `arcade`), lr-mame (current MAME)
//!   reads `mame-libretro` (and `arcade`) (`lr-fbneo.sh`, `lr-mame.sh`).
//! - The version isn't visible from the roms share, so it's reported as unknown.

use super::retrobat::{arcade_folder, system_folder};
use super::{has_entry, plan_in, AdapterKind, ArcadeSystem, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct RetroPie;

const ID: &str = "retropie";
const NAME: &str = "RetroPie";

/// RetroPie system folders (from `platforms.cfg`) that are common enough to count as
/// evidence, and the ones only RetroPie uses (strong evidence).
const SYSTEMS: &[&str] = &[
    "nes", "snes", "megadrive", "mastersystem", "gamegear", "psx", "n64", "gb", "gbc", "gba", "atari2600", "atari7800", "pcengine", "segacd", "sega32x", "neogeo", "ngp", "ngpc", "arcade", "fds", "zxspectrum", "c64", "amiga", "msx", "dreamcast", "psp", "nds", "vectrex", "intellivision",
    "atarilynx", "coleco", "sg-1000", "videopac", "wonderswan", "wonderswancolor", "fba", "mame-libretro", "mame-advmame", "mame-mame4all", "ports", "pc", "scummvm", "kodi",
];
const RETROPIE_ONLY: &[&str] = &["atarilynx", "coleco", "sg-1000", "videopac", "wonderswan", "wonderswancolor", "fba", "mame-libretro", "mame-advmame", "mame-mame4all", "ports", "pc"];

fn system_folders(dir: &Path) -> (usize, bool) {
    let found: Vec<&str> = SYSTEMS.iter().copied().filter(|s| has_entry(dir, s, true)).collect();
    (found.len(), found.iter().any(|s| RETROPIE_ONLY.contains(s)))
}

fn is_network_path(path: &str) -> bool {
    path.starts_with(r"\\") || path.starts_with("//")
}

/// A Raspberry Pi's boot partition (FAT, readable by Windows): never a game folder.
fn is_pi_boot_partition(root: &Path) -> bool {
    has_entry(root, "config.txt", false) && has_entry(root, "cmdline.txt", false) && (has_entry(root, "start.elf", false) || has_entry(root, "start4.elf", false) || has_entry(root, "bootcode.bin", false))
}

/// The folder that holds the system folders: the root itself (the roms share) or its `roms`.
pub(crate) fn games_folder(root: &Path) -> std::path::PathBuf {
    if has_entry(root, "roms", true) {
        root.join("roms")
    } else {
        root.to_path_buf()
    }
}

/// RetroBat/Batocera folder → RetroPie folder (`platforms.cfg`).
fn retropie_folder(platform: &str, extension: &str, request: &PlacementRequest) -> Result<&'static str, String> {
    let folder = match platform {
        "CPS1" | "CPS2" | "CPS3" | "Arcade" => {
            return match (request.options.arcade, arcade_folder(extension, request.options)) {
                (Some(ArcadeSystem::Fbneo), Ok(_)) => Ok("fba"),
                (Some(ArcadeSystem::Mame), Ok(_)) => Ok("mame-libretro"),
                (_, Err(e)) => Err(e),
                (None, Ok(_)) => unreachable!("arcade_folder needs a choice"),
            };
        }
        platform => system_folder(platform, extension, request.options, NAME)?,
    };
    Ok(match folder {
        "lynx" => "atarilynx",
        "jaguar" => "atarijaguar",
        "amiga500" | "amiga1200" => "amiga",
        "msx1" | "msx2" => "msx",
        "pcenginecd" | "supergrafx" => "pcengine",
        "sg1000" => "sg-1000",
        "odyssey2" => "videopac",
        "wswan" => "wonderswan",
        "wswanc" => "wonderswancolor",
        "colecovision" => "coleco",
        "megacd" => "segacd",
        "amigacd32" | "neogeocd" | "gx4000" | "supervision" | "naomi" | "atomiswave" => return Err(format!("RetroPie has no folder for {platform}.")),
        same => same,
    })
}

impl PlatformAdapter for RetroPie {
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
        if is_pi_boot_partition(root) {
            return detection(
                Confidence::Possible,
                vec!["Raspberry Pi boot files found".into()],
                Some("This is a Raspberry Pi's boot partition; games don't go here. Windows can't read RetroPie's game partition, so choose \\\\RETROPIE\\roms over the network while it's running, or the retropie folder on a USB stick.".into()),
            );
        }
        // Never claim RetroBat or Batocera folders.
        if has_entry(root, "RetroBat.exe", false) || (has_entry(root, "system", true) && has_entry(&root.join("system"), "batocera.conf", false)) || super::recalbox::has_recalbox_conf(root) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let folder = games_folder(root);
        // ES-DE creates folders for every system (including RetroPie-style names) and marks them.
        if super::esde::has_esde_markers(&folder) || super::rocknix::has_rocknix_markers(&folder) || super::lakka::has_lakka_markers(root) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let (count, distinctive) = system_folders(&folder);
        let mut evidence = Vec::new();
        if count > 0 {
            evidence.push(format!("{count} RetroPie system folders found"));
        }
        if distinctive {
            evidence.push("folders only RetroPie uses (e.g. ports, coleco, fba)".into());
        }
        if count >= 3 && distinctive {
            return detection(Confidence::Detected, evidence, None);
        }
        if count >= 3 {
            return detection(Confidence::Possible, evidence, Some("This looks like a roms folder, but nothing in it is specific to RetroPie. If it's RetroPie's, add it; RetroDrive will use RetroPie's folder names.".into()));
        }
        for nested in ["retropie", "retropie-mount"] {
            let inner = root.join(nested);
            if has_entry(root, nested, true) && has_entry(&inner, "roms", true) {
                return detection(Confidence::Possible, vec![format!("{nested} folder with roms found")], Some(format!("RetroPie keeps games in the {nested} folder on this drive. Choose {} instead.", inner.display())));
            }
        }
        detection(Confidence::NotDetected, evidence, None)
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let path = root.display().to_string();
        let network = is_network_path(&path);
        let folder = games_folder(root);
        let (count, _) = system_folders(&folder);
        vec![
            LayoutCheck {
                id: "systems",
                label: "System folders".into(),
                status: if count > 0 { CheckStatus::Ok } else { CheckStatus::Problem },
                detail: if count > 0 { format!("Found {count} system folders.") } else { "No system folders found. RetroPie creates them when its emulators are installed.".into() },
            },
            LayoutCheck {
                id: "access",
                label: "Access".into(),
                status: CheckStatus::Ok,
                detail: if network {
                    "Network share. It's only reachable while RetroPie is running and on the network.".into()
                } else {
                    "Folder on a drive Windows can read (for example a USB stick with a retropie folder).".into()
                },
            },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: if network { CheckStatus::Warning } else { CheckStatus::Ok },
                detail: if network {
                    "Copies over the network are new: keep RetroPie on and connected until the copy finishes. Every file is checked after it's written; nothing is overwritten or deleted. Restart RetroPie (or EmulationStation) afterwards to see new games.".into()
                } else {
                    "RetroDrive copies games here, checks every file after it's written, and never overwrites or deletes anything. Plug the stick back into RetroPie afterwards.".into()
                },
            },
            LayoutCheck { id: "version", label: "Version".into(), status: CheckStatus::Warning, detail: "RetroPie's version isn't visible from here; its menu shows it.".into() },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        if request.platform == crate::classify::BIOS {
            // A USB retropie / retropie-mount folder has BIOS beside roms; over the network
            // RetroPie keeps them on its separate bios share (not yet).
            return if request.options.has_roms_folder {
                super::plan_bios("BIOS", request)
            } else {
                Placement::Unsupported { reason: "Over the network, RetroPie keeps BIOS files on its separate bios share: use BIOS files in Customize to copy them there.".into() }
            };
        }
        match retropie_folder(&request.platform, &request.extension, request) {
            Ok(system) => plan_in(if request.options.has_roms_folder { "roms/" } else { "" }, system, request),
            Err(reason) => Placement::Unsupported { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{PlacementFile, PlacementOptions};
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

    fn request(platform: &str, ext: &str, options: PlacementOptions) -> PlacementRequest {
        PlacementRequest { title: "Saga".into(), platform: platform.into(), extension: ext.into(), files: vec![PlacementFile { relative_to_primary: format!("Saga.{ext}"), size: 1 }], options, bios_path: None }
    }

    #[test]
    fn the_roms_share_and_usb_folders_are_recognized() {
        let share = tempfile::tempdir().unwrap();
        make(share.path(), &["nes/", "snes/", "megadrive/", "ports/", "coleco/"]);
        assert_eq!(RetroPie.detect(share.path()).confidence, Confidence::Detected);
        // A plain roms folder (no RetroPie-only names) is only possible.
        let plain = tempfile::tempdir().unwrap();
        make(plain.path(), &["nes/", "snes/", "gba/"]);
        assert_eq!(RetroPie.detect(plain.path()).confidence, Confidence::Possible);
        // USB stick: the retropie folder is pointed at; the folder itself (with roms) is detected.
        let usb = tempfile::tempdir().unwrap();
        make(usb.path(), &["retropie/roms/nes/", "retropie/roms/snes/", "retropie/roms/fba/", "Photos/"]);
        let d = RetroPie.detect(usb.path());
        assert_eq!(d.confidence, Confidence::Possible);
        assert!(d.hint.unwrap().contains("retropie"));
        assert_eq!(RetroPie.detect(&usb.path().join("retropie")).confidence, Confidence::Detected);
        // Pi boot partition, RetroBat and Batocera folders.
        let boot = tempfile::tempdir().unwrap();
        make(boot.path(), &["config.txt", "cmdline.txt", "start4.elf"]);
        assert!(RetroPie.detect(boot.path()).hint.unwrap().contains("boot partition"));
        let bato = tempfile::tempdir().unwrap();
        make(bato.path(), &["roms/nes/", "roms/snes/", "roms/fba/", "roms/ports/", "system/batocera.conf"]);
        assert_eq!(RetroPie.detect(bato.path()).confidence, Confidence::NotDetected);
        assert!(RetroPie.capabilities().copy_games, "copies since its safety review (docs/18 §26)");
    }

    #[test]
    fn placement_uses_retropie_folders_in_either_layout() {
        let flat = PlacementOptions::default();
        let nested = PlacementOptions { has_roms_folder: true, ..PlacementOptions::default() };
        let dest = |p: Placement| match p {
            Placement::Planned { destinations, .. } => destinations[0].clone(),
            Placement::Unsupported { reason } => format!("unsupported: {reason}"),
        };
        assert_eq!(dest(RetroPie.plan_placement(&request("SNES", "sfc", flat))), "snes/Saga.sfc", "the roms share holds the system folders");
        assert_eq!(dest(RetroPie.plan_placement(&request("SNES", "sfc", nested))), "roms/snes/Saga.sfc", "a USB retropie folder has roms inside");
        assert_eq!(dest(RetroPie.plan_placement(&request("Atari Lynx", "lnx", flat))), "atarilynx/Saga.lnx");
        assert_eq!(dest(RetroPie.plan_placement(&request("ColecoVision", "col", flat))), "coleco/Saga.col");
        assert_eq!(dest(RetroPie.plan_placement(&request("Sega CD", "chd", flat))), "segacd/Saga.chd");
        assert_eq!(dest(RetroPie.plan_placement(&request("SG-1000", "sg", flat))), "sg-1000/Saga.sg");
        assert!(dest(RetroPie.plan_placement(&request("Naomi", "zip", flat))).starts_with("unsupported"));
        assert!(dest(RetroPie.plan_placement(&request("Arcade", "zip", flat))).contains("Choose MAME or FinalBurn Neo"));
        let fbneo = PlacementOptions { arcade: Some(ArcadeSystem::Fbneo), ..flat };
        assert_eq!(dest(RetroPie.plan_placement(&request("Arcade", "zip", fbneo))), "fba/Saga.zip");
        assert_eq!(dest(RetroPie.plan_placement(&request("CPS2", "zip", PlacementOptions { arcade: Some(ArcadeSystem::Mame), ..flat }))), "mame-libretro/Saga.zip");
    }
}
