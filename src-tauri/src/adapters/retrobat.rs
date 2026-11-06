//! RetroBat adapter (RDS-007): read-only recognition, layout checks and placement plans.
//!
//! Facts verified 2026-11-04 against the RetroBat wiki:
//! - Folder structure (https://wiki.retrobat.org/get-started/retrobat-folder-structure):
//!   `RetroBat.exe`, `retrobat.ini`, `emulationstation\`, `emulators\`, `roms\`
//!   (one folder per system), `bios\`, `saves\`, `system\` (replaced on update).
//!   No version file is documented. Observed on a real 8.3.1.1 install (2026-11-05):
//!   `system\version.info` and `emulationstation\version.info` both contain
//!   `8.3.1.1-stable-win64` (`RetroBat.exe` itself still says 8.3.0.0), and `system\`
//!   is replaced on every update, so RetroDrive reads that file. `retrobat.ini` doesn't
//!   exist until RetroBat has run, so it is supporting evidence only.
//! - Install advice (https://wiki.retrobat.org/get-started/installation-and-first-startup):
//!   avoid spaces and special characters in the path and don't install at a drive root.
//! - System folder names (https://wiki.retrobat.org/systems-and-emulators/supported-game-systems):
//!   nes, fds, snes, megadrive, mastersystem, gamegear, psx, ps2, gb, gbc, gba, n64;
//!   rechecked 2026-11-05 for: amstradcpc, gx4000, atari800, atarist, atari2600, atari5200,
//!   atari7800, lynx, jaguar, c64, amiga500, amiga1200, amigacd32, x68000, msx1, msx2, zx81,
//!   zxspectrum, pcengine, pcenginecd, supergrafx, nds, neogeo, neogeocd, ngp, ngpc,
//!   intellivision, sg1000, odyssey2, wswan, wswanc, colecovision, vectrex, gameandwatch,
//!   sega32x, megacd, pokemini, virtualboy, supervision, cps1, cps2, cps3, dreamcast, naomi,
//!   atomiswave, saturn, 3do;
//!   arcade has no single folder (`mame` and `fbneo` take different ROM sets).
//! - Arcade guide (https://wiki.retrobat.org/systems-and-emulators/arcade-guide, checked
//!   2026-11-05): MAME games go in `roms\mame`, FinalBurn Neo in `roms\fbneo`; the romset
//!   version must match the emulator; games stay zipped ("Do not unzip"); a CHD goes in a
//!   folder named exactly like its game zip (`roms\mame\kinst\kinst.chd`); non-merged sets
//!   are recommended, split sets need their parent zips, merged sets aren't supported.

use super::{has_entry, plan_in_roms, AdapterKind, ArcadeSystem, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementOptions, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct RetroBat;

const ID: &str = "retrobat";
const NAME: &str = "RetroBat";

/// `roms\` folder for a catalog platform; `Err` explains why a game can't be placed.
/// Shared with Batocera, whose folder names are the same (checked 2026-11-06);
/// `target` names the platform in messages.
pub(super) fn system_folder(platform: &str, extension: &str, options: PlacementOptions, target: &str) -> Result<&'static str, String> {
    Ok(match platform {
        "NES" if extension == "fds" => "fds",
        "NES" => "nes",
        "SNES" => "snes",
        "Genesis" => "megadrive",
        "Master System" => "mastersystem",
        "Game Gear" => "gamegear",
        "PlayStation" => "psx",
        "PlayStation 2" => "ps2",
        "Game Boy" => "gb",
        "Game Boy Color" => "gbc",
        "GBA" => "gba",
        "N64" => "n64",
        "Famicom Disk System" => "fds",
        "Amstrad CPC" => "amstradcpc",
        "Amstrad GX4000" => "gx4000",
        "Atari 800" => "atari800",
        "Atari ST" => "atarist",
        "Atari 2600" => "atari2600",
        "Atari 5200" => "atari5200",
        "Atari 7800" => "atari7800",
        "Atari Lynx" => "lynx",
        "Atari Jaguar" => "jaguar",
        "Commodore 64" => "c64",
        "Amiga" => "amiga500",
        "Amiga 1200" => "amiga1200",
        "Amiga CD32" => "amigacd32",
        "Sharp X68000" => "x68000",
        "MSX" => "msx1",
        "MSX2" => "msx2",
        "ZX81" => "zx81",
        "ZX Spectrum" => "zxspectrum",
        "PC Engine" => "pcengine",
        "PC Engine CD" => "pcenginecd",
        "SuperGrafx" => "supergrafx",
        "Nintendo DS" => "nds",
        "Neo Geo" => "neogeo",
        "Neo Geo CD" => "neogeocd",
        "Neo Geo Pocket" => "ngp",
        "Neo Geo Pocket Color" => "ngpc",
        "Intellivision" => "intellivision",
        "SG-1000" => "sg1000",
        "Odyssey 2" => "odyssey2",
        "WonderSwan" => "wswan",
        "WonderSwan Color" => "wswanc",
        "ColecoVision" => "colecovision",
        "Vectrex" => "vectrex",
        "Game & Watch" => "gameandwatch",
        "Sega 32X" => "sega32x",
        "Sega CD" => "megacd",
        "Pokemon Mini" => "pokemini",
        "Virtual Boy" => "virtualboy",
        "Watara Supervision" => "supervision",
        "CPS1" => "cps1",
        "CPS2" => "cps2",
        "CPS3" => "cps3",
        "Dreamcast" => "dreamcast",
        "Naomi" => "naomi",
        "Atomiswave" => "atomiswave",
        "Sega Saturn" => "saturn",
        "3DO" => "3do",
        "BIOS" => return Err(format!("BIOS files belong in {target}'s bios folder, not with games; RetroDrive doesn't copy BIOS files.")),
        "Arcade" => return arcade_folder(extension, options),
        "Unclassified" => return Err("The platform isn't known (e.g. a loose .zip or .iso), so RetroDrive can't tell which system folder it belongs in.".into()),
        other => return Err(format!("RetroDrive doesn't map {other} to a {target} folder yet.")),
    })
}

/// Arcade games go to the folder of the emulator family the user chose. Only the
/// game archives themselves are placed, unchanged (their names are the romset names).
pub(super) fn arcade_folder(extension: &str, options: PlacementOptions) -> Result<&'static str, String> {
    let Some(system) = options.arcade else {
        return Err("Choose MAME or FinalBurn Neo for this installation to place arcade games (they use different ROM sets, so RetroDrive doesn't guess).".into());
    };
    match extension {
        "zip" | "7z" => Ok(match system {
            ArcadeSystem::Mame => "mame",
            ArcadeSystem::Fbneo => "fbneo",
        }),
        "chd" => Err("CHD disks must go in a folder named after their game's zip; RetroDrive doesn't link CHDs to their games yet.".into()),
        other => Err(format!("Arcade .{other} files aren't game archives, so they aren't placed.")),
    }
}

/// RetroBat's version from `system\version.info` (fallback `emulationstation\version.info`),
/// e.g. `8.3.1.1-stable-win64`. Reads at most 128 bytes and accepts only a short
/// version-like string, so an unexpected file can't inject text into the UI.
fn read_version(root: &Path) -> Option<String> {
    use std::io::Read;
    for relative in ["system/version.info", "emulationstation/version.info"] {
        let Ok(file) = std::fs::File::open(root.join(relative)) else { continue };
        let mut text = String::new();
        if file.take(128).read_to_string(&mut text).is_err() {
            continue;
        }
        let version = text.trim();
        let valid = !version.is_empty()
            && version.len() <= 64
            && version.starts_with(|c: char| c.is_ascii_digit())
            && version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
        if valid {
            return Some(version.to_string());
        }
    }
    None
}

/// Path problems the RetroBat wiki warns about (pure, for testing).
fn path_warnings(path: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if path.contains(' ') {
        warnings.push("The path contains spaces, which the RetroBat wiki advises against.".to_string());
    }
    if !path.is_ascii() {
        warnings.push("The path contains accented or special characters, which the RetroBat wiki advises against.".to_string());
    }
    let trimmed = path.trim_end_matches(['\\', '/']);
    if trimmed.len() <= 2 && trimmed.ends_with(':') {
        warnings.push("RetroBat is at the root of a drive; the RetroBat wiki recommends a subfolder.".to_string());
    }
    warnings
}

impl PlatformAdapter for RetroBat {
    fn id(&self) -> &'static str {
        ID
    }
    fn display_name(&self) -> &'static str {
        NAME
    }
    fn kind(&self) -> AdapterKind {
        AdapterKind::PortableApp
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: true, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let markers = [
            ("RetroBat.exe", false, "RetroBat.exe found"),
            ("emulationstation", true, "emulationstation folder found"),
            ("retrobat.ini", false, "retrobat.ini found"),
            ("roms", true, "roms folder found"),
        ];
        let evidence: Vec<String> = markers
            .iter()
            .filter(|(name, dir, _)| has_entry(root, name, *dir))
            .map(|(_, _, text)| text.to_string())
            .collect();
        let has_exe = has_entry(root, "RetroBat.exe", false);
        let confidence = match (has_exe, evidence.len()) {
            (true, n) if n >= 2 => Confidence::Detected,
            (_, n) if n >= 2 => Confidence::Possible,
            (true, _) => Confidence::Possible,
            _ => Confidence::NotDetected,
        };
        let version = if confidence == Confidence::NotDetected { None } else { read_version(root) };
        let hint = (confidence == Confidence::Possible).then(|| "Choose the folder that contains RetroBat.exe.".to_string());
        Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::PortableApp, confidence, evidence, version, hint }
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let folder = |id: &'static str, name: &str, missing: CheckStatus, why: &str| {
            let present = has_entry(root, name, true);
            LayoutCheck {
                id,
                label: format!("{name} folder"),
                status: if present { CheckStatus::Ok } else { missing },
                detail: if present { format!("Found {name}\\") } else { format!("{name}\\ is missing. {why}") },
            }
        };
        let mut checks = vec![
            folder("roms", "roms", CheckStatus::Problem, "Games go in roms\\<system>; start RetroBat once so it creates the folders."),
            folder("bios", "bios", CheckStatus::Warning, "Some systems need BIOS files you supply here."),
            folder("emulators", "emulators", CheckStatus::Warning, "RetroBat downloads most emulators on first use."),
            folder("emulationstation", "emulationstation", CheckStatus::Problem, "This holds RetroBat's interface and settings."),
        ];
        let path = root.display().to_string();
        let warnings = path_warnings(&path);
        checks.push(LayoutCheck {
            id: "install_path",
            label: "Install location".into(),
            status: if warnings.is_empty() { CheckStatus::Ok } else { CheckStatus::Warning },
            detail: if warnings.is_empty() { "The path follows the RetroBat wiki's advice.".into() } else { warnings.join(" ") },
        });
        checks.push(match read_version(root) {
            Some(version) => LayoutCheck {
                id: "version",
                label: "Version".into(),
                status: CheckStatus::Ok,
                detail: format!("RetroBat {version} (from system\\version.info)."),
            },
            None => LayoutCheck {
                id: "version",
                label: "Version".into(),
                status: CheckStatus::Warning,
                detail: "No readable system\\version.info, so the version is unknown.".into(),
            },
        });
        checks
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        match system_folder(&request.platform, &request.extension, request.options, NAME) {
            Ok(system) => plan_in_roms(system, request),
            Err(reason) => Placement::Unsupported { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::PlacementFile;
    use std::fs;

    fn install(dir: &Path, entries: &[(&str, bool)]) {
        for (name, is_dir) in entries {
            if *is_dir {
                fs::create_dir_all(dir.join(name)).unwrap();
            } else {
                fs::write(dir.join(name), b"").unwrap();
            }
        }
    }

    const FULL: &[(&str, bool)] = &[
        ("RetroBat.exe", false),
        ("retrobat.ini", false),
        ("emulationstation", true),
        ("emulators", true),
        ("roms", true),
        ("bios", true),
    ];

    fn request(platform: &str, ext: &str, files: &[&str]) -> PlacementRequest {
        PlacementRequest {
            title: "Saga: Part 2 (USA)".into(),
            platform: platform.into(),
            extension: ext.into(),
            files: files.iter().map(|f| PlacementFile { relative_to_primary: f.to_string(), size: 1 }).collect(),
            options: PlacementOptions::default(),
        }
    }

    #[test]
    fn detects_a_full_installation_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &[("retrobat.EXE", false), ("EmulationStation", true), ("roms", true)]);
        let d = RetroBat.detect(dir.path());
        assert_eq!(d.confidence, Confidence::Detected);
        assert_eq!(d.evidence.len(), 3, "{:?}", d.evidence);
        assert_eq!(d.version, None, "no version file in this folder");
    }

    #[test]
    fn a_fresh_install_reports_its_version() {
        // Layout of a real 8.3.1.1 install before first launch: no retrobat.ini.
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &[("RetroBat.exe", false), ("emulationstation", true), ("roms", true), ("bios", true), ("emulators", true), ("system", true)]);
        fs::write(dir.path().join("system/version.info"), b"8.3.1.1-stable-win64").unwrap();
        let d = RetroBat.detect(dir.path());
        assert_eq!(d.confidence, Confidence::Detected);
        assert_eq!(d.version.as_deref(), Some("8.3.1.1-stable-win64"));
        let version = RetroBat.validate_layout(dir.path()).into_iter().find(|c| c.id == "version").unwrap();
        assert_eq!(version.status, CheckStatus::Ok);
        assert!(version.detail.contains("8.3.1.1"));

        // Falls back to emulationstation\version.info; ignores junk.
        fs::write(dir.path().join("system/version.info"), b"<script>alert(1)</script>").unwrap();
        fs::write(dir.path().join("emulationstation/version.info"), b"8.3.1.1-stable-win64\r\n").unwrap();
        assert_eq!(RetroBat.detect(dir.path()).version.as_deref(), Some("8.3.1.1-stable-win64"));
        fs::remove_file(dir.path().join("emulationstation/version.info")).unwrap();
        assert_eq!(RetroBat.detect(dir.path()).version, None);
    }

    #[test]
    fn partial_or_unrelated_folders_are_not_detected() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(RetroBat.detect(empty.path()).confidence, Confidence::NotDetected);
        let roms_only = tempfile::tempdir().unwrap();
        install(roms_only.path(), &[("roms", true)]);
        assert_eq!(RetroBat.detect(roms_only.path()).confidence, Confidence::NotDetected);
        let looks_like = tempfile::tempdir().unwrap();
        install(looks_like.path(), &[("roms", true), ("emulationstation", true)]);
        assert_eq!(RetroBat.detect(looks_like.path()).confidence, Confidence::Possible, "no RetroBat.exe");
    }

    #[test]
    fn layout_checks_report_missing_folders() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), FULL);
        let checks = RetroBat.validate_layout(dir.path());
        let status = |id: &str| checks.iter().find(|c| c.id == id).unwrap().status;
        assert_eq!(status("roms"), CheckStatus::Ok);
        assert_eq!(status("bios"), CheckStatus::Ok);
        fs::remove_dir(dir.path().join("bios")).unwrap();
        fs::remove_dir(dir.path().join("roms")).unwrap();
        let checks = RetroBat.validate_layout(dir.path());
        let status = |id: &str| checks.iter().find(|c| c.id == id).unwrap().status;
        assert_eq!(status("bios"), CheckStatus::Warning);
        assert_eq!(status("roms"), CheckStatus::Problem);
    }

    #[test]
    fn path_advice_from_the_wiki() {
        assert!(path_warnings(r"C:\RetroBat").is_empty());
        assert_eq!(path_warnings(r"C:\Retro Bat").len(), 1);
        assert_eq!(path_warnings(r"C:\Jeux\Rétro").len(), 1);
        assert_eq!(path_warnings(r"E:\").len(), 1);
    }

    #[test]
    fn single_files_go_into_their_system_folder() {
        let plan = |p: &str, e: &str, f: &str| RetroBat.plan_placement(&request(p, e, &[f]));
        assert_eq!(plan("NES", "nes", "Kid.nes"), Placement::Planned { system_folder: "nes".into(), destinations: vec!["roms/nes/Kid.nes".into()] });
        assert_eq!(plan("NES", "fds", "Disk.fds"), Placement::Planned { system_folder: "fds".into(), destinations: vec!["roms/fds/Disk.fds".into()] });
        assert_eq!(plan("Genesis", "md", "S.md"), Placement::Planned { system_folder: "megadrive".into(), destinations: vec!["roms/megadrive/S.md".into()] });
        for (platform, folder) in [("SNES", "snes"), ("Master System", "mastersystem"), ("Game Gear", "gamegear"), ("PlayStation 2", "ps2"), ("Game Boy", "gb"), ("Game Boy Color", "gbc"), ("GBA", "gba"), ("N64", "n64")] {
            match plan(platform, "x", "a.x") {
                Placement::Planned { system_folder, .. } => assert_eq!(system_folder, folder),
                other => panic!("{platform}: {other:?}"),
            }
        }
    }

    #[test]
    fn new_systems_map_to_retrobat_folders_and_bios_is_never_placed() {
        let plan = |p: &str| RetroBat.plan_placement(&request(p, "zip", &["a.zip"]));
        for (platform, folder) in [("Commodore 64", "c64"), ("Amiga", "amiga500"), ("MSX", "msx1"), ("Atari Lynx", "lynx"), ("Sega CD", "megacd"), ("WonderSwan Color", "wswanc"), ("Famicom Disk System", "fds")] {
            match plan(platform) {
                Placement::Planned { system_folder, .. } => assert_eq!(system_folder, folder, "{platform}"),
                other => panic!("{platform}: {other:?}"),
            }
        }
        assert!(matches!(plan("BIOS"), Placement::Unsupported { .. }));
    }

    #[test]
    fn arcade_and_unknown_platforms_are_not_guessed() {
        for platform in ["Arcade", "Unclassified", "Nintendo Switch"] {
            assert!(matches!(RetroBat.plan_placement(&request(platform, "zip", &["a.zip"])), Placement::Unsupported { .. }), "{platform}");
        }
    }

    #[test]
    fn arcade_games_follow_the_chosen_emulator_and_keep_their_names() {
        let with = |system, ext: &str, file: &str| {
            let mut r = request("Arcade", ext, &[file]);
            r.options.arcade = system;
            RetroBat.plan_placement(&r)
        };
        assert_eq!(with(Some(ArcadeSystem::Mame), "zip", "sf2.zip"), Placement::Planned { system_folder: "mame".into(), destinations: vec!["roms/mame/sf2.zip".into()] });
        assert_eq!(with(Some(ArcadeSystem::Fbneo), "7z", "mslug.7z"), Placement::Planned { system_folder: "fbneo".into(), destinations: vec!["roms/fbneo/mslug.7z".into()] });
        match with(None, "zip", "sf2.zip") {
            Placement::Unsupported { reason } => assert!(reason.contains("Choose MAME or FinalBurn Neo"), "{reason}"),
            other => panic!("{other:?}"),
        }
        match with(Some(ArcadeSystem::Mame), "chd", "kinst.chd") {
            Placement::Unsupported { reason } => assert!(reason.contains("CHD"), "{reason}"),
            other => panic!("{other:?}"),
        }
        // Other platforms ignore the arcade choice.
        let mut nes = request("NES", "nes", &["Kid.nes"]);
        nes.options.arcade = Some(ArcadeSystem::Fbneo);
        assert_eq!(RetroBat.plan_placement(&nes), Placement::Planned { system_folder: "nes".into(), destinations: vec!["roms/nes/Kid.nes".into()] });
    }

    #[test]
    fn multi_file_games_get_their_own_folder_keeping_relative_paths() {
        let plan = RetroBat.plan_placement(&request("PlayStation", "m3u", &["Saga.m3u", "Saga (Disc 1).cue", "CD2/Saga (Disc 2).cue"]));
        assert_eq!(
            plan,
            Placement::Planned {
                system_folder: "psx".into(),
                destinations: vec![
                    "roms/psx/Saga_ Part 2 (USA)/Saga.m3u".into(),
                    "roms/psx/Saga_ Part 2 (USA)/Saga (Disc 1).cue".into(),
                    "roms/psx/Saga_ Part 2 (USA)/CD2/Saga (Disc 2).cue".into(),
                ]
            }
        );
        let escaping = RetroBat.plan_placement(&request("PlayStation", "m3u", &["Saga.m3u", "../elsewhere/d1.cue"]));
        assert!(matches!(escaping, Placement::Unsupported { .. }));
    }
}
