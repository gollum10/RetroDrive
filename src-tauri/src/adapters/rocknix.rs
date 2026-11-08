//! ROCKNIX adapter (RDS-025, 2026-11-08: recognize, check, plan, ADR-076; copying since
//! the owner's go-ahead "switch on copying to Rocknix", ADR-077, docs/18 §47) with the same
//! verified copier as the other platforms; `\\ROCKNIX\games-roms` gets the share id (ADR-046).
//!
//! Facts checked 2026-11-07/08 in ROCKNIX's source (`ROCKNIX/distribution`, branch `next`):
//! - The Samba share `[games-roms]` is `/storage/roms` (`projects/ROCKNIX/packages/network/
//!   samba/config/smb.conf`), so over the network the system folders sit at the top of
//!   `\\ROCKNIX\games-roms` (like RetroPie's roms share). `games-internal` /
//!   `games-external` hold a `roms` folder too.
//! - `/storage/roms` also gets `bios`, `bezels`, `music`, `savestates` and `themes`
//!   (`add_system_dir` in `projects/ROCKNIX/packages/virtual/emulators/package.mk`).
//! - System folder names are the `add_es_system` names in that file. Most match
//!   RetroBat's; ROCKNIX uses `atarilynx`, `atarijaguar`, `sg-1000`, `wonderswan`,
//!   `wonderswancolor`, `fbn` (FinalBurn Neo), `neocd`, `msx` and `amiga`.
//! - Samba is switched on in ROCKNIX's Network Settings (rocknix.org, "Add games").

use super::retrobat::{arcade_folder, system_folder};
use super::{has_entry, plan_bios, plan_in, AdapterKind, ArcadeSystem, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct Rocknix;

const ID: &str = "rocknix";
const NAME: &str = "ROCKNIX";

/// ROCKNIX's system folders (`add_es_system` in its emulators package).
pub(crate) const SYSTEMS: &[&str] = &[
    "3do", "3ds", "amiga", "amigacd32", "amstradcpc", "arcade", "arduboy", "atari2600", "atari5200", "atari7800", "atari800", "atarijaguar", "atarilynx", "atarist", "atomiswave", "bbcmicro", "bk", "c128", "c16", "c64", "cdi", "channelf",
    "chip-8", "colecovision", "cps1", "cps2", "cps3", "daphne", "doom", "dreamcast", "easyrpg", "famicom", "fbn", "fds", "gameandwatch", "gamecube", "gamegear", "gb", "gba", "gbah", "gbav", "gbc", "gbch", "gbh", "genesis", "genh",
    "ggh", "gp32", "intellivision", "j2me", "macintosh", "mame", "mastersystem", "megacd", "megadrive", "megadrive-japan", "megadriveh", "megaduck", "moto", "msx", "msx2", "n64", "n64dd", "naomi", "nds", "neocd", "neogeo", "nes",
    "nesh", "ngp", "ngpc", "odyssey2", "openbor", "pc", "pc-8800", "pc-9800", "pcengine", "pcenginecd", "pcfx", "pet", "pico-8", "pokemini", "ports", "ps2", "ps3", "psp", "psx", "satellaview", "saturn", "scummvm", "scv", "sega32x",
    "segacd", "segamodel3", "sfc", "sg-1000", "snes", "snesh", "snesmsu1", "st-v", "sufami", "supergrafx", "supervision", "tg16", "tg16cd", "tic-80", "triforce", "uzebox", "vectrex", "vic20", "videopac", "virtualboy", "wii", "wiiu",
    "wonderswan", "wonderswancolor", "x1", "x68000", "xbox", "zmachine", "zx81", "zxspectrum",
];
/// Folders only ROCKNIX (and its JELOS family) uses, next to the systems.
const ROCKNIX_ONLY: &[&str] = &["savestates", "bezels", "fbn", "neocd", "sfc", "genh", "gbh", "snesh", "nesh", "megadriveh", "tg16"];

fn is_network_path(path: &str) -> bool {
    path.starts_with(r"\\") || path.starts_with("//")
}

/// The folder that holds the system folders: the root (the `games-roms` share) or its `roms`.
pub(crate) fn games_folder(root: &Path) -> std::path::PathBuf {
    if has_entry(root, "roms", true) {
        root.join("roms")
    } else {
        root.to_path_buf()
    }
}

/// How many of ROCKNIX's own folders (savestates, bezels, fbn, sfc…) are there.
fn rocknix_marks(folder: &Path) -> Vec<&'static str> {
    ROCKNIX_ONLY.iter().copied().filter(|f| has_entry(folder, f, true)).collect()
}

/// A ROCKNIX roms folder (other adapters step aside for it).
pub(crate) fn has_rocknix_markers(folder: &Path) -> bool {
    let marks = rocknix_marks(folder);
    marks.contains(&"savestates") && marks.contains(&"bezels") || marks.len() >= 3
}

fn is_games_roms_share(root: &Path) -> bool {
    let text = root.display().to_string();
    is_network_path(&text) && text.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().is_some_and(|share| share.eq_ignore_ascii_case("games-roms"))
}

fn rocknix_folder(platform: &str, extension: &str, request: &PlacementRequest) -> Result<&'static str, String> {
    let folder = system_folder(platform, extension, request.options, NAME)?;
    let folder = match folder {
        "lynx" => "atarilynx",
        "jaguar" => "atarijaguar",
        "sg1000" => "sg-1000",
        "wswan" => "wonderswan",
        "wswanc" => "wonderswancolor",
        "fbneo" => "fbn",
        "neogeocd" => "neocd",
        "msx1" => "msx",
        "amiga500" | "amiga1200" => "amiga",
        same => same,
    };
    // The arcade choice: MAME → mame, FinalBurn Neo → fbn (zip/7z sets).
    if platform == "Arcade" {
        let _ = arcade_folder(extension, request.options)?;
        return Ok(if request.options.arcade == Some(ArcadeSystem::Fbneo) { "fbn" } else { "mame" });
    }
    SYSTEMS.iter().copied().find(|s| *s == folder).ok_or_else(|| format!("ROCKNIX has no folder for {platform} games."))
}

impl PlatformAdapter for Rocknix {
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
        // Copying since the owner's go-ahead (ADR-077).
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: true, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let detection = |confidence, evidence: Vec<String>, hint: Option<String>| Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::BootableOs, confidence, evidence, version: None, hint };
        if has_entry(root, "RetroBat.exe", false) || super::recalbox::has_recalbox_conf(root) || (has_entry(root, "system", true) && has_entry(&root.join("system"), "batocera.conf", false)) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let folder = games_folder(root);
        // ES-DE writes systeminfo.txt into its system folders (and portable ES-DE's
        // ROMs_ALL has one for every system, ROCKNIX's names among them): not ROCKNIX.
        if super::esde::has_esde_markers(&folder) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let systems = SYSTEMS.iter().filter(|s| has_entry(&folder, s, true)).count();
        let marks = rocknix_marks(&folder);
        let mut evidence = Vec::new();
        if is_games_roms_share(root) {
            evidence.push("ROCKNIX's games-roms share".to_string());
        }
        if systems > 0 {
            evidence.push(format!("{systems} ROCKNIX system folders found"));
        }
        if !marks.is_empty() {
            evidence.push(format!("folders ROCKNIX uses: {}", marks.join(", ")));
        }
        if (is_games_roms_share(root) && systems >= 1) || (has_rocknix_markers(&folder) && systems >= 3) {
            return detection(Confidence::Detected, evidence, None);
        }
        if systems >= 3 && !marks.is_empty() {
            return detection(Confidence::Possible, evidence, Some("This looks like a ROCKNIX roms folder. If it is, add it; RetroDrive will use ROCKNIX's folder names.".into()));
        }
        detection(Confidence::NotDetected, evidence, None)
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let folder = games_folder(root);
        let network = is_network_path(&root.display().to_string());
        let systems = SYSTEMS.iter().filter(|s| has_entry(&folder, s, true)).count();
        let check = |id: &'static str, label: &str, ok: bool, missing: CheckStatus, found: String, why: &str| LayoutCheck { id, label: label.into(), status: if ok { CheckStatus::Ok } else { missing }, detail: if ok { found } else { why.into() } };
        vec![
            check("systems", "System folders", systems > 0, CheckStatus::Problem, format!("{systems} system folders found"), "No ROCKNIX system folders found. Start ROCKNIX once so it creates them."),
            check("bios", "bios folder", has_entry(&folder, "bios", true), CheckStatus::Warning, "Found bios\\".into(), "bios\\ is missing. Some systems need BIOS files here."),
            LayoutCheck {
                id: "access",
                label: "Access".into(),
                status: CheckStatus::Ok,
                detail: if network { "Network share (\\\\ROCKNIX\\games-roms). It's only reachable while ROCKNIX is on and Samba is switched on in its Network Settings.".into() } else { "Folder Windows can read (e.g. an SD card's roms folder).".into() },
            },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: if network { CheckStatus::Warning } else { CheckStatus::Ok },
                detail: if network {
                    "Copies over the network: keep ROCKNIX on and connected until the copy finishes. Every file is checked after it's written; nothing is overwritten or deleted. Restart ROCKNIX's menu (or the device) to see new games.".into()
                } else {
                    "RetroDrive copies games here, checks every file after it's written, and never overwrites or deletes anything.".into()
                },
            },
            LayoutCheck { id: "version", label: "Version".into(), status: CheckStatus::Warning, detail: "ROCKNIX's version isn't visible from here; its menu shows it.".into() },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        let prefix = if request.options.has_roms_folder { "roms/" } else { "" };
        if request.platform == crate::classify::BIOS {
            return plan_bios(&format!("{prefix}bios"), request);
        }
        match rocknix_folder(&request.platform, &request.extension, request) {
            Ok(system) => plan_in(prefix, system, request),
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
            fs::create_dir_all(dir.join(entry)).unwrap();
        }
    }

    const ROMS: &[&str] = &["nes", "snes", "sfc", "megadrive", "genesis", "psx", "fbn", "mame", "atarilynx", "bios", "bezels", "savestates", "music", "themes"];

    #[test]
    fn a_rocknix_roms_folder_is_recognized_and_retropie_steps_aside() {
        let roms = tempfile::tempdir().unwrap();
        make(roms.path(), ROMS);
        assert_eq!(Rocknix.detect(roms.path()).confidence, Confidence::Detected);
        assert_eq!(crate::adapters::retropie::RetroPie.detect(roms.path()).confidence, Confidence::NotDetected, "not RetroPie (atarilynx…)");
        // An SD card's games-internal folder: roms inside.
        let card = tempfile::tempdir().unwrap();
        make(&card.path().join("roms"), ROMS);
        assert_eq!(Rocknix.detect(card.path()).confidence, Confidence::Detected);
        // A plain RetroPie roms share isn't ROCKNIX.
        let pie = tempfile::tempdir().unwrap();
        make(pie.path(), &["nes", "snes", "megadrive", "ports", "coleco", "fba"]);
        assert_eq!(Rocknix.detect(pie.path()).confidence, Confidence::NotDetected);
        // Portable ES-DE's ROMs_ALL (an empty folder with systeminfo.txt for every system,
        // sfc and tg16 included) isn't ROCKNIX (found on the owner's G:\ES-DE).
        let all = tempfile::tempdir().unwrap();
        make(all.path(), ROMS);
        make(all.path(), &["tg16"]);
        for system in ["nes", "snes", "sfc", "tg16"] {
            fs::write(all.path().join(system).join("systeminfo.txt"), "x").unwrap();
        }
        assert_eq!(Rocknix.detect(all.path()).confidence, Confidence::NotDetected);
        assert!(Rocknix.capabilities().copy_games, "copies since the owner's go-ahead (docs/18 §47)");
        assert!(Rocknix.validate_layout(roms.path()).iter().all(|c| c.status != CheckStatus::Problem));
    }

    #[test]
    fn placement_uses_rocknix_folder_names() {
        let req = |platform: &str, ext: &str, options: PlacementOptions| PlacementRequest { title: "Saga".into(), platform: platform.into(), extension: ext.into(), files: vec![PlacementFile { relative_to_primary: format!("Saga.{ext}"), size: 1 }], options, bios_path: None };
        let dest = |p: Placement| match p {
            Placement::Planned { destinations, .. } => destinations[0].clone(),
            Placement::Unsupported { reason } => format!("unsupported: {reason}"),
        };
        let o = PlacementOptions::default();
        assert_eq!(dest(Rocknix.plan_placement(&req("SNES", "sfc", o))), "snes/Saga.sfc", "the share root is the roms folder");
        assert_eq!(dest(Rocknix.plan_placement(&req("SNES", "sfc", PlacementOptions { has_roms_folder: true, ..o }))), "roms/snes/Saga.sfc");
        assert_eq!(dest(Rocknix.plan_placement(&req("Atari Lynx", "lnx", o))), "atarilynx/Saga.lnx");
        assert_eq!(dest(Rocknix.plan_placement(&req("SG-1000", "sg", o))), "sg-1000/Saga.sg");
        assert_eq!(dest(Rocknix.plan_placement(&req("WonderSwan Color", "wsc", o))), "wonderswancolor/Saga.wsc");
        assert_eq!(dest(Rocknix.plan_placement(&req("Neo Geo CD", "chd", o))), "neocd/Saga.chd");
        assert_eq!(dest(Rocknix.plan_placement(&req("MSX", "rom", o))), "msx/Saga.rom");
        assert!(dest(Rocknix.plan_placement(&req("Arcade", "zip", o))).starts_with("unsupported"), "arcade needs the MAME/FBNeo choice");
        assert_eq!(dest(Rocknix.plan_placement(&req("Arcade", "zip", PlacementOptions { arcade: Some(ArcadeSystem::Fbneo), ..o }))), "fbn/Saga.zip");
        assert_eq!(dest(Rocknix.plan_placement(&req("Arcade", "zip", PlacementOptions { arcade: Some(ArcadeSystem::Mame), ..o }))), "mame/Saga.zip");
        assert!(dest(Rocknix.plan_placement(&req("Amstrad GX4000", "cpr", o))).contains("no folder"), "no guessing");
        let mut bios = req("BIOS", "bin", o);
        bios.bios_path = Some("scph1001.bin".into());
        assert_eq!(dest(Rocknix.plan_placement(&bios)), "bios/scph1001.bin");
    }
}


