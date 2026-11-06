//! Batocera adapter (RDS-008): read-only recognition, layout checks and placement
//! *previews*. Copying into Batocera is off (`copy_games: false`) until it has its
//! own safety review: the copier's checks were reviewed for local Windows drives
//! (docs/18), not for network shares.
//!
//! Facts checked 2026-11-06:
//! - Batocera is a bootable OS. The **BATOCERA** partition is the system and the only
//!   one Windows sees by default; its key files are `boot/batocera` (the system image)
//!   and `batocera-boot.conf` (https://wiki.batocera.org/batocera.linux_architecture;
//!   `board/batocera/scripts/post-image-script.sh` also writes `boot/batocera.board`).
//!   Games never go there.
//! - Games live on the **SHARE** ("userdata") partition, ext4 by default, which Windows
//!   can't read without extra tools (same page; https://wiki.batocera.org/add_games_bios).
//!   From Windows it's reached as the network share `\\BATOCERA\share` (add_games_bios).
//! - When an external drive is chosen as storage, Batocera uses a `batocera` folder at
//!   that drive's root as userdata (`board/batocera/fsoverlay/etc/init.d/S11share`
//!   bind-mounts `<drive>/batocera` on `/userdata`); exFAT and NTFS are readable by
//!   Windows (architecture page).
//! - Every boot, `S12populateshare` creates `roms`, `bios`, `saves`, `screenshots`,
//!   `music`, `splash`, ... and `system/batocera.conf` if missing, so a set-up userdata
//!   folder always has `roms` and `system/batocera.conf`.
//! - System folders (`roms/<system>`) come from `es_systems.yml`; every RetroBat
//!   folder RetroDrive uses exists there except `cps1`/`cps2`/`cps3` (Batocera plays
//!   CPS games from its arcade folders).
//! - No version file is on SHARE or readable on the boot partition (it's inside the
//!   system image), so the version is reported as unknown.

use super::retrobat::{arcade_folder, system_folder};
use super::{has_entry, plan_in_roms, AdapterKind, Capabilities, CheckStatus, Confidence, Detection, LayoutCheck, Placement, PlacementRequest, PlatformAdapter};
use std::path::Path;

pub struct Batocera;

const ID: &str = "batocera";
const NAME: &str = "Batocera";

/// Folders Batocera's first boot creates in userdata besides `roms` and `system`.
const USERDATA_FOLDERS: &[&str] = &["bios", "saves", "screenshots", "music", "splash", "extractions", "library", "recordings", "kodi"];

fn has_conf(root: &Path) -> bool {
    has_entry(root, "system", true) && has_entry(&root.join("system"), "batocera.conf", false)
}

fn is_boot_partition(root: &Path) -> bool {
    has_entry(root, "batocera-boot.conf", false) && has_entry(root, "boot", true) && has_entry(&root.join("boot"), "batocera", false)
}

fn is_network_path(path: &str) -> bool {
    let path = path.strip_prefix(r"\\?\UNC\").map(|_| r"\\").unwrap_or(path);
    path.starts_with(r"\\") || path.starts_with("//")
}

impl PlatformAdapter for Batocera {
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
        Capabilities { detect: true, validate_layout: true, plan_placement: true, copy_games: false, install: false, write_files: false }
    }

    fn detect(&self, root: &Path) -> Detection {
        let detection = |confidence, evidence: Vec<String>, hint: Option<String>| Detection { adapter_id: ID, display_name: NAME, kind: AdapterKind::BootableOs, confidence, evidence, version: None, hint };

        if is_boot_partition(root) {
            return detection(
                Confidence::Possible,
                vec!["batocera-boot.conf found".into(), "boot\\batocera found".into()],
                Some("This is Batocera's system partition; games don't go here. Choose Batocera's game folder instead: \\\\BATOCERA\\share over the network while Batocera is running, or the batocera folder on an external game drive. Windows can't read Batocera's own SHARE partition (ext4) by default.".into()),
            );
        }
        // RetroBat has a similar layout; never claim a RetroBat folder.
        if has_entry(root, "RetroBat.exe", false) {
            return detection(Confidence::NotDetected, Vec::new(), None);
        }
        let roms = has_entry(root, "roms", true);
        let conf = has_conf(root);
        let others: Vec<&str> = USERDATA_FOLDERS.iter().copied().filter(|f| has_entry(root, f, true)).collect();
        let mut evidence = Vec::new();
        if roms {
            evidence.push("roms folder found".to_string());
        }
        if conf {
            evidence.push("system\\batocera.conf found".to_string());
        }
        if !others.is_empty() {
            evidence.push(format!("Batocera folders found: {}", others.join(", ")));
        }
        if roms && conf {
            return detection(Confidence::Detected, evidence, None);
        }
        if roms && others.len() >= 2 {
            return detection(
                Confidence::Possible,
                evidence,
                Some("This looks like a Batocera game folder, but system\\batocera.conf is missing. Start Batocera once with this drive or share so it finishes setting it up, then choose it again.".into()),
            );
        }
        // An external game drive: Batocera keeps everything in <drive>\batocera.
        let nested = root.join("batocera");
        if has_entry(root, "batocera", true) && has_entry(&nested, "roms", true) {
            return detection(
                Confidence::Possible,
                vec!["batocera folder with roms found".into()],
                Some(format!("Batocera keeps its games in the batocera folder on this drive. Choose {} instead.", nested.display())),
            );
        }
        detection(Confidence::NotDetected, evidence, None)
    }

    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck> {
        let check = |id: &'static str, label: &str, ok: bool, missing: CheckStatus, found: &str, why: &str| LayoutCheck {
            id,
            label: label.into(),
            status: if ok { CheckStatus::Ok } else { missing },
            detail: if ok { found.into() } else { why.into() },
        };
        let path = root.display().to_string();
        let network = is_network_path(&path);
        vec![
            check("roms", "roms folder", has_entry(root, "roms", true), CheckStatus::Problem, "Found roms\\", "roms\\ is missing. Games go in roms\\<system>; start Batocera once so it creates the folders."),
            check("bios", "bios folder", has_entry(root, "bios", true), CheckStatus::Warning, "Found bios\\", "bios\\ is missing. Some systems need BIOS files you supply here."),
            check("config", "Batocera settings", has_conf(root), CheckStatus::Warning, "Found system\\batocera.conf", "system\\batocera.conf is missing; Batocera creates it when it starts."),
            LayoutCheck {
                id: "access",
                label: "Access".into(),
                status: CheckStatus::Ok,
                detail: if network {
                    "Network share. It's only reachable while Batocera is running and on the network.".into()
                } else {
                    "Folder on a drive Windows can read (for example an exFAT or NTFS game drive).".into()
                },
            },
            LayoutCheck {
                id: "copying",
                label: "Copying".into(),
                status: CheckStatus::Warning,
                detail: "RetroDrive can plan for Batocera and show what would go where, but doesn't copy to it yet. Copying needs its own safety review first.".into(),
            },
            LayoutCheck {
                id: "version",
                label: "Version".into(),
                status: CheckStatus::Warning,
                detail: "Batocera doesn't keep its version in the game folder, so it's unknown here. Batocera's menu shows it.".into(),
            },
        ]
    }

    fn plan_placement(&self, request: &PlacementRequest) -> Placement {
        let folder = match request.platform.as_str() {
            // Batocera has no CPS folders; CPS games are arcade romsets.
            "CPS1" | "CPS2" | "CPS3" => arcade_folder(&request.extension, request.options),
            platform => system_folder(platform, &request.extension, request.options, NAME),
        };
        match folder {
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

    const USERDATA: &[&str] = &["roms/nes/_info.txt", "bios/", "saves/", "screenshots/", "system/batocera.conf"];

    fn request(platform: &str, ext: &str, files: &[&str]) -> PlacementRequest {
        PlacementRequest {
            title: "Saga".into(),
            platform: platform.into(),
            extension: ext.into(),
            files: files.iter().map(|f| PlacementFile { relative_to_primary: f.to_string(), size: 1 }).collect(),
            options: PlacementOptions::default(),
        }
    }

    #[test]
    fn a_set_up_game_folder_is_detected_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        make(dir.path(), &["ROMS/snes/", "Bios/", "System/Batocera.conf"]);
        let d = Batocera.detect(dir.path());
        assert_eq!(d.confidence, Confidence::Detected, "{:?}", d.evidence);
        assert_eq!(d.kind, AdapterKind::BootableOs);
        assert_eq!(d.version, None);
        assert!(d.hint.is_none());
    }

    #[test]
    fn near_misses_explain_which_folder_to_choose() {
        // Boot partition: never saved, and says where games go.
        let boot = tempfile::tempdir().unwrap();
        make(boot.path(), &["batocera-boot.conf", "boot/batocera", "boot/batocera.board"]);
        let d = Batocera.detect(boot.path());
        assert_eq!(d.confidence, Confidence::Possible);
        assert!(d.hint.unwrap().contains("system partition"));

        // External game drive root: points at <drive>\batocera.
        let drive = tempfile::tempdir().unwrap();
        make(drive.path(), &["batocera/roms/", "batocera/system/batocera.conf", "Movies/"]);
        let d = Batocera.detect(drive.path());
        assert_eq!(d.confidence, Confidence::Possible);
        assert!(d.hint.unwrap().contains("batocera"));
        assert_eq!(Batocera.detect(&drive.path().join("batocera")).confidence, Confidence::Detected);

        // Userdata folders without batocera.conf.
        let unfinished = tempfile::tempdir().unwrap();
        make(unfinished.path(), &["roms/", "bios/", "saves/"]);
        assert_eq!(Batocera.detect(unfinished.path()).confidence, Confidence::Possible);
    }

    #[test]
    fn retrobat_and_unrelated_folders_are_not_batocera() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(Batocera.detect(empty.path()).confidence, Confidence::NotDetected);
        let roms_only = tempfile::tempdir().unwrap();
        make(roms_only.path(), &["roms/"]);
        assert_eq!(Batocera.detect(roms_only.path()).confidence, Confidence::NotDetected);
        let retrobat = tempfile::tempdir().unwrap();
        make(retrobat.path(), &["RetroBat.exe", "emulationstation/", "roms/", "bios/", "saves/", "system/batocera.conf"]);
        assert_eq!(Batocera.detect(retrobat.path()).confidence, Confidence::NotDetected);
        let best = crate::adapters::detect_all(retrobat.path()).into_iter().next().unwrap();
        assert_eq!((best.adapter_id, best.confidence), ("retrobat", Confidence::Detected));
    }

    #[test]
    fn layout_checks_say_copying_is_not_available() {
        let dir = tempfile::tempdir().unwrap();
        make(dir.path(), USERDATA);
        let checks = Batocera.validate_layout(dir.path());
        let get = |id: &str| checks.iter().find(|c| c.id == id).unwrap().clone();
        assert_eq!(get("roms").status, CheckStatus::Ok);
        assert_eq!(get("config").status, CheckStatus::Ok);
        assert_eq!(get("copying").status, CheckStatus::Warning);
        fs::remove_dir_all(dir.path().join("roms")).unwrap();
        assert_eq!(Batocera.validate_layout(dir.path()).iter().find(|c| c.id == "roms").unwrap().status, CheckStatus::Problem);
        assert!(is_network_path(r"\\BATOCERA\share"));
        assert!(is_network_path(r"\\?\UNC\BATOCERA\share"));
        assert!(!is_network_path(r"E:\batocera"));
    }

    #[test]
    fn placement_uses_batocera_folders() {
        let plan = |p: &str, e: &str, f: &[&str]| Batocera.plan_placement(&request(p, e, f));
        assert_eq!(plan("SNES", "sfc", &["Mario.sfc"]), Placement::Planned { system_folder: "snes".into(), destinations: vec!["roms/snes/Mario.sfc".into()] });
        assert_eq!(plan("Genesis", "md", &["S.md"]), Placement::Planned { system_folder: "megadrive".into(), destinations: vec!["roms/megadrive/S.md".into()] });
        assert_eq!(
            plan("PlayStation", "m3u", &["Saga.m3u", "Saga (Disc 1).cue"]),
            Placement::Planned { system_folder: "psx".into(), destinations: vec!["roms/psx/Saga/Saga.m3u".into(), "roms/psx/Saga/Saga (Disc 1).cue".into()] }
        );
        match plan("BIOS", "bin", &["scph.bin"]) {
            Placement::Unsupported { reason } => assert!(reason.contains("Batocera's bios folder"), "{reason}"),
            other => panic!("{other:?}"),
        }
        // CPS games follow the arcade choice; without one they aren't placed.
        assert!(matches!(plan("CPS2", "zip", &["ddtod.zip"]), Placement::Unsupported { .. }));
        let mut cps = request("CPS2", "zip", &["ddtod.zip"]);
        cps.options.arcade = Some(ArcadeSystem::Fbneo);
        assert_eq!(Batocera.plan_placement(&cps), Placement::Planned { system_folder: "fbneo".into(), destinations: vec!["roms/fbneo/ddtod.zip".into()] });
    }
}
