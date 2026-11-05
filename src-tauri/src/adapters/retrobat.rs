//! RetroBat adapter (RDS-007): read-only recognition, layout checks and placement plans.
//!
//! Facts verified 2026-11-04 against the RetroBat wiki:
//! - Folder structure (https://wiki.retrobat.org/get-started/retrobat-folder-structure):
//!   `RetroBat.exe`, `retrobat.ini`, `emulationstation\`, `emulators\`, `roms\`
//!   (one folder per system), `bios\`, `saves\`, `system\` (replaced on update).
//!   No version file is documented, so the version is reported as unknown.
//! - Install advice (https://wiki.retrobat.org/get-started/installation-and-first-startup):
//!   avoid spaces and special characters in the path and don't install at a drive root.
//! - System folder names (https://wiki.retrobat.org/systems-and-emulators/supported-game-systems):
//!   nes, fds, snes, megadrive, mastersystem, gamegear, psx, ps2, gb, gbc, gba, n64;
//!   arcade has no single folder (`mame` and `fbneo` take different ROM sets).

use super::{AdapterKind, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct RetroBat;

const ID: &str = "retrobat";
const NAME: &str = "RetroBat";

/// Case-insensitive check for an entry directly inside `root`.
fn has_entry(root: &Path, name: &str, want_dir: bool) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else { return false };
    entries.filter_map(Result::ok).any(|e| {
        e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.file_type().map(|t| t.is_dir() == want_dir).unwrap_or(false)
    })
}

/// RetroBat `roms\` folder for a catalog platform; `Err` explains why a game can't be placed.
fn system_folder(platform: &str, extension: &str) -> Result<&'static str, String> {
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
        "Arcade" => {
            return Err("Arcade games need a choice between MAME and FinalBurn Neo (they use different ROM sets), so they aren't placed automatically yet.".into())
        }
        "Unclassified" => return Err("The platform isn't known (e.g. a loose .zip or .iso), so RetroDrive can't tell which system folder it belongs in.".into()),
        other => return Err(format!("RetroDrive doesn't map {other} to a RetroBat folder yet.")),
    })
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
        Capabilities { detect: true, validate_layout: true, plan_placement: true, install: false, write_files: false }
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
        Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::PortableApp, confidence, evidence, version: None }
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
        checks.push(LayoutCheck {
            id: "version",
            label: "Version".into(),
            status: CheckStatus::Warning,
            detail: "RetroBat doesn't document a version file, so the version is unknown.".into(),
        });
        checks
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        let system = match system_folder(&request.platform, &request.extension) {
            Ok(s) => s,
            Err(reason) => return Placement::Unsupported { reason },
        };
        if request.files.iter().any(|f| f.relative_to_primary.split('/').any(|part| part == ".." || part.is_empty())) {
            return Placement::Unsupported { reason: "Its files are spread across folders, so the set can't be copied as one unit.".into() };
        }
        // Single files go straight into the system folder; multi-file games get
        // their own subfolder so .m3u/.cue references keep working.
        let base = if request.files.len() > 1 {
            format!("roms/{system}/{}", super::folder_name(&request.title))
        } else {
            format!("roms/{system}")
        };
        Placement::Planned {
            system_folder: system.to_string(),
            destinations: request.files.iter().map(|f| format!("{base}/{}", f.relative_to_primary)).collect(),
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
        }
    }

    #[test]
    fn detects_a_full_installation_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &[("retrobat.EXE", false), ("EmulationStation", true), ("roms", true)]);
        let d = RetroBat.detect(dir.path());
        assert_eq!(d.confidence, Confidence::Detected);
        assert_eq!(d.evidence.len(), 3, "{:?}", d.evidence);
        assert_eq!(d.version, None, "no documented version file");
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
    fn arcade_and_unknown_platforms_are_not_guessed() {
        for platform in ["Arcade", "Unclassified", "Atari Jaguar"] {
            assert!(matches!(RetroBat.plan_placement(&request(platform, "zip", &["a.zip"])), Placement::Unsupported { .. }), "{platform}");
        }
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
