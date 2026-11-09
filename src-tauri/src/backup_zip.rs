//! Backups as one .zip file (owner 2026-11-09: "Export a backup as .zip"; ADR-096;
//! docs/18 §60), so a backup is easy to keep on a USB stick or move to another PC.
//!
//! **Save as .zip** reads a finished backup folder and writes one new `.zip` next to it
//! in the backup folder. Every file is checked against the backup's list
//! (`manifest.json`, SHA-256) while it's packed, and the finished zip is read back and
//! checked again before it gets its name. It's written under a temporary name and named
//! without replacing anything; a zip that fails is our own temporary file and is removed.
//! The backup folder itself is only read.
//!
//! **Add a backup from a .zip** unpacks a zip made this way into a new backup folder.
//! Only the files on the zip's list are unpacked, only to the places that kind of
//! backup holds ([`system_backup::covered`]), each to a new file that must match its
//! size and SHA-256; anything else in the zip is ignored. Like a backup run, the folder
//! is called "(incomplete)" until every file is in, and is never deleted.
//! Nothing outside the backup folder is written, and the system isn't touched: restoring
//! from the unpacked backup is the normal restore, with its preview.

use crate::system_backup::{self, Manifest, BackupEntry, FORMAT, MANIFEST};
use crate::{copier, db, gamelists};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The biggest backup list read from a zip (a list of 100,000 files is about 30 MB).
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ZipSaved {
    pub path: String,
    pub name: String,
    pub files: usize,
    /// Size of the zip file.
    pub bytes: u64,
}

/// The name a file has inside the zip: the same as in the backup folder.
fn entry_name(f: &system_backup::ManifestFile) -> String {
    format!("{}/{}", f.area.folder(), f.path)
}

/// Copy `input` to `out`, at most `limit` bytes plus one (so a longer input is noticed),
/// returning the bytes copied and their SHA-256.
fn copy_hashed(input: &mut impl Read, out: &mut impl Write, limit: u64) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut total = 0u64;
    let mut input = input.take(limit.saturating_add(1));
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
        total += n as u64;
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

/// `<name>.zip`, or `<name> (2).zip` and so on if that's taken.
fn free_zip_name(destination: &Path, name: &str) -> Result<String, String> {
    (1..1000)
        .map(|n| if n == 1 { format!("{name}.zip") } else { format!("{name} ({n}).zip") })
        .find(|c| fs::symlink_metadata(destination.join(c)).is_err())
        .ok_or_else(|| "Couldn't find a free name for the .zip file.".into())
}

/// Save a finished backup folder as one .zip file in `destination`.
pub fn save(folder: &Path, destination: &Path, progress: gamelists::ProgressFn) -> Result<ZipSaved, String> {
    let manifest_text = fs::read(folder.join(MANIFEST)).map_err(|_| "This folder has no readable backup list (manifest.json).".to_string())?;
    let m: Manifest = serde_json::from_slice(&manifest_text).map_err(|_| "This backup's list of files (manifest.json) can't be read.".to_string())?;
    if !m.complete || m.format != FORMAT {
        return Err("Only a complete backup can be saved as a .zip.".into());
    }
    let bytes: u64 = m.files.iter().map(|f| f.size).sum();
    if let Some(space) = crate::volumes::space(destination) {
        if space.free_bytes < bytes + bytes / 20 + 16 * 1024 * 1024 {
            return Err(format!("Not enough space in {}: the .zip may need up to {} MB.", destination.display(), bytes / (1024 * 1024) + 1));
        }
    }
    let base = folder.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or("That isn't a backup folder.")?;
    let name = free_zip_name(destination, &base)?;
    let target = destination.join(&name);
    let marker = format!("{}{}", copier::TEMP_MARKER, &db::new_id()[..8]);
    let temp = destination.join(format!("{name}{marker}"));
    let fail = |m: String| -> Result<ZipSaved, String> {
        copier::remove_own_temp(&temp, &marker);
        Err(m)
    };
    let total = m.files.len();

    // 1. Pack, checking each file against the list as it's read.
    let written = (|| -> Result<(), String> {
        let out = OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("Couldn't create the .zip file: {e}"))?;
        let mut zip = ZipWriter::new(io::BufWriter::new(out));
        let packed = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (i, f) in m.files.iter().enumerate() {
            progress("packing", i, total);
            let path = crate::paths::join(&folder.join(f.area.folder()).display().to_string(), &f.path);
            let mut input = File::open(&path).map_err(|e| format!("Couldn't read {} from the backup ({e}). Use Check to see what's missing.", f.path))?;
            zip.start_file(entry_name(f), packed.large_file(f.size >= u32::MAX as u64)).map_err(|e| format!("Couldn't write the .zip file: {e}"))?;
            let (size, sha) = copy_hashed(&mut input, &mut zip, f.size).map_err(|e| format!("Couldn't pack {}: {e}", f.path))?;
            if size != f.size || sha != f.sha256 {
                return Err(format!("{} changed since the backup was made, so no .zip was saved. Use Check to see what changed.", f.path));
            }
        }
        zip.start_file(MANIFEST, packed).map_err(|e| format!("Couldn't write the .zip file: {e}"))?;
        zip.write_all(&manifest_text).map_err(|e| format!("Couldn't write the .zip file: {e}"))?;
        let out = zip.finish().map_err(|e| format!("Couldn't finish the .zip file: {e}"))?;
        let out = out.into_inner().map_err(|e| format!("Couldn't finish the .zip file: {}", e.error()))?;
        out.sync_all().map_err(|e| format!("Couldn't finish the .zip file: {e}"))
    })();
    if let Err(e) = written {
        return fail(e);
    }

    // 2. Read the zip back and check every file again.
    let checked = (|| -> Result<(), String> {
        let mut archive = ZipArchive::new(File::open(&temp).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for (i, f) in m.files.iter().enumerate() {
            progress("checking", i, total);
            let mut entry = archive.by_name(&entry_name(f)).map_err(|e| format!("{}: {e}", f.path))?;
            let (size, sha) = copy_hashed(&mut entry, &mut io::sink(), f.size).map_err(|e| format!("{}: {e}", f.path))?;
            if size != f.size || sha != f.sha256 {
                return Err(format!("{} didn't match", f.path));
            }
        }
        Ok(())
    })();
    if let Err(e) = checked {
        return fail(format!("The .zip didn't check out ({e}), so it wasn't kept."));
    }
    progress("checking", total, total);
    if let Err(e) = copier::rename_no_replace(&temp, &target) {
        return fail(format!("Couldn't name the .zip file: {e}"));
    }
    let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    Ok(ZipSaved { path: target.display().to_string(), name, files: total, bytes: size })
}

/// Read and check a zip's backup list.
fn zip_manifest<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<(Vec<u8>, Manifest), String> {
    let not_backup = "This .zip isn't a RetroDrive backup (it has no backup list).";
    let entry = archive.by_name(MANIFEST).map_err(|_| not_backup.to_string())?;
    let mut text = Vec::new();
    entry.take(MAX_MANIFEST + 1).read_to_end(&mut text).map_err(|e| format!("Couldn't read the .zip file: {e}"))?;
    if text.len() as u64 > MAX_MANIFEST {
        return Err(not_backup.into());
    }
    let m: Manifest = serde_json::from_slice(&text).map_err(|_| "This .zip's backup list can't be read; it may come from a newer RetroDrive.".to_string())?;
    if m.format != FORMAT {
        return Err("This backup comes from a different version of RetroDrive and can't be added.".into());
    }
    if !m.complete {
        return Err("This .zip holds a backup that didn't finish, so it can't be added.".into());
    }
    let mut seen = std::collections::HashSet::new();
    for f in &m.files {
        if !system_backup::covered(&m.adapter_id, f.area, &f.path) {
            return Err(format!("This .zip lists a file RetroDrive doesn't back up ({}), so nothing was added.", f.path));
        }
        if !seen.insert((f.area, f.path.to_lowercase())) {
            return Err(format!("This .zip lists {} twice, so nothing was added.", f.path));
        }
    }
    Ok((text, m))
}

/// Unpack one file to a new file, checking its size and SHA-256.
fn unpack(entry: &mut impl Read, target: &Path, f: &system_backup::ManifestFile) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
    }
    let marker = format!("{}{}", copier::TEMP_MARKER, &db::new_id()[..8]);
    let temp = PathBuf::from(format!("{}{marker}", target.display()));
    let fail = |m: String| -> Result<(), String> {
        copier::remove_own_temp(&temp, &marker);
        Err(m)
    };
    let copied = (|| -> io::Result<(u64, String)> {
        let mut out = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        let r = copy_hashed(entry, &mut out, f.size)?;
        out.sync_all()?;
        Ok(r)
    })();
    match copied {
        Err(e) => fail(format!("Couldn't unpack {}: {e}", f.path)),
        Ok((size, sha)) if size != f.size || sha != f.sha256 => fail(format!("{} in the .zip doesn't match its list", f.path)),
        Ok(_) => copier::rename_no_replace(&temp, target).or_else(|e| fail(format!("Couldn't name {}: {e}", f.path))),
    }
}

/// Add a backup from a .zip: unpack it into a new folder in `destination`.
pub fn add(zip_path: &Path, destination: &Path, progress: gamelists::ProgressFn) -> Result<BackupEntry, String> {
    let file = File::open(zip_path).map_err(|e| format!("Couldn't open {}: {e}", zip_path.display()))?;
    let mut archive = ZipArchive::new(io::BufReader::new(file)).map_err(|_| "This file isn't a .zip file RetroDrive can read.".to_string())?;
    let (manifest_text, m) = zip_manifest(&mut archive)?;
    for f in &m.files {
        if archive.index_for_name(&entry_name(f)).is_none() {
            return Err(format!("{} is on the backup's list but isn't in the .zip, so nothing was added.", f.path));
        }
    }
    let bytes: u64 = m.files.iter().map(|f| f.size).sum();
    fs::create_dir_all(destination).map_err(|e| format!("Couldn't create {}: {e}", destination.display()))?;
    if let Some(space) = crate::volumes::space(destination) {
        if space.free_bytes < bytes + bytes / 20 + 16 * 1024 * 1024 {
            return Err(format!("Not enough space in {}: the backup needs about {} MB.", destination.display(), bytes / (1024 * 1024) + 1));
        }
    }
    let stem = zip_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = system_backup::safe_name(&stem);
    let base = if stem.is_empty() { system_backup::safe_name(&format!("{} backup", m.adapter_name)) } else { stem };
    let name = system_backup::free_folder(destination, &base)?;
    let working = destination.join(format!("{name}{}", system_backup::INCOMPLETE));
    fs::create_dir(&working).map_err(|e| format!("Couldn't create the backup folder: {e}"))?;
    let total = m.files.len();
    for (i, f) in m.files.iter().enumerate() {
        progress("unpacking", i, total);
        let target = crate::paths::join(&working.join(f.area.folder()).display().to_string(), &f.path);
        let mut entry = archive.by_name(&entry_name(f)).map_err(|e| format!("Couldn't read {} from the .zip: {e}", f.path))?;
        unpack(&mut entry, &target, f).map_err(|e| format!("{e}. The unpacked part is kept as \"{name}{}\".", system_backup::INCOMPLETE))?;
    }
    progress("unpacking", total, total);
    system_backup::write_manifest_bytes(&working, &manifest_text)?;
    let done = destination.join(&name);
    copier::rename_no_replace(&working, &done).map_err(|e| format!("The backup was added but couldn't be renamed: {e}. It's in {}.", working.display()))?;
    system_backup::entry(&done).ok_or_else(|| "The backup was added but can't be listed.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_backup::{Area, ManifestFile};

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = crate::paths::join(&root.display().to_string(), rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn file(area: Area, path: &str, bytes: &[u8]) -> ManifestFile {
        ManifestFile { group: "saves".into(), area, path: path.into(), size: bytes.len() as u64, sha256: sha(bytes), modified: None }
    }
    fn manifest(files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            format: FORMAT,
            app: "RetroDrive test".into(),
            created_at: "2026-11-09T20:00:00".into(),
            adapter_id: "retrobat".into(),
            adapter_name: "RetroBat".into(),
            version: None,
            installation_id: "inst-1".into(),
            location: r"G:\RetroBat".into(),
            groups: vec!["saves".into(), "settings".into()],
            complete: true,
            problems: Vec::new(),
            files,
        }
    }
    /// A finished backup folder named `name` in `dir`.
    fn backup(dir: &Path, name: &str) -> PathBuf {
        let folder = dir.join(name);
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let files = [("saves/snes/Mario.srm", b"save data".to_vec()), ("saves/psx/Big.mcd", big), ("retrobat.ini", b"[Main]\n".to_vec())];
        let mut list = Vec::new();
        for (path, bytes) in files {
            put(&folder.join("install"), path, &bytes);
            list.push(file(Area::Install, path, &bytes));
        }
        fs::write(folder.join(MANIFEST), serde_json::to_string_pretty(&manifest(list)).unwrap()).unwrap();
        folder
    }
    fn listing(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<_> = walkdir::WalkDir::new(root).into_iter().map(|e| e.unwrap()).filter(|e| e.file_type().is_file()).map(|e| (e.path().strip_prefix(root).unwrap().display().to_string(), fs::read(e.path()).unwrap())).collect();
        out.sort();
        out
    }
    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }
    fn quiet(_: &str, _: usize, _: usize) {}
    /// A zip with these (name, bytes) entries.
    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn a_backup_saved_as_zip_and_added_back_is_identical_and_the_original_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let folder = backup(dir.path(), "RetroBat (G) 2026-11-09 20.00");
        let before = listing(&folder);
        let saved = save(&folder, dir.path(), &quiet).unwrap();
        assert_eq!(saved.name, "RetroBat (G) 2026-11-09 20.00.zip");
        assert_eq!(saved.files, 3);
        assert_eq!(listing(&folder), before, "the backup folder is only read");
        assert!(!names(dir.path()).iter().any(|n| n.contains(copier::TEMP_MARKER)));

        // Saving again never replaces the first zip.
        let again = save(&folder, dir.path(), &quiet).unwrap();
        assert_eq!(again.name, "RetroBat (G) 2026-11-09 20.00 (2).zip");

        // Added on "another PC": a different backup folder.
        let other = tempfile::tempdir().unwrap();
        let added = add(Path::new(&saved.path), other.path(), &quiet).unwrap();
        assert_eq!(added.name, "RetroBat (G) 2026-11-09 20.00");
        assert!(added.complete);
        assert_eq!((added.files, added.system.as_deref()), (3, Some("RetroBat")));
        assert_eq!(listing(&other.path().join(&added.name)), before);
        assert_eq!(system_backup::check(&other.path().join(&added.name)).unwrap().ok, 3);

        // Adding it next to the original gets a new name.
        let beside = add(Path::new(&saved.path), dir.path(), &quiet).unwrap();
        assert_eq!(beside.name, "RetroBat (G) 2026-11-09 20.00 (2)");
        assert_eq!(listing(&folder), before);
    }

    #[test]
    fn a_backup_that_changed_or_did_not_finish_is_not_zipped() {
        let dir = tempfile::tempdir().unwrap();
        let folder = backup(dir.path(), "RetroBat 1");
        fs::write(folder.join("install/saves/snes/Mario.srm"), b"edited!!!").unwrap();
        let e = save(&folder, dir.path(), &quiet).unwrap_err();
        assert!(e.contains("changed since the backup"), "{e}");
        assert_eq!(names(dir.path()), vec!["RetroBat 1"], "no zip or temporary file is left");

        let unfinished = dir.path().join("RetroBat 2 (incomplete)");
        fs::create_dir(&unfinished).unwrap();
        let mut m = manifest(Vec::new());
        m.complete = false;
        fs::write(unfinished.join(MANIFEST), serde_json::to_string(&m).unwrap()).unwrap();
        assert!(save(&unfinished, dir.path(), &quiet).unwrap_err().contains("complete backup"));
    }

    #[test]
    fn only_listed_files_are_unpacked_and_only_to_places_a_backup_holds() {
        let dir = tempfile::tempdir().unwrap();
        let ok = file(Area::Install, "saves/snes/Mario.srm", b"save");
        let text = serde_json::to_vec(&manifest(vec![ok.clone()])).unwrap();
        // Extra entries (a program, a path escaping the folder) are ignored.
        let zip = dir.path().join("extra.zip");
        make_zip(&zip, &[(MANIFEST, &text), ("install/saves/snes/Mario.srm", b"save"), ("../evil.exe", b"MZ"), ("install/RetroBat.exe", b"MZ")]);
        let out = dir.path().join("backups");
        let added = add(&zip, &out, &quiet).unwrap();
        assert_eq!(listing(&out.join(&added.name)).iter().map(|(p, _)| p.replace('\\', "/")).collect::<Vec<_>>(), vec!["install/saves/snes/Mario.srm", "manifest.json"]);
        assert!(!dir.path().join("evil.exe").exists());

        // A list naming a place outside what backups hold is refused before anything is written.
        for bad in ["../outside.srm", "emulators/retroarch/retroarch.exe", "saves/.retrodrive/x"] {
            let f = file(Area::Install, bad, b"x");
            let text = serde_json::to_vec(&manifest(vec![f])).unwrap();
            let z = dir.path().join("bad.zip");
            let _ = fs::remove_file(&z);
            make_zip(&z, &[(MANIFEST, &text), (&format!("install/{bad}"), b"x")]);
            let fresh = dir.path().join("fresh");
            assert!(add(&z, &fresh, &quiet).is_err(), "{bad}");
            assert!(!fresh.exists() || names(&fresh).is_empty(), "{bad}");
        }
    }

    #[test]
    fn a_damaged_zip_adds_nothing_complete() {
        let dir = tempfile::tempdir().unwrap();
        let f = file(Area::Install, "saves/snes/Mario.srm", b"save");
        let text = serde_json::to_vec(&manifest(vec![f])).unwrap();

        // Contents not matching the list: kept as "(incomplete)", never listed as complete.
        let z = dir.path().join("changed.zip");
        make_zip(&z, &[(MANIFEST, &text), ("install/saves/snes/Mario.srm", b"SAVE")]);
        let out = dir.path().join("b1");
        assert!(add(&z, &out, &quiet).unwrap_err().contains("doesn't match"));
        let listed = system_backup::list(&out);
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].complete && listed[0].name.ends_with("(incomplete)"));

        // A listed file missing from the zip: nothing is created.
        let z = dir.path().join("missing.zip");
        make_zip(&z, &[(MANIFEST, &text)]);
        let out = dir.path().join("b2");
        assert!(add(&z, &out, &quiet).unwrap_err().contains("isn't in the .zip"));
        assert!(!out.exists());

        // Not a backup, or not a zip at all.
        let z = dir.path().join("photos.zip");
        make_zip(&z, &[("photo.jpg", b"jpg")]);
        assert!(add(&z, &dir.path().join("b3"), &quiet).unwrap_err().contains("isn't a RetroDrive backup"));
        let z = dir.path().join("text.zip");
        fs::write(&z, b"not a zip").unwrap();
        assert!(add(&z, &dir.path().join("b4"), &quiet).unwrap_err().contains("isn't a .zip"));
    }
}
