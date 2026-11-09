//! Setup wizard, "Find your system" (owner 2026-11-09; ADR-100): look for emulation
//! systems the user could add, so they don't have to know where a folder is.
//!
//! Reads only: it lists folders and asks each adapter whether it recognises one
//! (`adapters::detect_all`, the same check as **Add existing installation**). Nothing
//! is written, opened for writing, or added; the user picks what to add.
//!
//! - **Drives:** each connected drive's top folder, the folders in it, and the folders
//!   one level further down (e.g. `G:\RetroBat`, `D:\Games\RetroBat`, `E:\ES-DE`),
//!   skipping Windows' own folders. A capped number of folders per level keeps it quick.
//! - **Network:** the names the systems use for their shares (`\\BATOCERA\share` and so
//!   on). A name nobody answers can take Windows a long time to give up on, so each is
//!   checked on its own thread and anything that hasn't answered by the deadline is left
//!   out (the user can still pick it with Browse…).

use crate::adapters::{self, Confidence};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    pub adapter_id: String,
    pub name: String,
    pub version: Option<String>,
    /// Already saved in Emulation Systems.
    pub added: bool,
}

/// The share names each system uses on the network (as in Add existing installation).
pub const NETWORK_NAMES: &[&str] = &[r"\\BATOCERA\share", r"\\KNULLI\share", r"\\RECALBOX\share", r"\\RETROPIE\roms", r"\\ROCKNIX\games-roms", r"\\LAKKA\ROMs"];

/// Folders that never hold an emulation system.
const SKIP: &[&str] = &["windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information", "recovery", "$windows.~bt", "$windows.~ws", "msocache", "perflogs", "config.msi", "appdata", "node_modules", ".git"];
/// Folders looked at per level (a drive with thousands of game folders stays quick).
const PER_LEVEL: usize = 400;

fn key(path: &Path) -> String {
    path.display().to_string().trim_end_matches(['\\', '/']).to_lowercase()
}

fn detect(dir: &Path, added: &HashSet<String>) -> Option<Found> {
    let d = adapters::detect_all(dir).into_iter().find(|d| d.confidence == Confidence::Detected)?;
    Some(Found { path: dir.display().to_string(), adapter_id: d.adapter_id.into(), name: d.display_name.into(), version: d.version, added: added.contains(&key(dir)) })
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        // Real folders only: links and junctions aren't followed.
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            !SKIP.contains(&name.as_str()) && !name.starts_with('.')
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out.truncate(PER_LEVEL);
    out
}

/// Look on these drive roots, at most two folders deep, plus `extra` folders checked
/// directly (an installed ES-DE's `%USERPROFILE%\ROMs`). `added` are the saved
/// installations' paths.
pub fn on_drives(roots: &[PathBuf], extra: &[PathBuf], added: &[String]) -> Vec<Found> {
    let added: HashSet<String> = added.iter().map(|p| key(Path::new(p))).collect();
    let mut found: Vec<Found> = extra.iter().filter_map(|d| detect(d, &added)).collect();
    for root in roots {
        if let Some(f) = detect(root, &added) {
            found.push(f);
            continue;
        }
        for one in subfolders(root) {
            if let Some(f) = detect(&one, &added) {
                found.push(f);
                continue;
            }
            for two in subfolders(&one) {
                if let Some(f) = detect(&two, &added) {
                    found.push(f);
                }
            }
        }
    }
    let mut seen = HashSet::new();
    found.retain(|f| seen.insert(key(Path::new(&f.path))));
    found
}

/// Ask each network name in parallel; whatever hasn't answered by `deadline` is left out.
pub fn on_network(names: &[&str], added: &[String], deadline: Duration) -> Vec<Found> {
    let added: HashSet<String> = added.iter().map(|p| key(Path::new(p))).collect();
    let (tx, rx) = mpsc::channel();
    for (i, name) in names.iter().enumerate() {
        let tx = tx.clone();
        let path = PathBuf::from(name);
        let added = added.clone();
        // A thread per name: Windows can take a long time on a name nobody answers.
        std::thread::spawn(move || {
            let f = if path.is_dir() { detect(&path, &added) } else { None };
            let _ = tx.send((i, f));
        });
    }
    drop(tx);
    let end = Instant::now() + deadline;
    let mut found: Vec<(usize, Found)> = Vec::new();
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok((i, Some(f))) => found.push((i, f)),
            Ok((_, None)) => {}
            Err(_) => break,
        }
    }
    found.sort_by_key(|(i, _)| *i);
    found.into_iter().map(|(_, f)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retrobat(root: &Path) {
        fs::create_dir_all(root.join("emulationstation")).unwrap();
        fs::create_dir_all(root.join("roms/snes")).unwrap();
        fs::write(root.join("RetroBat.exe"), b"MZ").unwrap();
        fs::write(root.join("retrobat.ini"), b"[Main]\n").unwrap();
    }
    fn listing(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = walkdir::WalkDir::new(root).into_iter().map(Result::unwrap).map(|e| (e.path().display().to_string(), if e.file_type().is_file() { fs::read(e.path()).unwrap() } else { Vec::new() })).collect();
        v.sort();
        v
    }

    #[test]
    fn systems_up_to_two_folders_deep_are_found_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        retrobat(&d.join("RetroBat"));
        retrobat(&d.join("Games/RetroBat 7"));
        retrobat(&d.join("a/b/c/TooDeep"));
        retrobat(&d.join("Windows/RetroBat"));
        fs::create_dir_all(d.join("Photos/2024")).unwrap();
        let before = listing(d);
        let added = vec![d.join("RetroBat").display().to_string() + "\\"];
        let found = on_drives(&[d.to_path_buf()], &[], &added);
        let paths: Vec<String> = found.iter().map(|f| f.path.strip_prefix(&d.display().to_string()).unwrap().replace('/', "\\")).collect();
        assert_eq!(paths, [r"\Games\RetroBat 7", r"\RetroBat"]);
        assert!(found.iter().all(|f| f.adapter_id == "retrobat" && f.name == "RetroBat"));
        assert_eq!(found.iter().map(|f| f.added).collect::<Vec<_>>(), [false, true], "already added is marked");
        assert_eq!(listing(d), before, "reads only");
    }

    #[test]
    fn a_system_at_the_drive_root_is_found_without_looking_further() {
        let dir = tempfile::tempdir().unwrap();
        retrobat(dir.path());
        retrobat(&dir.path().join("Copy"));
        let found = on_drives(&[dir.path().to_path_buf()], &[dir.path().join("Copy"), dir.path().to_path_buf()], &[]);
        assert_eq!(found.len(), 2, "the extra folder is checked directly, and nothing is listed twice");
    }

    #[test]
    fn network_names_that_do_not_answer_are_left_out_by_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        retrobat(&dir.path().join("share"));
        let here = dir.path().join("share").display().to_string();
        let missing = dir.path().join("nothing").display().to_string();
        let t = Instant::now();
        let found = on_network(&[missing.as_str(), here.as_str()], &[], Duration::from_secs(5));
        assert_eq!(found.len(), 1);
        assert!(t.elapsed() < Duration::from_secs(5), "returns as soon as every name answered");
    }
}
