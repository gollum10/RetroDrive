//! Platform inference from folder names and file extensions (docs/07 "Recognition").
//!
//! A platform folder (e.g. `snes`, `psx`) is only trusted for files whose extension
//! belongs to that platform, so readme files, saves, states and gamelists inside a
//! platform folder are not indexed as games. Without a folder hint, only extensions
//! unique to one platform are classified; generic containers (`.zip`, `.chd`, `.iso`,
//! `.cue`, `.m3u`, `.7z`) are recorded as `Unclassified`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSource {
    Folder,
    Extension,
    Ambiguous,
}

impl HintSource {
    pub fn as_str(self) -> &'static str {
        match self {
            HintSource::Folder => "folder",
            HintSource::Extension => "extension",
            HintSource::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub platform: &'static str,
    pub source: HintSource,
}

pub const UNCLASSIFIED: &str = "Unclassified";

struct Platform {
    name: &'static str,
    folders: &'static [&'static str],
    extensions: &'static [&'static str],
    /// Extensions accepted inside this platform's folders but too generic to claim alone.
    folder_only: &'static [&'static str],
}

const CARTRIDGE_ARCHIVES: [&str; 2] = ["zip", "7z"];

// `.bin` is accepted only in Genesis folders; for disc systems it is a data track
// referenced by a `.cue`, so counting it separately would double-count games (QA-007).
const PLATFORMS: &[Platform] = &[
    Platform { name: "NES", folders: &["nes", "fc", "famicom"], extensions: &["nes", "fds", "unf", "unif"], folder_only: &[] },
    Platform { name: "SNES", folders: &["snes", "sfc", "superfamicom"], extensions: &["sfc", "smc"], folder_only: &[] },
    Platform { name: "Genesis", folders: &["megadrive", "genesis", "md"], extensions: &["md", "gen", "smd"], folder_only: &["bin"] },
    Platform { name: "Arcade", folders: &["arcade", "mame", "fbneo"], extensions: &["chd"], folder_only: &[] },
    Platform { name: "PlayStation", folders: &["psx", "ps1", "psone", "playstation"], extensions: &["cue", "chd", "iso", "pbp", "m3u"], folder_only: &[] },
    Platform { name: "PlayStation 2", folders: &["ps2"], extensions: &["iso", "chd", "cso"], folder_only: &[] },
    Platform { name: "GBA", folders: &["gba"], extensions: &["gba"], folder_only: &[] },
    Platform { name: "Game Boy", folders: &["gb"], extensions: &["gb"], folder_only: &[] },
    Platform { name: "Game Boy Color", folders: &["gbc"], extensions: &["gbc"], folder_only: &["gb"] },
    Platform { name: "N64", folders: &["n64"], extensions: &["n64", "z64", "v64"], folder_only: &[] },
    Platform { name: "Master System", folders: &["mastersystem", "sms"], extensions: &["sms"], folder_only: &[] },
    Platform { name: "Game Gear", folders: &["gamegear", "gg"], extensions: &["gg"], folder_only: &[] },
];

/// Extensions that are valid game containers but say nothing about the platform.
const AMBIGUOUS_EXTENSIONS: [&str; 6] = ["zip", "7z", "chd", "iso", "cue", "m3u"];

fn accepts(platform: &Platform, extension: &str) -> bool {
    platform.extensions.contains(&extension)
        || platform.folder_only.contains(&extension)
        || (platform.name != "PlayStation" && platform.name != "PlayStation 2" && CARTRIDGE_ARCHIVES.contains(&extension))
}

/// Classify a file. `folders` are the folder names between the scan root (inclusive)
/// and the file, ordered from the scan root down to the file's parent.
/// `extension` must be lower-case without the dot. Returns `None` for non-game files.
pub fn classify(folders: &[&str], extension: &str) -> Option<Classification> {
    for folder in folders.iter().rev() {
        let folder = folder.to_lowercase();
        if let Some(platform) = PLATFORMS.iter().find(|p| p.folders.contains(&folder.as_str())) {
            // The nearest platform folder decides; a non-matching extension inside it
            // means the file is not a game for that platform.
            return accepts(platform, extension)
                .then_some(Classification { platform: platform.name, source: HintSource::Folder });
        }
    }
    if AMBIGUOUS_EXTENSIONS.contains(&extension) {
        return Some(Classification { platform: UNCLASSIFIED, source: HintSource::Ambiguous });
    }
    let mut matches = PLATFORMS.iter().filter(|p| p.extensions.contains(&extension));
    let first = matches.next()?;
    if matches.next().is_some() {
        return Some(Classification { platform: UNCLASSIFIED, source: HintSource::Ambiguous });
    }
    Some(Classification { platform: first.name, source: HintSource::Extension })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform(folders: &[&str], ext: &str) -> Option<&'static str> {
        classify(folders, ext).map(|c| c.platform)
    }

    #[test]
    fn non_game_files_in_platform_folders_are_skipped() {
        for ext in ["txt", "srm", "state", "xml", "png", "jpg", "nfo", "bin"] {
            assert_eq!(platform(&["roms", "snes"], ext), None, "{ext} should be skipped");
        }
    }

    #[test]
    fn platform_folder_with_matching_extension() {
        let c = classify(&["roms", "snes"], "sfc").unwrap();
        assert_eq!(c.platform, "SNES");
        assert_eq!(c.source, HintSource::Folder);
        assert_eq!(platform(&["roms", "nes"], "zip"), Some("NES"));
        assert_eq!(platform(&["roms", "megadrive"], "bin"), Some("Genesis"));
        assert_eq!(platform(&["roms", "arcade"], "zip"), Some("Arcade"));
    }

    #[test]
    fn nested_game_folders_use_nearest_platform_ancestor() {
        assert_eq!(platform(&["roms", "psx", "Some Game (USA)"], "cue"), Some("PlayStation"));
        assert_eq!(platform(&["psx", "Disc Set", "Disc 1"], "chd"), Some("PlayStation"));
    }

    #[test]
    fn disc_data_tracks_are_not_counted_as_games() {
        assert_eq!(platform(&["psx", "Game"], "bin"), None);
        assert_eq!(platform(&["psx"], "zip"), None);
    }

    #[test]
    fn scan_root_name_counts_as_a_folder_hint() {
        assert_eq!(platform(&["GBA"], "gba"), Some("GBA"));
    }

    #[test]
    fn extension_only_classification() {
        let c = classify(&["My Stuff"], "sfc").unwrap();
        assert_eq!(c.platform, "SNES");
        assert_eq!(c.source, HintSource::Extension);
        assert_eq!(platform(&["My Stuff"], "z64"), Some("N64"));
    }

    #[test]
    fn ambiguous_extensions_are_unclassified() {
        for ext in ["zip", "7z", "chd", "iso", "cue", "m3u"] {
            let c = classify(&["Downloads"], ext).unwrap();
            assert_eq!(c.platform, UNCLASSIFIED, "{ext}");
            assert_eq!(c.source, HintSource::Ambiguous);
        }
        assert_eq!(platform(&["Downloads"], "gb"), Some("Game Boy"));
        assert_eq!(platform(&["roms", "gbc"], "gb"), Some("Game Boy Color"));
    }

    #[test]
    fn unknown_extensions_are_skipped() {
        assert_eq!(platform(&["Downloads"], "exe"), None);
        assert_eq!(platform(&["Downloads"], "bin"), None);
        assert_eq!(platform(&["Downloads"], ""), None);
    }
}
