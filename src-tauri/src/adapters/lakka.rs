//! Lakka adapter (RDS-025, 2026-11-09, ADR-086; owner "Do it all, let's go", docs/18 §54):
//! recognize, check, plan and copy to Lakka's ROMs share with the same verified copier
//! as the other platforms. Playlists and box art: `lakka_playlists`; BIOS: the BIOS step.
//!
//! Facts checked 2026-11-09 in Lakka's source (`libretro/Lakka-LibreELEC`, branch
//! `Lakka-v6.1`):
//! - Samba shares (`distributions/Lakka/config/smb.conf`): `[ROMs]` = /storage/roms,
//!   `[Playlists]` = /storage/playlists, `[Thumbnails]` = /storage/thumbnails,
//!   `[System]` = /tmp/system (an overlay whose writable layer is /storage/system,
//!   `tmp-system.mount`), `[Configfiles]` = /storage/.config, and others. /storage is
//!   ext4, so Windows reaches Lakka only over the network.
//! - RetroArch's settings (`packages/lakka/retroarch_base/retroarch/config/retroarch.cfg`):
//!   `rgui_browser_directory = /storage/roms`, `playlist_directory = /storage/playlists`,
//!   `thumbnails_directory = /storage/thumbnails`, `system_directory = /tmp/system`.
//! - Lakka has no EmulationStation and no required folder names: games show in its
//!   menu through RetroArch playlists. RetroDrive uses its usual short folder names.

use super::retrobat::{arcade_folder, system_folder};
use super::{plan_in, AdapterKind, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::{Path, PathBuf};

pub struct Lakka;

pub const ID: &str = "lakka";
const NAME: &str = "Lakka";

/// One of Lakka's other shares next to its ROMs share: `\\host\ROMs` → `\\host\<name>`
/// (a local folder: its sibling, which is how the tests lay a Lakka out).
pub(crate) fn share(roms: &Path, name: &str) -> Option<PathBuf> {
    let text = roms.display().to_string().replace('/', "\\");
    if let Some(rest) = text.strip_prefix(r"\\") {
        let host = rest.split('\\').next().filter(|h| !h.is_empty() && *h != "?" && *h != ".")?;
        return Some(PathBuf::from(format!(r"\\{host}\{name}")));
    }
    roms.parent().map(|p| p.join(name))
}

fn is_roms_share(root: &Path) -> bool {
    root.display().to_string().trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().is_some_and(|s| s.eq_ignore_ascii_case("ROMs"))
}

fn has_dir(path: Option<PathBuf>) -> bool {
    path.is_some_and(|p| p.is_dir())
}

/// RetroArch's settings on the Configfiles share (Lakka's own marker).
fn retroarch_cfg(root: &Path) -> Option<PathBuf> {
    share(root, "Configfiles").map(|c| c.join("retroarch").join("retroarch.cfg")).filter(|p| p.is_file())
}

/// A Lakka ROMs share: named ROMs, with Lakka's Playlists share and RetroArch's settings next to it.
pub(crate) fn has_lakka_markers(root: &Path) -> bool {
    is_roms_share(root) && has_dir(share(root, "Playlists")) && retroarch_cfg(root).is_some()
}

/// RetroArch's playlist name (its database name) for a RetroDrive system folder;
/// thumbnails are found by this name (`libretro-database/rdb`, checked 2026-11-09).
pub fn playlist_name(system: &str) -> Option<&'static str> {
    Some(match system {
        "nes" => "Nintendo - Nintendo Entertainment System",
        "fds" => "Nintendo - Family Computer Disk System",
        "snes" => "Nintendo - Super Nintendo Entertainment System",
        "n64" => "Nintendo - Nintendo 64",
        "gb" => "Nintendo - Game Boy",
        "gbc" => "Nintendo - Game Boy Color",
        "gba" => "Nintendo - Game Boy Advance",
        "nds" => "Nintendo - Nintendo DS",
        "virtualboy" => "Nintendo - Virtual Boy",
        "pokemini" => "Nintendo - Pokemon Mini",
        "gameandwatch" => "Handheld Electronic Game",
        "megadrive" => "Sega - Mega Drive - Genesis",
        "mastersystem" => "Sega - Master System - Mark III",
        "gamegear" => "Sega - Game Gear",
        "sega32x" => "Sega - 32X",
        "megacd" => "Sega - Mega-CD - Sega CD",
        "saturn" => "Sega - Saturn",
        "dreamcast" => "Sega - Dreamcast",
        "sg1000" => "Sega - SG-1000",
        "naomi" => "Sega - Naomi",
        "atomiswave" => "Atomiswave",
        "psx" => "Sony - PlayStation",
        "ps2" => "Sony - PlayStation 2",
        "psp" => "Sony - PlayStation Portable",
        "pcengine" => "NEC - PC Engine - TurboGrafx 16",
        "pcenginecd" => "NEC - PC Engine CD - TurboGrafx-CD",
        "supergrafx" => "NEC - PC Engine SuperGrafx",
        "atari2600" => "Atari - 2600",
        "atari5200" => "Atari - 5200",
        "atari7800" => "Atari - 7800",
        "atari800" => "Atari - 8-bit Family",
        "lynx" => "Atari - Lynx",
        "jaguar" => "Atari - Jaguar",
        "atarist" => "Atari - ST",
        "c64" => "Commodore - 64",
        "amiga500" | "amiga1200" => "Commodore - Amiga",
        "amigacd32" => "Commodore - CD32",
        "amstradcpc" => "Amstrad - CPC",
        "gx4000" => "Amstrad - GX4000",
        "x68000" => "Sharp - X68000",
        "msx1" => "Microsoft - MSX",
        "msx2" => "Microsoft - MSX2",
        "zx81" => "Sinclair - ZX 81",
        "zxspectrum" => "Sinclair - ZX Spectrum",
        "neogeo" => "SNK - Neo Geo",
        "neogeocd" => "SNK - Neo Geo CD",
        "ngp" => "SNK - Neo Geo Pocket",
        "ngpc" => "SNK - Neo Geo Pocket Color",
        "intellivision" => "Mattel - Intellivision",
        "odyssey2" => "Magnavox - Odyssey2",
        "wswan" => "Bandai - WonderSwan",
        "wswanc" => "Bandai - WonderSwan Color",
        "colecovision" => "Coleco - ColecoVision",
        "vectrex" => "GCE - Vectrex",
        "supervision" => "Watara - Supervision",
        "3do" => "The 3DO Company - 3DO",
        "mame" => "MAME",
        "fbneo" | "cps1" | "cps2" | "cps3" => "FBNeo - Arcade Games",
        _ => return None,
    })
}

impl PlatformAdapter for Lakka {
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
        // Copying with the owner's go-ahead (ADR-086).
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: true, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let detection = |confidence, evidence: Vec<String>, hint: Option<String>| Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::BootableOs, confidence, evidence, version: None, hint };
        if !is_roms_share(root) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let mut evidence = vec!["a share called ROMs".to_string()];
        let playlists = has_dir(share(root, "Playlists"));
        if playlists {
            evidence.push("Lakka's Playlists share next to it".into());
        }
        let cfg = retroarch_cfg(root).is_some();
        if cfg {
            evidence.push("RetroArch's settings on the Configfiles share".into());
        }
        match (playlists, cfg) {
            (true, true) => detection(Confidence::Detected, evidence, None),
            (true, false) => detection(Confidence::Possible, evidence, Some("This looks like Lakka's ROMs share. If it is, add it; RetroDrive will add copied games to Lakka's playlists.".into())),
            _ => detection(Confidence::NotDetected, evidence, None),
        }
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let check = |id: &'static str, label: &str, share_name: &str, missing: CheckStatus, why: &str| {
            let ok = has_dir(share(root, share_name));
            LayoutCheck { id, label: label.into(), status: if ok { CheckStatus::Ok } else { missing }, detail: if ok { format!("{share_name} share found") } else { why.into() } }
        };
        vec![
            check("playlists", "Playlists share", "Playlists", CheckStatus::Problem, "Lakka's Playlists share can't be reached, so copied games wouldn't show in its menu. Check that Lakka is on and Samba is switched on (Settings → Services)."),
            check("thumbnails", "Thumbnails share", "Thumbnails", CheckStatus::Warning, "Lakka's Thumbnails share can't be reached, so box art can't be added."),
            check("bios", "System share (BIOS)", "System", CheckStatus::Warning, "Lakka's System share can't be reached, so BIOS files can't be added."),
            LayoutCheck { id: "access", label: "Access".into(), status: CheckStatus::Ok, detail: "Network share (\\\\LAKKA\\ROMs). It's only reachable while Lakka is on and Samba is switched on in Settings → Services.".into() },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: CheckStatus::Warning,
                detail: "Copies over the network: keep Lakka on and connected until the copy finishes. Every file is checked after it's written; nothing is overwritten or deleted. Copied games are added to Lakka's playlists; on first start Lakka asks which core to use.".into(),
            },
            LayoutCheck { id: "version", label: "Version".into(), status: CheckStatus::Warning, detail: "Lakka's version isn't visible from here; its Information menu shows it.".into() },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        if request.platform == crate::classify::BIOS {
            return Placement::Unsupported { reason: "Lakka keeps BIOS files on its System share; add them with the BIOS step in Customize.".into() };
        }
        let folder = if request.platform == "Arcade" { arcade_folder(&request.extension, request.options) } else { system_folder(&request.platform, &request.extension, request.options, NAME) };
        match folder {
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

    /// A Lakka laid out like its shares: ROMs, Playlists, Configfiles\retroarch\retroarch.cfg.
    pub(crate) fn lakka(dir: &Path) -> PathBuf {
        for d in ["ROMs", "Playlists", "Thumbnails", "System", "Configfiles/retroarch"] {
            fs::create_dir_all(dir.join(d)).unwrap();
        }
        fs::write(dir.join("Configfiles/retroarch/retroarch.cfg"), "playlist_directory = \"/storage/playlists\"\n").unwrap();
        dir.join("ROMs")
    }

    #[test]
    fn lakkas_roms_share_is_recognized_by_its_neighbours() {
        assert_eq!(share(Path::new(r"\\LAKKA\ROMs"), "Playlists"), Some(PathBuf::from(r"\\LAKKA\Playlists")));
        assert_eq!(share(Path::new(r"\\192.168.1.9\ROMs\"), "System"), Some(PathBuf::from(r"\\192.168.1.9\System")));
        let dir = tempfile::tempdir().unwrap();
        let roms = lakka(dir.path());
        assert_eq!(Lakka.detect(&roms).confidence, Confidence::Detected);
        assert!(Lakka.validate_layout(&roms).iter().all(|c| c.status != CheckStatus::Problem));
        // Not Lakka: another folder name, or no RetroArch settings.
        let other = tempfile::tempdir().unwrap();
        fs::create_dir_all(other.path().join("roms/snes")).unwrap();
        fs::create_dir_all(other.path().join("Playlists")).unwrap();
        assert_eq!(Lakka.detect(&other.path().join("roms/snes")).confidence, Confidence::NotDetected);
        assert_eq!(Lakka.detect(&other.path().join("roms")).confidence, Confidence::Possible, "Playlists but no RetroArch settings");
        let plain = tempfile::tempdir().unwrap();
        fs::create_dir_all(plain.path().join("ROMs/nes")).unwrap();
        assert_eq!(Lakka.detect(&plain.path().join("ROMs")).confidence, Confidence::NotDetected);
        assert!(Lakka.capabilities().copy_games);
        assert_eq!(playlist_name("snes"), Some("Nintendo - Super Nintendo Entertainment System"));
        assert_eq!(playlist_name("atari800"), Some("Atari - 8-bit Family"));
        assert_eq!(playlist_name("nope"), None);
    }

    #[test]
    fn placement_uses_the_usual_folder_names_at_the_share_root() {
        let req = |platform: &str, ext: &str, options: PlacementOptions| PlacementRequest { title: "Saga".into(), platform: platform.into(), extension: ext.into(), files: vec![PlacementFile { relative_to_primary: format!("Saga.{ext}"), size: 1 }], options, bios_path: None };
        let dest = |p: Placement| match p {
            Placement::Planned { destinations, .. } => destinations[0].clone(),
            Placement::Unsupported { reason } => format!("unsupported: {reason}"),
        };
        let o = PlacementOptions::default();
        assert_eq!(dest(Lakka.plan_placement(&req("SNES", "sfc", o))), "snes/Saga.sfc");
        assert_eq!(dest(Lakka.plan_placement(&req("Genesis", "md", o))), "megadrive/Saga.md");
        assert!(dest(Lakka.plan_placement(&req("Arcade", "zip", o))).starts_with("unsupported"));
        assert_eq!(dest(Lakka.plan_placement(&req("Arcade", "zip", PlacementOptions { arcade: Some(ArcadeSystem::Fbneo), ..o }))), "fbneo/Saga.zip");
        let mut bios = req("BIOS", "bin", o);
        bios.bios_path = Some("scph1001.bin".into());
        assert!(dest(Lakka.plan_placement(&bios)).contains("BIOS step"), "BIOS go to the System share");
    }
}
