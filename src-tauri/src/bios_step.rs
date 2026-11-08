//! BIOS files outside the installation folder (owner request 2026-11-07, "#1";
//! docs/18 §36): RetroPie over the network keeps BIOS on its own `bios` share
//! (`\\<host>\bios`, RetroPie-Setup `samba.sh`), and ES-DE has no BIOS folder: its
//! RetroArch reads them from RetroArch's `system` folder. Plans and the copier keep
//! every file inside the installation folder, so this is a separate step built like
//! the art step's picture copies: **new files only** (temporary name, checked, renamed
//! without replacing), a file already there is never replaced (a different one is
//! listed as kept), and nothing is deleted. Where the installation folder can hold the
//! BIOS itself (RetroBat, Batocera, Recalbox, a RetroPie USB folder), the `bios` row in
//! Plan does it instead.
//!
//! ES-DE (portable): RetroArch is looked for in `Emulators\RetroArch-Win64` and
//! `Emulators\RetroArch` next to `ES-DE.exe` (the portable release's README and
//! `es_find_rules.xml`), then `C:\RetroArch-Win64` (RetroArch's default install). Its
//! system folder is `system_directory` from `retroarch.cfg` (`:\` = RetroArch's own
//! folder; `default` or missing = `<RetroArch>\system`).

use crate::adapters;
use crate::copier;
use crate::customizer::PresenceFn;
use crate::gamelists;
use crate::installations;
use crate::paths;
use crate::volumes::Presence;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const LIST_LIMIT: usize = 50;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BiosPreview {
    /// Where the files go.
    pub target: String,
    pub to_copy: usize,
    pub bytes: u64,
    pub already_there: usize,
    /// Different files with the same name already there (kept), first few.
    pub kept: Vec<String>,
    pub kept_count: usize,
    /// Paths with two different files in the library (not copied), first few.
    pub two_versions: Vec<String>,
    pub two_versions_count: usize,
}

/// `\\host\roms\…` → `\\host\bios\` (RetroPie's BIOS share on the same Pi).
pub fn bios_share(installation: &str) -> Option<PathBuf> {
    let rest = installation.strip_prefix(r"\\")?;
    let host = rest.split(['\\', '/']).next().filter(|h| !h.is_empty())?;
    Some(PathBuf::from(format!(r"\\{host}\bios\")))
}

/// RetroArch's system folder from `retroarch.cfg` in `ra` (RetroArch's own folder).
pub fn retroarch_system_dir(ra: &Path) -> PathBuf {
    let value = fs::read_to_string(ra.join("retroarch.cfg"))
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == "system_directory").then(|| v.trim().trim_matches('"').to_string())
            })
        });
    match value.as_deref() {
        None | Some("") | Some("default") => ra.join("system"),
        Some(v) if v.starts_with(":\\") || v.starts_with(":/") => {
            ra.join(v[2..].trim_start_matches(['\\', '/']))
        }
        Some(v) if v.len() > 2 && (v.as_bytes()[1] == b':' || v.starts_with(r"\\")) => {
            PathBuf::from(v)
        }
        Some(_) => ra.join("system"),
    }
}

/// The RetroArch folder ES-DE uses for this ROMs folder, if one is installed.
pub fn esde_retroarch(roms: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(program) = roms.parent().filter(|p| p.join("ES-DE.exe").is_file()) {
        candidates.push(program.join("Emulators").join("RetroArch-Win64"));
        candidates.push(program.join("Emulators").join("RetroArch"));
    }
    candidates.push(PathBuf::from(r"C:\RetroArch-Win64"));
    candidates
        .into_iter()
        .find(|d| d.join("retroarch.exe").is_file())
}

/// Where this installation's BIOS step copies to, or why there's none.
fn target_for(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
) -> Result<PathBuf, String> {
    let install = installations::located(connection, presence, Some(installation_id))?
        .into_iter()
        .next()
        .ok_or("Installation not found")?;
    if install.presence != Presence::Connected {
        return Err("The installation isn't connected.".into());
    }
    let root_text = install.root.display().to_string();
    let target = match install.adapter_id.as_str() {
        "retropie" if install.options.has_roms_folder => {
            return Err(
                "On a RetroPie USB stick, BIOS files go with the games: tick bios in Plan.".into(),
            )
        }
        "retropie" => bios_share(&root_text).ok_or(
            "RetroPie's BIOS share is reached over the network; add RetroPie by its network share.",
        )?,
        "esde" => {
            let ra = esde_retroarch(&install.root).ok_or("ES-DE uses RetroArch's system folder for BIOS files, and RetroArch isn't installed for this ES-DE yet. Put RetroArch in ES-DE's Emulators folder (Emulators\\RetroArch), start it once, then try again.")?;
            retroarch_system_dir(&ra)
        }
        _ => {
            return Err("For this system, BIOS files go with the games: tick bios in Plan.".into())
        }
    };
    if !target.is_dir() {
        return Err(format!(
            "{} can't be reached. Make sure it exists (start the system or RetroArch once).",
            target.display()
        ));
    }
    Ok(target)
}

/// One BIOS file in the library: path below its `bios` folder, the file, its size.
struct LibraryBios {
    rel: String,
    source: PathBuf,
    size: u64,
}

/// BIOS files from library `bios` folders on connected drives, by path below `bios`.
fn library_bios(
    connection: &Connection,
    presence: PresenceFn,
) -> Result<BTreeMap<String, Vec<LibraryBios>>, String> {
    let mut statement = connection
        .prepare(
            "SELECT fl.relative_path, gf.size_bytes, fl.present_status, s.root_path, s.stable_id
             FROM game_files gf JOIN game_releases r ON r.id = gf.release_id JOIN games g ON g.id = r.game_id
             JOIN file_locations fl ON fl.game_file_id = gf.id JOIN storage_locations s ON s.id = fl.storage_location_id
             WHERE g.platform = ?1 AND gf.excluded_at IS NULL ORDER BY fl.relative_path",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, i64, String, String, Option<String>)> = statement
        .query_map([crate::classify::BIOS], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut online: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut out: BTreeMap<String, Vec<LibraryBios>> = BTreeMap::new();
    for (relative, size, status, root, stable) in rows {
        if status != "present"
            || !*online
                .entry(root.clone())
                .or_insert_with(|| presence(&root, stable.as_deref()) == Presence::Connected)
        {
            continue;
        }
        let parts: Vec<&str> = relative.split('/').collect();
        let Some(at) = parts.iter().position(|p| p.eq_ignore_ascii_case("bios")) else {
            continue;
        };
        let rel = parts[at + 1..].join("/");
        if rel.is_empty()
            || adapters::is_housekeeping(&rel)
            || rel.split('/').any(|p| p.is_empty() || p == "..")
        {
            continue;
        }
        out.entry(rel.to_lowercase())
            .or_default()
            .push(LibraryBios {
                rel,
                source: paths::join(&root, &relative),
                size: size.max(0) as u64,
            });
    }
    Ok(out)
}

/// What copying the library's BIOS files into `target` would do, and the copies.
fn plan_into(
    connection: &Connection,
    target: &Path,
    presence: PresenceFn,
) -> Result<(BiosPreview, Vec<(PathBuf, PathBuf)>), String> {
    let library = library_bios(connection, presence)?;
    let mut preview = BiosPreview {
        target: target.display().to_string(),
        to_copy: 0,
        bytes: 0,
        already_there: 0,
        kept: Vec::new(),
        kept_count: 0,
        two_versions: Vec::new(),
        two_versions_count: 0,
    };
    let mut copies = Vec::new();
    for files in library.values() {
        let first = &files[0];
        // Two different files for the same place: not copied (sizes differ, or same size and different contents).
        let differ = files.iter().skip(1).any(|f| {
            f.size != first.size
                || copier::sha256_of(&f.source).ok() != copier::sha256_of(&first.source).ok()
        });
        if differ {
            preview.two_versions_count += 1;
            if preview.two_versions.len() < LIST_LIMIT {
                preview.two_versions.push(first.rel.clone());
            }
            continue;
        }
        let dest = paths::join(&target.display().to_string(), &first.rel);
        match fs::metadata(&dest) {
            Ok(m)
                if m.len() == first.size
                    && copier::sha256_of(&dest).ok() == copier::sha256_of(&first.source).ok() =>
            {
                preview.already_there += 1
            }
            Ok(_) => {
                preview.kept_count += 1;
                if preview.kept.len() < LIST_LIMIT {
                    preview.kept.push(first.rel.clone());
                }
            }
            Err(_) => {
                preview.to_copy += 1;
                preview.bytes += first.size;
                copies.push((first.source.clone(), dest));
            }
        }
    }
    Ok((preview, copies))
}

/// What the BIOS step would do for this installation (reads only).
pub fn preview(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
) -> Result<BiosPreview, String> {
    let target = target_for(connection, installation_id, presence)?;
    plan_into(connection, &target, presence).map(|(p, _)| p)
}

/// Copy the missing BIOS files (new files only; nothing replaced or deleted).
pub fn apply(
    connection: &Connection,
    installation_id: &str,
    presence: PresenceFn,
    progress: gamelists::ProgressFn,
) -> Result<String, String> {
    let target = target_for(connection, installation_id, presence)?;
    let (preview, copies) = plan_into(connection, &target, presence)?;
    if copies.is_empty() {
        return Err("Nothing to copy: the BIOS files from your library are already there (or a different file with the same name is kept).".into());
    }
    let results = gamelists::parallel_map(
        &copies,
        "copying",
        |(src, dst)| {
            if fs::symlink_metadata(dst).is_err() {
                gamelists::copy_image(src, dst)
            } else {
                Ok(())
            }
        },
        progress,
    );
    let failed: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    let copied = copies.len() - failed.len();
    let mut message = format!("Copied {copied} BIOS file(s) to {}.", preview.target);
    if preview.kept_count > 0 {
        message.push_str(&format!(
            " {} different file(s) already there were kept.",
            preview.kept_count
        ));
    }
    if let Some(first) = failed.first() {
        message.push_str(&format!(
            " {} couldn't be copied (first: {first}).",
            failed.len()
        ));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;
    use crate::{db, scanner};

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn locations_for_retropie_and_retroarch() {
        assert_eq!(
            bios_share(r"\\RETROPIE\roms\"),
            Some(PathBuf::from(r"\\RETROPIE\bios\"))
        );
        assert_eq!(bios_share(r"E:\retropie"), None);
        let dir = tempfile::tempdir().unwrap();
        let ra = dir.path().join("RetroArch");
        fs::create_dir_all(&ra).unwrap();
        assert_eq!(
            retroarch_system_dir(&ra),
            ra.join("system"),
            "no retroarch.cfg"
        );
        fs::write(
            ra.join("retroarch.cfg"),
            "video_driver = \"gl\"\nsystem_directory = \":\\\\bios\"\n",
        )
        .unwrap();
        assert_eq!(
            retroarch_system_dir(&ra),
            ra.join("bios"),
            ":\\ is RetroArch's own folder"
        );
        fs::write(ra.join("retroarch.cfg"), "system_directory = \"default\"\n").unwrap();
        assert_eq!(retroarch_system_dir(&ra), ra.join("system"));
        // Portable ES-DE: Emulators\RetroArch next to ES-DE.exe.
        let es = dir.path().join("ES-DE");
        fs::create_dir_all(es.join("ROMs")).unwrap();
        fs::write(es.join("ES-DE.exe"), b"").unwrap();
        write(&es, "Emulators/RetroArch/retroarch.exe", b"");
        assert_eq!(
            esde_retroarch(&es.join("ROMs")),
            Some(es.join("Emulators").join("RetroArch"))
        );
    }

    #[test]
    fn missing_bios_files_are_copied_and_nothing_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("library");
        write(&lib, "roms/bios/scph1001.bin", b"psx bios");
        write(&lib, "roms/bios/dc/dc_boot.bin", b"dc boot");
        write(&lib, "roms/bios/neogeo.zip", b"neogeo");
        write(&lib, "roms/bios/.gitkeep", b"");
        write(&lib, "roms II/bios/neogeo.zip", b"NEOGEO!");
        write(&lib, "roms II/bios/scph1001.bin", b"psx bios");
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &lib, &|_: &str| VolumeProbe::unknown()).unwrap();
        let target = dir.path().join("system");
        write(&target, "dc/dc_boot.bin", b"theirs!");
        let before = fs::read(target.join("dc/dc_boot.bin")).unwrap();
        let (p, copies) = plan_into(&catalog, &target, &connected).unwrap();
        assert_eq!(
            (
                p.to_copy,
                p.already_there,
                p.kept_count,
                p.two_versions_count
            ),
            (1, 0, 1, 1),
            "{p:?}"
        );
        assert_eq!(p.two_versions, ["neogeo.zip"]);
        assert_eq!(p.kept, ["dc/dc_boot.bin"]);
        for (src, dst) in &copies {
            gamelists::copy_image(src, dst).unwrap();
        }
        assert_eq!(fs::read(target.join("scph1001.bin")).unwrap(), b"psx bios");
        assert_eq!(
            fs::read(target.join("dc/dc_boot.bin")).unwrap(),
            before,
            "a different file already there is kept"
        );
        assert!(!target.join(".gitkeep").exists() && !target.join("neogeo.zip").exists());
        let (again, more) = plan_into(&catalog, &target, &connected).unwrap();
        assert_eq!(
            (again.to_copy, again.already_there, more.len()),
            (0, 1, 0),
            "nothing left to copy"
        );
    }
}
