//! Read-only mounted-volume discovery. Nothing here writes to any disk.

use serde::Serialize;
use sysinfo::Disks;

#[derive(Serialize)]
pub struct Drive {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
}

pub fn list() -> Vec<Drive> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .map(|disk| Drive {
            name: disk.name().to_string_lossy().into_owned(),
            mount_point: disk.mount_point().to_string_lossy().into_owned(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
            removable: disk.is_removable(),
        })
        .collect()
}

/// Classify a volume root (`C:\`, `\\server\share\`) for the catalog.
pub fn volume_kind(root: &str) -> &'static str {
    if root.starts_with(r"\\") {
        return "network";
    }
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .find(|d| d.mount_point().to_string_lossy().eq_ignore_ascii_case(root))
        .map(|d| {
            if d.is_removable() {
                "removable"
            } else {
                "local"
            }
        })
        .unwrap_or("unknown")
}
