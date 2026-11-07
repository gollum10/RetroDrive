//! Light guns D (RDS-028; owner 2026-11-07 "then do D"; ADR-067; docs/18 §40): copy
//! the Sinden software the user downloaded into RetroBat's `system\tools\sinden`
//! folder, where RetroBat looks for it (RetroBat wiki, Lightguns). RetroDrive never
//! downloads it. New files only, each copied under a temporary name, checked, then
//! renamed without replacing (`gamelists::copy_image`); files already there are kept.

use crate::customizer::{self, PresenceFn};
use crate::{copier, gamelists, installations};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Where RetroBat looks for the Sinden software: `system\tools\sinden` in the RetroBat folder.
pub fn sinden_dir(root: &Path) -> PathBuf {
    root.join("system").join("tools").join("sinden")
}
const PROGRAM: &str = "Lightgun.exe";
/// A Sinden download is a few dozen files; anything far bigger is the wrong folder.
const MAX_FILES: usize = 3000;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const LIST_LIMIT: usize = 5;

/// Whether each RetroBat installation already has the Sinden software.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SindenStatus {
    pub installation_id: String,
    pub location: String,
    pub target: String,
    pub installed: bool,
    /// Why it can't be checked (e.g. not connected).
    pub problem: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SindenPreview {
    pub source: String,
    pub target: String,
    pub to_copy: usize,
    pub bytes: u64,
    pub already_there: usize,
    /// Different files with the same name already there (kept), first few.
    pub kept: Vec<String>,
    pub kept_count: usize,
}

fn has_program(dir: &Path) -> bool {
    fs::read_dir(dir).map(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(PROGRAM) && e.path().is_file())).unwrap_or(false)
}

/// The Sinden status of every RetroBat installation (reads only).
pub fn status(connection: &Connection, presence: PresenceFn) -> Result<Vec<SindenStatus>, String> {
    Ok(installations::located(connection, presence, None)?
        .into_iter()
        .filter(|l| l.adapter_id == "retrobat")
        .map(|l| {
            let target = sinden_dir(&l.root);
            let connected = l.presence == crate::volumes::Presence::Connected;
            SindenStatus {
                installation_id: l.id,
                location: l.root.display().to_string(),
                target: target.display().to_string(),
                installed: connected && has_program(&target),
                problem: (!connected).then(|| "Not connected.".to_string()),
            }
        })
        .collect())
}

/// Files below `dir` (relative paths with `/`), not following links; `Err` when too many or too big.
fn files_below(dir: &Path) -> Result<Vec<(String, PathBuf, u64)>, String> {
    let mut out = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((folder, prefix)) = stack.pop() {
        let entries = fs::read_dir(&folder).map_err(|e| format!("Couldn't read {}: {e}", folder.display()))?;
        for entry in entries.flatten() {
            let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if meta.file_type().is_symlink() {
                continue;
            } else if meta.is_dir() {
                stack.push((entry.path(), rel));
            } else if meta.is_file() {
                total += meta.len();
                out.push((rel, entry.path(), meta.len()));
                if out.len() > MAX_FILES || total > MAX_BYTES {
                    return Err("That folder is much bigger than the Sinden software, so nothing was copied. Choose the folder that has Lightgun.exe in it.".into());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

fn same_place(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a.starts_with(&b) || b.starts_with(&a),
        _ => false,
    }
}

/// What copying `source` into `target` would do, and the copies.
fn plan(source: &Path, target: &Path) -> Result<(SindenPreview, Vec<(PathBuf, PathBuf)>), String> {
    if !source.is_dir() || !has_program(source) {
        return Err("That folder doesn't have Lightgun.exe in it. Choose the folder you unpacked the Sinden software into.".into());
    }
    if same_place(source, target) {
        return Err("That's RetroBat's own Sinden folder (or a folder inside it). Choose the folder you downloaded the Sinden software to.".into());
    }
    let mut preview = SindenPreview { source: source.display().to_string(), target: target.display().to_string(), to_copy: 0, bytes: 0, already_there: 0, kept: Vec::new(), kept_count: 0 };
    let mut copies = Vec::new();
    for (rel, src, size) in files_below(source)? {
        let dest = rel.split('/').fold(target.to_path_buf(), |p, part| p.join(part));
        match fs::symlink_metadata(&dest) {
            Ok(m) if m.is_file() && m.len() == size && copier::sha256_of(&dest).ok() == copier::sha256_of(&src).ok() => preview.already_there += 1,
            Ok(_) => {
                preview.kept_count += 1;
                if preview.kept.len() < LIST_LIMIT {
                    preview.kept.push(rel);
                }
            }
            Err(_) => {
                preview.to_copy += 1;
                preview.bytes += size;
                copies.push((src, dest));
            }
        }
    }
    Ok((preview, copies))
}

fn target_for(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<PathBuf, String> {
    let install = customizer::install_for(connection, installation_id, presence, &["retrobat"], "The Sinden software folder is a RetroBat feature.")?;
    if !install.root.join("system").is_dir() {
        return Err("This RetroBat folder has no system folder. Start RetroBat once, then try again.".into());
    }
    Ok(sinden_dir(&install.root))
}

/// What copying the Sinden software from `source` would do (reads only).
pub fn preview(connection: &Connection, installation_id: &str, source: &Path, presence: PresenceFn) -> Result<SindenPreview, String> {
    plan(source, &target_for(connection, installation_id, presence)?).map(|(p, _)| p)
}

/// Copy the Sinden software's missing files (new files only; nothing replaced or deleted).
pub fn apply(connection: &Connection, installation_id: &str, source: &Path, presence: PresenceFn, progress: gamelists::ProgressFn) -> Result<String, String> {
    let target = target_for(connection, installation_id, presence)?;
    let (preview, copies) = plan(source, &target)?;
    if copies.is_empty() {
        return Err("Nothing to copy: the Sinden software is already there.".into());
    }
    let results = gamelists::parallel_map(&copies, "copying", |(src, dst)| if fs::symlink_metadata(dst).is_err() { gamelists::copy_image(src, dst) } else { Ok(()) }, progress);
    let failed: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    let mut message = format!("Copied {} file(s) of the Sinden software to {}. RetroBat starts it when you play a light gun game.", copies.len() - failed.len(), preview.target);
    if preview.kept_count > 0 {
        message.push_str(&format!(" {} different file(s) already there were kept.", preview.kept_count));
    }
    if let Some(first) = failed.first() {
        message.push_str(&format!(" {} couldn't be copied (first: {first}).", failed.len()));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::volumes::{Presence, VolumeProbe};

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn the_sinden_software_is_copied_into_retrobat_without_replacing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let download = dir.path().join("SindenLightgunSoftware");
        write(&download, "Lightgun.exe", b"program");
        write(&download, "Lightgun.exe.config", b"<config/>");
        write(&download, "lib/AForge.dll", b"dll");
        let rb = dir.path().join("RetroBat");
        write(&rb, "retrobat.exe", b"x");
        fs::create_dir_all(rb.join("roms/snes")).unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        let id = installations::add(&c, &rb, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let connected = |_: &str, _: Option<&str>| Presence::Connected;

        assert!(preview(&c, &id, &download, &connected).unwrap_err().contains("system folder"), "RetroBat never started");
        fs::create_dir_all(rb.join("system")).unwrap();
        assert!(!status(&c, &connected).unwrap()[0].installed);
        // The wrong folder: no Lightgun.exe.
        assert!(preview(&c, &id, &rb, &connected).unwrap_err().contains("Lightgun.exe"));
        // A different file already there is kept.
        write(&rb, "system/tools/sinden/Lightgun.exe.config", b"<mine/>");
        let p = preview(&c, &id, &download, &connected).unwrap();
        assert_eq!((p.to_copy, p.already_there, p.kept_count), (2, 0, 1), "{p:?}");
        let message = apply(&c, &id, &download, &connected, &|_, _, _| {}).unwrap();
        assert!(message.contains("Copied 2 file(s)") && message.contains("kept"), "{message}");
        assert_eq!(fs::read(rb.join("system/tools/sinden/lib/AForge.dll")).unwrap(), b"dll");
        assert_eq!(fs::read(rb.join("system/tools/sinden/Lightgun.exe.config")).unwrap(), b"<mine/>", "not replaced");
        assert!(status(&c, &connected).unwrap()[0].installed);
        assert!(apply(&c, &id, &download, &connected, &|_, _, _| {}).unwrap_err().contains("already there"));
        // Choosing RetroBat's own Sinden folder as the source is refused.
        assert!(preview(&c, &id, &sinden_dir(&rb), &connected).unwrap_err().contains("own Sinden folder"));
    }
}

