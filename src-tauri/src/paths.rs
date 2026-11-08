//! Path helpers: normalization, volume splitting and rebuilding.
//!
//! Catalog paths are stored as (volume root, relative path). The volume root is the
//! OS root of the path (`C:\`, `\\server\share\`, or `/`), and the relative path uses
//! `/` separators so it is independent of drive letters, which are not stable
//! identifiers (docs/05). Windows verbatim prefixes (`\\?\`) are never stored.

use std::path::{Component, Path, PathBuf};

/// Canonicalize without producing `\\?\` verbatim paths on Windows.
///
/// The prefix is removed even for paths longer than 260 characters: Rust's
/// standard library adds it back internally when it calls Windows, so long plain
/// paths still work, and the catalog always sees one spelling per volume (QA-008).
pub fn normalize(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    Ok(match canonical.to_str() {
        Some(text) => PathBuf::from(strip_verbatim(text)),
        // Non-Unicode names are rejected later by `split_volume`.
        None => canonical,
    })
}

/// Remove a Windows verbatim prefix from a stored string path (used for v0.1 data).
pub fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Split an absolute path into its volume root and a `/`-separated relative path.
/// Returns `None` for relative paths, `..` components, or non-Unicode names.
pub fn split_volume(path: &Path) -> Option<(String, String)> {
    let mut root = PathBuf::new();
    let mut rest: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => root.push(component.as_os_str()),
            Component::Normal(part) => rest.push(part.to_str()?),
            Component::CurDir => {}
            Component::ParentDir => return None,
        }
    }
    if root.as_os_str().is_empty() || !path.has_root() {
        return None;
    }
    Some((root.to_str()?.to_string(), rest.join("/")))
}

/// Rebuild a native path from a volume root and a stored relative path.
pub fn join(root: &str, relative: &str) -> PathBuf {
    let mut path = PathBuf::from(root);
    for part in relative.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefixes() {
        assert_eq!(strip_verbatim(r"\\?\C:\Roms\a.nes"), r"C:\Roms\a.nes");
        assert_eq!(strip_verbatim(r"\\?\UNC\nas\share\a.nes"), r"\\nas\share\a.nes");
        assert_eq!(strip_verbatim(r"C:\Roms\a.nes"), r"C:\Roms\a.nes");
    }

    #[test]
    fn rejects_relative_and_parent_paths() {
        assert_eq!(split_volume(Path::new("roms/a.nes")), None);
        #[cfg(windows)]
        assert_eq!(split_volume(Path::new(r"C:\roms\..\a.nes")), None);
        #[cfg(not(windows))]
        assert_eq!(split_volume(Path::new("/roms/../a.nes")), None);
    }

    #[cfg(windows)]
    #[test]
    fn splits_windows_volumes() {
        assert_eq!(
            split_volume(Path::new(r"C:\Roms\snes\Super Game.sfc")),
            Some((r"C:\".into(), "Roms/snes/Super Game.sfc".into()))
        );
        assert_eq!(
            split_volume(Path::new(r"\\nas\games\gba\x.gba")),
            Some((r"\\nas\games\".into(), "gba/x.gba".into()))
        );
        assert_eq!(join(r"C:\", "Roms/snes/Super Game.sfc"), PathBuf::from(r"C:\Roms\snes\Super Game.sfc"));
        assert_eq!(split_volume(Path::new(r"D:\")), Some((r"D:\".into(), String::new())));
    }

    #[cfg(windows)]
    #[test]
    fn normalize_has_no_verbatim_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let normalized = normalize(dir.path()).unwrap();
        assert!(!normalized.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn unicode_round_trip() {
        #[cfg(windows)]
        let original = PathBuf::from(r"E:\Spiele\Ünïcode – 日本\ゲーム (Japan).sfc");
        #[cfg(not(windows))]
        let original = PathBuf::from("/media/Spiele/Ünïcode – 日本/ゲーム (Japan).sfc");
        let (root, rel) = split_volume(&original).unwrap();
        assert_eq!(join(&root, &rel), original);
    }
}
