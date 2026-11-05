//! Volume identity (RDS-003, docs/05 "Drive identifiers").
//!
//! Drive letters are not stable: the same USB drive can come back as `F:` and a
//! different drive can take `E:`. Each volume is therefore identified by an
//! OS-visible fingerprint, stored in `storage_locations.stable_id`:
//! - local / removable volumes: `vol:<serial>:<total bytes>`, from the filesystem's
//!   volume serial number (changes only when the volume is reformatted) plus its size.
//! - network shares: `unc:<\\server\share\>` (lower-case); the share path itself is stable.
//!
//! Known limitation: two byte-for-byte clones of the same volume share a fingerprint.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeIdentity {
    pub stable_id: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub serial: Option<String>,
    pub guid: Option<String>,
    pub total_bytes: Option<i64>,
}

/// Everything the catalog needs to know about the volume holding a scanned folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeProbe {
    /// `local`, `removable`, `network` or `unknown`.
    pub kind: &'static str,
    pub identity: Option<VolumeIdentity>,
}

impl VolumeProbe {
    #[cfg(test)]
    pub fn unknown() -> Self {
        Self { kind: "unknown", identity: None }
    }
}

/// Probe the volume mounted at `root` (e.g. `E:\` or `\\nas\games\`).
pub fn probe(root: &str) -> VolumeProbe {
    if root.starts_with(r"\\") {
        return VolumeProbe {
            kind: "network",
            identity: Some(VolumeIdentity {
                stable_id: format!("unc:{}", root.to_lowercase()),
                label: None,
                filesystem: None,
                serial: None,
                guid: None,
                total_bytes: None,
            }),
        };
    }
    VolumeProbe { kind: crate::drives::volume_kind(root), identity: identity(root) }
}

/// Whether a catalog storage location is currently attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// Reachable at its last known root and (when known) the fingerprint matches.
    Connected,
    /// Nothing reachable at its last known root.
    NotConnected,
    /// Its last known drive letter now holds a different volume.
    OtherVolume,
}

/// Check whether the volume recorded at `root` with fingerprint `stable_id` is attached.
pub fn presence(root: &str, stable_id: Option<&str>) -> Presence {
    if !std::path::Path::new(root).is_dir() {
        return Presence::NotConnected;
    }
    match (stable_id, probe(root).identity) {
        (Some(expected), Some(current)) if !current.stable_id.eq_ignore_ascii_case(expected) => Presence::OtherVolume,
        // Matching fingerprint, no fingerprint on record, or reachable but unidentifiable.
        _ => Presence::Connected,
    }
}

#[cfg(windows)]
fn identity(root: &str) -> Option<VolumeIdentity> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW, GetVolumeNameForVolumeMountPointW};

    let mut with_slash = root.to_string();
    if !with_slash.ends_with('\\') {
        with_slash.push('\\');
    }
    let wide: Vec<u16> = OsStr::new(&with_slash).encode_wide().chain(std::iter::once(0)).collect();

    let mut label = [0u16; 261];
    let mut filesystem = [0u16; 261];
    let mut serial: u32 = 0;
    // SAFETY: all buffers are valid for the sizes passed; `wide` is NUL-terminated.
    let ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    if ok == 0 {
        return None;
    }

    let mut total: u64 = 0;
    // SAFETY: valid NUL-terminated path and a valid out pointer; other outputs unused.
    let have_total = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), std::ptr::null_mut(), &mut total, std::ptr::null_mut()) } != 0;

    let mut guid = [0u16; 64];
    // SAFETY: buffer length matches the size passed.
    let have_guid = unsafe { GetVolumeNameForVolumeMountPointW(wide.as_ptr(), guid.as_mut_ptr(), guid.len() as u32) } != 0;

    let text = |buf: &[u16]| {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]);
        (!s.is_empty()).then_some(s)
    };
    let serial_text = format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF);
    let total_bytes = have_total.then(|| i64::try_from(total).unwrap_or(i64::MAX));
    Some(VolumeIdentity {
        stable_id: format!("vol:{serial_text}:{}", total_bytes.unwrap_or(0)),
        label: text(&label),
        filesystem: text(&filesystem),
        serial: Some(serial_text),
        guid: if have_guid { text(&guid) } else { None },
        total_bytes,
    })
}

#[cfg(not(windows))]
fn identity(_root: &str) -> Option<VolumeIdentity> {
    // Not implemented outside Windows yet (NFR-08); falls back to path matching.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_shares_use_their_path() {
        let p = probe(r"\\NAS\Games\");
        assert_eq!(p.kind, "network");
        assert_eq!(p.identity.unwrap().stable_id, r"unc:\\nas\games\");
    }

    #[test]
    fn unreachable_roots_are_not_present() {
        assert_eq!(presence(r"Q:\definitely-not-a-volume\", None), Presence::NotConnected);
    }

    #[cfg(windows)]
    #[test]
    fn system_volume_has_a_fingerprint() {
        let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
        let id = identity(&root).expect("system volume should be identifiable");
        assert!(id.stable_id.starts_with("vol:"), "{}", id.stable_id);
        assert!(id.total_bytes.unwrap_or(0) > 0);
        assert_eq!(presence(&root, Some(&id.stable_id)), Presence::Connected);
        assert_eq!(presence(&root, Some("vol:0000-0000:1")), Presence::OtherVolume, "a different volume at the same letter");
        assert_eq!(presence(&root, None), Presence::Connected);
    }
}
