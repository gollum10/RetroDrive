//! BIOS checker (FR-07, ADR-088; docs/18 §55): for the systems in the user's library,
//! which BIOS files RetroArch's cores look for, and whether the user has them in a
//! library `bios` folder and on the chosen system. Reads only; RetroDrive never ships
//! or downloads BIOS files. The list is `resources/bios_requirements.tsv` (from
//! libretro-core-info, MIT).

use crate::adapters::{self, Placement, PlacementFile, PlacementOptions, PlacementRequest};
use crate::bios_step;
use crate::customizer::PresenceFn;
use crate::installations;
use crate::volumes::Presence;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIREMENTS: &str = include_str!("../resources/bios_requirements.tsv");

#[derive(Debug, Clone, PartialEq)]
struct Requirement {
    system: &'static str,
    file: &'static str,
    level: &'static str,
    group: Option<&'static str>,
    what: &'static str,
    core: &'static str,
}

fn requirements() -> Vec<Requirement> {
    REQUIREMENTS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() == 6).then(|| Requirement { system: f[0], file: f[1], level: f[2], group: Some(f[3]).filter(|g| !g.is_empty()), what: f[4], core: f[5] })
        })
        .collect()
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct BiosFile {
    /// Path below the BIOS folder (`dc/dc_boot.bin`).
    pub file: String,
    /// `required`, `recommended` or `optional`.
    pub level: String,
    /// Files with the same group: any one is enough.
    pub group: Option<String>,
    pub what: String,
    pub core: String,
    /// `yes`, `other_folder` (same name, different subfolder) or `no`.
    pub library: String,
    /// On the chosen system (`None`: no system chosen or not reachable).
    pub on_system: Option<bool>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SystemBios {
    pub system: String,
    pub platform: String,
    pub games: i64,
    /// `ok`, `missing` (a required file or group isn't found anywhere), `recommended`
    /// (only recommended files are missing) or `not_on_system` (in the library, not yet
    /// on the chosen system).
    pub status: String,
    pub files: Vec<BiosFile>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct BiosCheck {
    pub systems: Vec<SystemBios>,
    /// The chosen system's BIOS folder, when it could be read.
    pub system_folder: Option<String>,
    /// Why the chosen system's BIOS folder couldn't be checked.
    pub system_note: Option<String>,
}

/// Library platforms (not BIOS) with their number of games.
fn library_platforms(connection: &Connection) -> Result<Vec<(String, i64)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT g.platform, COUNT(DISTINCT g.id) FROM games g JOIN game_releases r ON r.game_id = g.id JOIN game_files gf ON gf.release_id = r.id
             WHERE gf.excluded_at IS NULL AND g.platform <> ?1 GROUP BY g.platform ORDER BY g.platform",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([crate::classify::BIOS], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string());
    rows
}

/// A path below `dir`, matched without regard to case (Samba shares on Linux are case-sensitive).
fn exists_ci(dir: &Path, rel: &str) -> bool {
    let mut current = dir.to_path_buf();
    for part in rel.split('/') {
        let exact = current.join(part);
        if fs::symlink_metadata(&exact).is_ok() {
            current = exact;
            continue;
        }
        let Some(found) = fs::read_dir(&current).ok().and_then(|r| r.filter_map(Result::ok).find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))) else { return false };
        current = found.path();
    }
    true
}

/// The installation's BIOS folder: where Plan puts BIOS files, or the BIOS step's target.
fn installation_bios_dir(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<(PathBuf, PathBuf), String> {
    let install = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if install.presence != Presence::Connected {
        return Err("The system isn't connected, so its BIOS folder wasn't checked.".into());
    }
    let adapter = adapters::registry().into_iter().find(|a| a.id() == install.adapter_id).ok_or("Unknown system")?;
    let request = PlacementRequest { title: "bios".into(), platform: crate::classify::BIOS.into(), extension: "bin".into(), files: vec![PlacementFile { relative_to_primary: "x.bin".into(), size: 1 }], options: install.options, bios_path: Some("x.bin".into()) };
    let dir = match adapter.plan_placement(&request) {
        Placement::Planned { destinations, .. } => {
            let rel = destinations.first().and_then(|d| d.strip_suffix("x.bin")).unwrap_or("bios/").trim_end_matches('/').to_string();
            crate::paths::join(&install.root.display().to_string(), &rel)
        }
        Placement::Unsupported { .. } => bios_step::target_for(connection, installation_id, presence)?,
    };
    if !dir.is_dir() {
        return Err(format!("{} wasn't found, so the system's BIOS files weren't checked.", dir.display()));
    }
    Ok((dir, install.root))
}

/// Check the library (and optionally one system) against the BIOS list (reads only).
pub fn check(connection: &Connection, installation_id: Option<&str>, presence: PresenceFn) -> Result<BiosCheck, String> {
    let library = bios_step::library_bios(connection, presence)?;
    let names: Vec<String> = library.keys().map(|k| k.rsplit('/').next().unwrap_or(k).to_string()).collect();
    let (system_dir, system_root, system_note) = match installation_id {
        None => (None, None, None),
        Some(id) => match installation_bios_dir(connection, id, presence) {
            Ok((dir, root)) => (Some(dir), Some(root), None),
            Err(e) => (None, None, Some(e)),
        },
    };
    let reqs = requirements();
    let mut by_system: BTreeMap<&str, Vec<&Requirement>> = BTreeMap::new();
    for r in &reqs {
        by_system.entry(r.system).or_default().push(r);
    }
    let mut systems = Vec::new();
    for (platform, games) in library_platforms(connection)? {
        let Ok(folder) = adapters::retrobat::system_folder(&platform, "", PlacementOptions::default(), "RetroDrive") else { continue };
        let Some(list) = by_system.get(folder) else { continue };
        let files: Vec<BiosFile> = list
            .iter()
            .map(|r| {
                let key = r.file.to_lowercase();
                let base = key.rsplit('/').next().unwrap_or(&key).to_string();
                // Neo Geo's BIOS can also sit with the games (FinalBurn Neo looks there too).
                let with_games = r.file == "neogeo.zip" && system_root.as_ref().is_some_and(|root| exists_ci(root, "roms/neogeo/neogeo.zip") || exists_ci(root, "neogeo/neogeo.zip"));
                let library_state = if library.contains_key(&key) { "yes" } else if names.contains(&base) { "other_folder" } else { "no" };
                BiosFile {
                    file: r.file.to_string(),
                    level: r.level.to_string(),
                    group: r.group.map(str::to_string),
                    what: r.what.to_string(),
                    core: r.core.to_string(),
                    library: library_state.to_string(),
                    on_system: system_dir.as_ref().map(|d| exists_ci(d, r.file) || with_games),
                }
            })
            .collect();
        systems.push(SystemBios { system: folder.to_string(), platform: platform.clone(), games, status: status(&files, system_dir.is_some()), files });
    }
    Ok(BiosCheck { systems, system_folder: system_dir.map(|d| d.display().to_string()), system_note })
}

/// A system's overall state from its files (groups: any one file is enough).
fn status(files: &[BiosFile], checking_system: bool) -> String {
    let have_any = |f: &BiosFile| f.library == "yes" || f.on_system == Some(true);
    let have_there = |f: &BiosFile| if checking_system { f.on_system == Some(true) } else { f.library == "yes" };
    let satisfied = |level: &str, have: &dyn Fn(&BiosFile) -> bool| {
        let mut groups: BTreeMap<String, bool> = BTreeMap::new();
        for f in files.iter().filter(|f| f.level == level) {
            let key = f.group.clone().unwrap_or_else(|| format!("file:{}", f.file));
            *groups.entry(key).or_insert(false) |= have(f);
        }
        groups.values().all(|ok| *ok)
    };
    if !satisfied("required", &have_any) {
        "missing".into()
    } else if checking_system && !satisfied("required", &have_there) {
        "not_on_system".into()
    } else if !satisfied("recommended", &have_any) {
        "recommended".into()
    } else {
        "ok".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, scanner};
    use crate::volumes::VolumeProbe;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn write(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn the_list_parses_and_names_known_systems() {
        let reqs = requirements();
        assert!(reqs.len() > 30);
        for r in &reqs {
            assert!(["required", "recommended", "optional"].contains(&r.level), "{r:?}");
            assert!(!r.file.contains('\\') && !r.file.starts_with('/') && !r.file.contains(".."), "{r:?}");
        }
        assert!(reqs.iter().any(|r| r.system == "megacd" && r.group == Some("segacd")));
    }

    #[test]
    fn library_and_system_bios_are_checked_per_system() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("library");
        for g in ["segacd/Sonic CD (USA).chd", "psx/Crash (USA).chd", "lynx/Klax (USA).lnx", "snes/Mario (USA).sfc", "dreamcast/Crazy Taxi (USA).chd"] {
            write(&lib, g);
        }
        write(&lib, "bios/bios_CD_U.bin");
        write(&lib, "bios/dc_boot.bin");
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &lib, &|_: &str| VolumeProbe::unknown()).unwrap();

        let library_only = check(&c, None, &connected).unwrap();
        let by = |r: &BiosCheck, s: &str| r.systems.iter().find(|x| x.system == s).cloned();
        assert_eq!(by(&library_only, "megacd").unwrap().status, "ok", "one of the regional Sega CD BIOS is enough");
        assert_eq!(by(&library_only, "lynx").unwrap().status, "missing");
        assert_eq!(by(&library_only, "psx").unwrap().status, "recommended");
        let dc = by(&library_only, "dreamcast").unwrap();
        assert_eq!(dc.files[0].library, "other_folder", "dc_boot.bin belongs in a dc subfolder");
        assert!(by(&library_only, "snes").is_none(), "no BIOS needed");

        // With a RetroBat: its bios folder is checked too (names matched without regard to case).
        let rb = dir.path().join("RetroBat");
        for d in ["emulationstation", "emulators", "roms/snes"] {
            fs::create_dir_all(rb.join(d)).unwrap();
        }
        fs::write(rb.join("RetroBat.exe"), b"").unwrap();
        write(&rb, "bios/LYNXBOOT.IMG");
        let id = installations::add(&c, &rb, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let with = check(&c, Some(&id), &connected).unwrap();
        assert!(with.system_folder.as_deref().unwrap().ends_with("bios"));
        assert_eq!(by(&with, "lynx").unwrap().status, "ok", "on the system, though not in the library");
        assert_eq!(by(&with, "megacd").unwrap().status, "not_on_system", "in the library, not yet copied");
        assert_eq!(by(&with, "megacd").unwrap().files.iter().find(|f| f.file == "bios_CD_U.bin").unwrap().on_system, Some(false));
    }
}
