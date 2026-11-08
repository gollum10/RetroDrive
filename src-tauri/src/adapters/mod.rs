//! Platform adapter contract (RDS-006, docs/04 "Platform adapter contract").
//!
//! Each supported platform (RetroBat, Batocera; others later) implements
//! `PlatformAdapter`. Adapters are strictly read-only: they recognize an
//! installation, check its layout and *plan* where games would go. Nothing here
//! writes files; the copier (RDS-010) does, and only for adapters whose
//! `Capabilities::copy_games` is set.

pub mod batocera;
pub mod esde;
pub mod lakka;
pub mod recalbox;
pub mod retrobat;
pub mod retropie;
pub mod rocknix;

use serde::Serialize;
use std::path::Path;

/// Bump when the trait or its data types change incompatibly. Stored with each
/// saved installation so later versions can tell which contract recorded it.
/// v2 (2026-11-05): `PlacementRequest` carries the installation's `PlacementOptions`.
/// v3 (2026-11-06, RDS-008): `Capabilities::copy_games` and `Detection::hint`.
pub const CONTRACT_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Frontend is for a later ES-DE adapter.
pub enum AdapterKind {
    /// A bootable OS image (e.g. Batocera).
    BootableOs,
    /// A portable Windows application (e.g. RetroBat).
    PortableApp,
    /// A frontend that launches separately installed emulators (e.g. ES-DE).
    Frontend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    NotDetected,
    Possible,
    Detected,
}

/// What an adapter found in a folder, with the evidence behind it.
#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub adapter_id: &'static str,
    pub display_name: &'static str,
    pub kind: AdapterKind,
    pub confidence: Confidence,
    pub evidence: Vec<String>,
    /// Upstream version when it can be read reliably; `None` means unknown.
    pub version: Option<String>,
    /// What to do when this isn't quite the right folder (e.g. "choose the folder
    /// that contains RetroBat.exe"). Shown when nothing was recognized.
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Problem,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutCheck {
    pub id: &'static str,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
}

/// What an adapter can do. Adapters themselves never install or write.
#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub detect: bool,
    pub validate_layout: bool,
    pub plan_placement: bool,
    /// Whether RetroDrive's copier may copy games into this kind of installation.
    /// Off means plans are previews only and the copier refuses (docs/18).
    pub copy_games: bool,
    pub install: bool,
    pub write_files: bool,
}

/// One file of a game to place: the primary file plus any discs and tracks.
#[derive(Debug, Clone)]
pub struct PlacementFile {
    /// Path relative to the primary file's folder, with `/` separators
    /// (e.g. `Saga (Disc 1).cue` or `CD1/track.bin`). Keeps playlist and cue
    /// references valid after copying.
    pub relative_to_primary: String,
    pub size: i64,
}

/// Which arcade emulator family's romset the user has. MAME and FinalBurn Neo
/// use different, version-specific romsets, so RetroDrive never guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcadeSystem {
    Mame,
    Fbneo,
}

impl ArcadeSystem {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mame" => Some(Self::Mame),
            "fbneo" => Some(Self::Fbneo),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mame => "mame",
            Self::Fbneo => "fbneo",
        }
    }
}

/// Per-installation choices that affect placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlacementOptions {
    /// `None`: arcade games aren't placed.
    pub arcade: Option<ArcadeSystem>,
    /// The installation folder has a `roms` folder inside (RetroPie: a USB `retropie`
    /// folder) rather than being the roms folder itself (RetroPie's network share).
    pub has_roms_folder: bool,
}

/// A game to plan, as the catalog knows it.
#[derive(Debug, Clone)]
pub struct PlacementRequest {
    pub title: String,
    pub platform: String,
    /// Lower-case extension of the primary file.
    pub extension: String,
    /// The primary file first, then its discs and tracks.
    pub files: Vec<PlacementFile>,
    pub options: PlacementOptions,
    /// For BIOS files kept in a library `bios` folder: the path below it (e.g.
    /// `dc/dc_boot.bin`), so they land in the same place in the installation's bios folder.
    pub bios_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Placement {
    /// Destination paths relative to the installation root, `/`-separated, in
    /// the same order as `PlacementRequest::files`.
    Planned { system_folder: String, destinations: Vec<String> },
    Unsupported { reason: String },
}

pub trait PlatformAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn kind(&self) -> AdapterKind;
    fn capabilities(&self) -> Capabilities;
    /// Read-only inspection of `root`.
    fn detect(&self, root: &Path) -> Detection;
    /// Read-only layout checks for an installation at `root`.
    fn validate_layout(&self, root: &Path) -> Vec<LayoutCheck>;
    /// Pure function: where a game would go. Must not touch the file system.
    fn plan_placement(&self, request: &PlacementRequest) -> Placement;
}

/// Summary of an adapter for the UI's platform list.
#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: AdapterKind,
    pub capabilities: Capabilities,
    pub contract_version: u32,
}

pub fn catalog() -> Vec<AdapterInfo> {
    registry()
        .iter()
        .map(|a| AdapterInfo { id: a.id(), display_name: a.display_name(), kind: a.kind(), capabilities: a.capabilities(), contract_version: CONTRACT_VERSION })
        .collect()
}

/// Every adapter this build ships.
pub fn registry() -> Vec<Box<dyn PlatformAdapter>> {
    vec![Box::new(retrobat::RetroBat), Box::new(batocera::Batocera), Box::new(retropie::RetroPie), Box::new(esde::EsDe), Box::new(recalbox::Recalbox), Box::new(rocknix::Rocknix), Box::new(lakka::Lakka)]
}

pub fn find(id: &str) -> Option<Box<dyn PlatformAdapter>> {
    registry().into_iter().find(|a| a.id() == id)
}

/// Run every adapter's detection on `root`, most confident first.
pub fn detect_all(root: &Path) -> Vec<Detection> {
    let mut found: Vec<Detection> = registry().iter().map(|a| a.detect(root)).collect();
    found.sort_by_key(|d| std::cmp::Reverse(d.confidence));
    found
}

/// Case-insensitive check for an entry directly inside `root`.
/// The RetroBat/Batocera system folder for a catalog platform (arcade as MAME), or
/// `None` when it has none. Used to match light gun games (ADR-066).
pub(crate) fn batocera_folder(platform: &str, extension: &str) -> Option<&'static str> {
    retrobat::system_folder(platform, extension, PlacementOptions { arcade: Some(ArcadeSystem::Mame), has_roms_folder: true }, "RetroBat").ok()
}

pub(crate) fn has_entry(root: &Path, name: &str, want_dir: bool) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else { return false };
    entries.filter_map(Result::ok).any(|e| {
        e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.file_type().map(|t| t.is_dir() == want_dir).unwrap_or(false)
    })
}

/// Placement in an EmulationStation-style `roms/<system>` tree (RetroBat and
/// Batocera). Single files go straight into the system folder; multi-file games
/// get their own subfolder so .m3u/.cue references keep working.
pub(crate) fn plan_in_roms(system: &str, request: &PlacementRequest) -> Placement {
    plan_in("roms/", system, request)
}

/// Like `plan_in_roms`, with the folder that holds the system folders (`""` when the
/// installation folder is the roms folder itself).
pub(crate) fn plan_in(prefix: &str, system: &str, request: &PlacementRequest) -> Placement {
    if request.files.iter().any(|f| f.relative_to_primary.split('/').any(|part| part == ".." || part.is_empty())) {
        return Placement::Unsupported { reason: "Its files are spread across folders, so the set can't be copied as one unit.".into() };
    }
    let base = if request.files.len() > 1 { format!("{prefix}{system}/{}", folder_name(&request.title)) } else { format!("{prefix}{system}") };
    Placement::Planned { system_folder: system.to_string(), destinations: request.files.iter().map(|f| format!("{base}/{}", f.relative_to_primary)).collect() }
}

/// Housekeeping files found in BIOS folders that aren't copied (`.gitkeep`, `.DS_Store`, …).
pub(crate) fn is_housekeeping(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    name.starts_with('.') || ["_ds_store", "thumbs.db", "desktop.ini", "_info.txt"].contains(&name.as_str())
}

/// BIOS files from a library `bios` folder go to the installation's BIOS folder
/// (`folder`), keeping their subfolders (owner request 2026-11-07; docs/18 §31).
pub(crate) fn plan_bios(folder: &str, request: &PlacementRequest) -> Placement {
    let Some(path) = request.bios_path.as_deref().filter(|p| !p.is_empty()) else {
        return Placement::Unsupported {
            reason: "BIOS sets named like \"[BIOS] … .zip\" are archives of the original chips; emulators need the files themselves with their exact names, so RetroDrive doesn't place these. Keep the BIOS files you use in a folder named bios in your library.".into(),
        };
    };
    if is_housekeeping(path) {
        return Placement::Unsupported { reason: "Housekeeping files (like .gitkeep, .DS_Store or _info.txt) aren't copied.".into() };
    }
    if request.files.len() != 1 || path.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Placement::Unsupported { reason: "This BIOS file's path can't be copied as it is.".into() };
    }
    Placement::Planned { system_folder: "bios".into(), destinations: vec![format!("{folder}/{path}")] }
}

/// Make a game title safe as a Windows folder name.
pub fn folder_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    if trimmed.is_empty() || reserved.contains(&trimmed.to_uppercase().as_str()) {
        format!("_{trimmed}")
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in adapter proving the contract works for any implementation.
    struct MockAdapter;
    impl PlatformAdapter for MockAdapter {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock"
        }
        fn kind(&self) -> AdapterKind {
            AdapterKind::Frontend
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities { detect: true, validate_layout: false, plan_placement: true, copy_games: false, install: false, write_files: false }
        }
        fn detect(&self, root: &Path) -> Detection {
            let found = root.join("mock.marker").exists();
            Detection {
                adapter_id: "mock",
                display_name: "Mock",
                kind: AdapterKind::Frontend,
                confidence: if found { Confidence::Detected } else { Confidence::NotDetected },
                evidence: vec![],
                version: None,
                hint: None,
            }
        }
        fn validate_layout(&self, _root: &Path) -> Vec<LayoutCheck> {
            vec![]
        }
        fn plan_placement(&self, request: &PlacementRequest) -> Placement {
            Placement::Planned {
                system_folder: "all".into(),
                destinations: request.files.iter().map(|f| format!("games/{}", f.relative_to_primary)).collect(),
            }
        }
    }

    #[test]
    fn contract_works_through_trait_objects() {
        let dir = tempfile::tempdir().unwrap();
        let adapters: Vec<Box<dyn PlatformAdapter>> = vec![Box::new(MockAdapter)];
        assert_eq!(adapters[0].detect(dir.path()).confidence, Confidence::NotDetected);
        std::fs::write(dir.path().join("mock.marker"), b"").unwrap();
        assert_eq!(adapters[0].detect(dir.path()).confidence, Confidence::Detected);
        let plan = adapters[0].plan_placement(&PlacementRequest {
            title: "X".into(),
            platform: "NES".into(),
            extension: "nes".into(),
            files: vec![PlacementFile { relative_to_primary: "X.nes".into(), size: 1 }],
            options: PlacementOptions::default(),
            bios_path: None,
        });
        assert_eq!(plan, Placement::Planned { system_folder: "all".into(), destinations: vec!["games/X.nes".into()] });
    }

    #[test]
    fn shipped_adapters_are_read_only_and_unique() {
        let adapters = registry();
        let mut ids: Vec<&str> = adapters.iter().map(|a| a.id()).collect();
        ids.dedup();
        assert_eq!(ids.len(), adapters.len());
        for adapter in &adapters {
            let caps = adapter.capabilities();
            assert!(!caps.install && !caps.write_files, "{} must be read-only", adapter.id());
        }
        assert!(find("retrobat").unwrap().capabilities().copy_games);
        assert!(find("batocera").unwrap().capabilities().copy_games, "Batocera copies since its safety review (docs/18 §21)");
        let info = catalog();
        assert_eq!(info[0].kind, AdapterKind::PortableApp);
        assert_eq!(info[0].contract_version, CONTRACT_VERSION);
        assert!(find("nope").is_none());
    }

    #[test]
    fn folder_names_are_windows_safe() {
        assert_eq!(folder_name("Saga: Part 2 (USA)"), "Saga_ Part 2 (USA)");
        assert_eq!(folder_name("What?/Why*"), "What__Why_");
        assert_eq!(folder_name("Trailing dots..."), "Trailing dots");
        assert_eq!(folder_name("con"), "_con");
        assert_eq!(folder_name("   "), "_");
    }

    #[test]
    fn detection_ranks_most_confident_first() {
        let dir = tempfile::tempdir().unwrap();
        let results = detect_all(dir.path());
        assert!(results.windows(2).all(|w| w[0].confidence >= w[1].confidence));
    }
}
