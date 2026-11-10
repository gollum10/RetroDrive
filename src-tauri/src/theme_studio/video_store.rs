//! Your own video clips for themes (ADR-117). Clips are too large for the picture store, so
//! each is kept as one file in a `theme-videos` folder beside the catalog, named by its
//! SHA-256 (`<sha>.mp4`). Files are added, never replaced or changed; a clip that is already
//! there (same content) is reused. Only MP4 is accepted, checked by its header.
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_CLIP: u64 = 50 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 1024 * 1024 * 1024;

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredVideo {
    pub sha256: String,
    /// Portable declaration path (`assets/video/<sha>.mp4`), not a machine location.
    pub path: String,
    pub bytes: u64,
}

/// The clip folder for a catalog file (`…\retrodrive.sqlite` → `…\theme-videos`).
pub fn store_dir(catalog: &Path) -> PathBuf {
    catalog.parent().unwrap_or_else(|| Path::new(".")).join("theme-videos")
}

fn is_sha(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An MP4 starts with a box whose type is `ftyp` (ISO base media file format).
fn looks_like_mp4(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp"
}

/// Add a clip the user chose. Reads `source` only; writes one new file in the clip folder.
pub fn import(catalog: &Path, source: &Path) -> Result<StoredVideo, String> {
    let size = fs::metadata(source).map_err(|e| format!("Couldn't read that file: {e}"))?.len();
    if size == 0 || size > MAX_CLIP {
        return Err("Choose an MP4 clip up to 50 MB.".into());
    }
    let bytes = fs::read(source).map_err(|e| format!("Couldn't read that file: {e}"))?;
    if bytes.len() as u64 != size {
        return Err("The file changed while it was being read.".into());
    }
    if !looks_like_mp4(&bytes) {
        return Err("That file isn't an MP4 video. Save or convert it as MP4 first.".into());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let dir = store_dir(catalog);
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the clip folder: {e}"))?;
    let target = dir.join(format!("{sha256}.mp4"));
    if target.exists() {
        // Same name means same content, but check rather than trust it.
        if read(catalog, &sha256)?.len() as u64 != size {
            return Err("A stored clip is damaged; it was not replaced.".into());
        }
    } else {
        let used: u64 = fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();
        if used + size > MAX_TOTAL {
            return Err("Your theme clips have reached the 1 GB limit.".into());
        }
        let temp = dir.join(format!("{sha256}.mp4.part"));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("Couldn't store the clip: {e}"))?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| format!("Couldn't store the clip: {e}"))?;
        drop(file);
        crate::copier::rename_no_replace(&temp, &target).map_err(|e| format!("Couldn't store the clip: {e}"))?;
    }
    Ok(StoredVideo { path: format!("assets/video/{sha256}.mp4"), sha256, bytes: size })
}

/// Read a stored clip, checking its content still matches its name.
pub fn read(catalog: &Path, sha256: &str) -> Result<Vec<u8>, String> {
    if !is_sha(sha256) {
        return Err("Not a clip name.".into());
    }
    let bytes = fs::read(store_dir(catalog).join(format!("{sha256}.mp4"))).map_err(|_| "That clip isn't stored on this PC.".to_string())?;
    if bytes.len() as u64 > MAX_CLIP || format!("{:x}", Sha256::digest(&bytes)) != sha256 || !looks_like_mp4(&bytes) {
        return Err("A stored clip is damaged; it was not used.".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mp4(extra: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0, 0, 0, 24];
        bytes.extend_from_slice(b"ftypisom");
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(extra);
        bytes
    }

    #[test]
    fn clips_are_stored_once_by_content_and_checked_when_read() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = dir.path().join("retrodrive.sqlite");
        let source = dir.path().join("my clip.mp4");
        fs::write(&source, mp4(b"frames")).unwrap();
        let stored = import(&catalog, &source).unwrap();
        assert_eq!(stored.path, format!("assets/video/{}.mp4", stored.sha256));
        assert_eq!(read(&catalog, &stored.sha256).unwrap(), mp4(b"frames"));
        // Adding the same clip again reuses it.
        assert_eq!(import(&catalog, &source).unwrap(), stored);
        assert_eq!(fs::read_dir(store_dir(&catalog)).unwrap().count(), 1);
        // The source is only read.
        assert_eq!(fs::read(&source).unwrap(), mp4(b"frames"));
        // A damaged stored clip is refused, not used.
        fs::write(store_dir(&catalog).join(format!("{}.mp4", stored.sha256)), mp4(b"other")).unwrap();
        assert!(read(&catalog, &stored.sha256).is_err());
        assert!(import(&catalog, &source).is_err(), "a damaged clip isn't replaced");
    }

    #[test]
    fn only_mp4_clips_within_the_limit_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = dir.path().join("retrodrive.sqlite");
        let text = dir.path().join("not-video.mp4");
        fs::write(&text, b"hello, this is not a video at all").unwrap();
        assert!(import(&catalog, &text).unwrap_err().contains("isn't an MP4"));
        let empty = dir.path().join("empty.mp4");
        fs::write(&empty, b"").unwrap();
        assert!(import(&catalog, &empty).is_err());
        assert!(read(&catalog, "../secret").is_err() && read(&catalog, "abc").is_err());
        assert!(!store_dir(&catalog).exists() || fs::read_dir(store_dir(&catalog)).unwrap().count() == 0);
    }
}
