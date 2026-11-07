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
/// BIOS and firmware files (a folder named `bios`, or a name starting with `[BIOS]`).
pub const BIOS: &str = "BIOS";

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
    Platform { name: "NES", folders: &["nes", "fc", "famicom", "nesh"], extensions: &["nes", "unf", "unif"], folder_only: &["fds"] },
    Platform { name: "Famicom Disk System", folders: &["fds"], extensions: &["fds"], folder_only: &[] },
    Platform { name: "SNES", folders: &["snes", "sfc", "superfamicom", "snesh"], extensions: &["sfc", "smc"], folder_only: &[] },
    Platform { name: "Genesis", folders: &["megadrive", "genesis", "md", "genh", "megadrive-japan"], extensions: &["md", "gen", "smd"], folder_only: &["bin"] },
    Platform { name: "Arcade", folders: &["arcade", "mame", "fbneo"], extensions: &["chd"], folder_only: &[] },
    Platform { name: "PlayStation", folders: &["psx", "ps1", "psone", "playstation"], extensions: &["cue", "chd", "iso", "pbp", "m3u"], folder_only: &[] },
    // `.cso` is used by both PS2 and PSP, so it only counts inside their folders (2026-11-07:
    // the owner's `psp` / `pspminis` games were being placed as PS2).
    Platform { name: "PlayStation 2", folders: &["ps2"], extensions: &["iso", "chd"], folder_only: &["cso"] },
    Platform { name: "PSP", folders: &["psp", "pspminis", "pspminis2"], extensions: &[], folder_only: &["iso", "cso", "pbp", "chd"] },
    Platform { name: "GBA", folders: &["gba", "gbah"], extensions: &["gba"], folder_only: &[] },
    Platform { name: "Game Boy", folders: &["gb", "gbh"], extensions: &["gb"], folder_only: &[] },
    Platform { name: "Game Boy Color", folders: &["gbc", "gbch"], extensions: &["gbc"], folder_only: &["gb"] },
    Platform { name: "N64", folders: &["n64"], extensions: &["n64", "z64", "v64"], folder_only: &[] },
    Platform { name: "Master System", folders: &["mastersystem", "sms"], extensions: &["sms"], folder_only: &[] },
    Platform { name: "Game Gear", folders: &["gamegear", "gg", "gamegearh"], extensions: &["gg"], folder_only: &[] },
    // Added 2026-11-05 (folder names from the owner's library and the RetroBat wiki).
    Platform { name: "Amstrad CPC", folders: &["amstradcpc", "cpc"], extensions: &["cdt"], folder_only: &["dsk"] },
    Platform { name: "Amstrad GX4000", folders: &["amstradgx4000", "gx4000"], extensions: &["cpr"], folder_only: &[] },
    Platform { name: "Atari 800", folders: &["atari800"], extensions: &["atr", "xex", "xfd", "atx"], folder_only: &["car", "bin"] },
    Platform { name: "Atari ST", folders: &["atarist"], extensions: &["st", "msa", "stx"], folder_only: &[] },
    Platform { name: "Atari 2600", folders: &["atari2600"], extensions: &["a26"], folder_only: &["bin"] },
    Platform { name: "Atari 5200", folders: &["atari5200"], extensions: &["a52"], folder_only: &["bin"] },
    Platform { name: "Atari 7800", folders: &["atari7800"], extensions: &["a78"], folder_only: &["bin"] },
    Platform { name: "Atari Lynx", folders: &["atarilynx", "lynx"], extensions: &["lnx"], folder_only: &[] },
    Platform { name: "Atari Jaguar", folders: &["atarijaguar", "jaguar"], extensions: &["j64", "jag"], folder_only: &[] },
    Platform { name: "Commodore 64", folders: &["c64"], extensions: &["d64", "t64", "crt", "g64"], folder_only: &["tap", "prg"] },
    Platform { name: "Amiga", folders: &["amiga", "amiga500", "amiga600"], extensions: &["adf", "ipf"], folder_only: &["hdf", "lha"] },
    Platform { name: "Amiga 1200", folders: &["amiga1200"], extensions: &[], folder_only: &["adf", "ipf", "hdf", "lha"] },
    Platform { name: "Amiga CD32", folders: &["amigacd32", "cd32"], extensions: &[], folder_only: &["cue", "chd", "iso"] },
    Platform { name: "Sharp X68000", folders: &["x68000"], extensions: &["dim", "xdf", "hdm"], folder_only: &["m3u", "hdf"] },
    Platform { name: "MSX", folders: &["msx", "msx1"], extensions: &["mx1"], folder_only: &["rom", "dsk", "cas"] },
    Platform { name: "MSX2", folders: &["msx2"], extensions: &["mx2"], folder_only: &["rom", "dsk", "cas"] },
    Platform { name: "ZX81", folders: &["zx81"], extensions: &["p", "81"], folder_only: &[] },
    Platform { name: "ZX Spectrum", folders: &["zxspectrum", "spectrum"], extensions: &["tzx", "z80", "sna"], folder_only: &["tap", "dsk"] },
    Platform { name: "PC Engine", folders: &["pcengine", "tg16", "turbografx16"], extensions: &["pce"], folder_only: &[] },
    Platform { name: "PC Engine CD", folders: &["pcenginecd", "tgcd"], extensions: &[], folder_only: &["cue", "chd", "iso", "m3u"] },
    Platform { name: "SuperGrafx", folders: &["supergrafx", "sgfx"], extensions: &["sgx"], folder_only: &[] },
    Platform { name: "Nintendo DS", folders: &["nds"], extensions: &["nds"], folder_only: &[] },
    Platform { name: "Neo Geo", folders: &["neogeo"], extensions: &[], folder_only: &[] },
    Platform { name: "Neo Geo CD", folders: &["neocd", "neogeocd"], extensions: &[], folder_only: &["chd", "cue", "iso", "m3u"] },
    Platform { name: "Neo Geo Pocket", folders: &["ngp"], extensions: &["ngp"], folder_only: &[] },
    Platform { name: "Neo Geo Pocket Color", folders: &["ngpc"], extensions: &["ngc"], folder_only: &["ngp"] },
    Platform { name: "Intellivision", folders: &["intellivision"], extensions: &["int"], folder_only: &["bin", "rom"] },
    Platform { name: "SG-1000", folders: &["sg-1000", "sg1000"], extensions: &["sg"], folder_only: &[] },
    Platform { name: "Odyssey 2", folders: &["odyssey", "odyssey2", "videopac"], extensions: &[], folder_only: &["bin"] },
    Platform { name: "WonderSwan", folders: &["wonderswan", "wswan"], extensions: &["ws"], folder_only: &[] },
    Platform { name: "WonderSwan Color", folders: &["wonderswancolor", "wswanc"], extensions: &["wsc"], folder_only: &["ws"] },
    Platform { name: "ColecoVision", folders: &["coleco", "colecovision"], extensions: &["col"], folder_only: &["rom", "bin"] },
    Platform { name: "Vectrex", folders: &["vectrex"], extensions: &["vec"], folder_only: &["bin"] },
    Platform { name: "Game & Watch", folders: &["gameandwatch", "gw"], extensions: &["mgw"], folder_only: &[] },
    Platform { name: "Sega 32X", folders: &["sega32x", "32x"], extensions: &["32x"], folder_only: &["bin"] },
    Platform { name: "Sega CD", folders: &["segacd", "megacd"], extensions: &[], folder_only: &["chd", "cue", "iso", "m3u"] },
    Platform { name: "Pokemon Mini", folders: &["pokemini"], extensions: &["min"], folder_only: &[] },
    Platform { name: "Virtual Boy", folders: &["virtualboy"], extensions: &["vb"], folder_only: &[] },
    Platform { name: "Watara Supervision", folders: &["supervision"], extensions: &["sv"], folder_only: &[] },
    Platform { name: "CPS1", folders: &["cps1"], extensions: &[], folder_only: &[] },
    Platform { name: "CPS2", folders: &["cps2"], extensions: &[], folder_only: &[] },
    Platform { name: "CPS3", folders: &["cps3"], extensions: &[], folder_only: &[] },
    Platform { name: "Dreamcast", folders: &["dreamcast", "dc"], extensions: &["cdi", "gdi"], folder_only: &["chd", "cue", "m3u"] },
    Platform { name: "Naomi", folders: &["naomi"], extensions: &[], folder_only: &["chd"] },
    Platform { name: "Atomiswave", folders: &["atomiswave"], extensions: &[], folder_only: &[] },
    Platform { name: "Sega Saturn", folders: &["saturn"], extensions: &[], folder_only: &["chd", "cue", "iso", "m3u"] },
    Platform { name: "3DO", folders: &["3do"], extensions: &[], folder_only: &["chd", "cue", "iso"] },
    // BIOS and firmware files the user keeps with their games: recognized so they
    // aren't listed as games, never placed or copied as games.
    Platform { name: BIOS, folders: &["bios"], extensions: &[], folder_only: &["bin", "rom", "img"] },
];

/// Extensions that are valid game containers but say nothing about the platform.
const AMBIGUOUS_EXTENSIONS: [&str; 7] = ["zip", "7z", "chd", "iso", "cue", "m3u", "cso"];

fn accepts(platform: &Platform, extension: &str) -> bool {
    platform.extensions.contains(&extension)
        || platform.folder_only.contains(&extension)
        || (!matches!(platform.name, "PlayStation" | "PlayStation 2" | "PSP") && CARTRIDGE_ARCHIVES.contains(&extension))
}

/// Not BIOS even inside a `bios` folder: Windows' own files and shortcuts.
const BIOS_SKIP: &[&str] = &["db", "ini", "lnk", "url"];

/// True for files named like `[BIOS] Game Boy Advance (World).zip`.
pub fn looks_like_bios(filename: &str) -> bool {
    filename.trim_start().to_ascii_lowercase().starts_with("[bios]")
}

/// Classify a file. `folders` are the folder names between the scan root (inclusive)
/// and the file, ordered from the scan root down to the file's parent.
/// `extension` must be lower-case without the dot. Returns `None` for non-game files.
pub fn classify(folders: &[&str], extension: &str) -> Option<Classification> {
    // Everything inside a `bios` folder is a BIOS file, whatever its subfolder (`bios\dc`,
    // `bios\PPSSPP`, …) or extension (`neogeo.zip`, `.dat`, …), except Windows clutter
    // (owner request 2026-11-07: copy BIOS files; docs/18 §31).
    if folders.iter().any(|f| f.eq_ignore_ascii_case("bios")) {
        return (!BIOS_SKIP.contains(&extension)).then_some(Classification { platform: BIOS, source: HintSource::Folder });
    }
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
    fn everything_in_a_bios_folder_is_bios() {
        assert_eq!(platform(&["roms", "bios", "dc"], "bin"), Some(BIOS), "a system-named subfolder inside bios");
        assert_eq!(platform(&["roms", "BIOS"], "zip"), Some(BIOS), "neogeo.zip");
        assert_eq!(platform(&["roms", "bios", "PPSSPP", "flash0", "font"], "pgf"), Some(BIOS));
        assert_eq!(platform(&["roms", "bios"], "db"), None, "Thumbs.db");
        assert_eq!(platform(&["roms", "dc"], "bin"), None, "outside bios, dc rules still apply");
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
    fn psp_games_are_psp_not_ps2() {
        for folder in ["psp", "pspminis", "PSP"] {
            assert_eq!(platform(&["roms II", folder], "cso"), Some("PSP"), "{folder}");
            assert_eq!(platform(&["roms", folder], "iso"), Some("PSP"));
        }
        assert_eq!(platform(&["roms", "psp"], "pbp"), Some("PSP"));
        assert_eq!(platform(&["roms", "psp"], "zip"), None, "PSP games aren't zipped");
        assert_eq!(platform(&["roms", "ps2"], "cso"), Some("PlayStation 2"));
        let loose = classify(&["Downloads"], "cso").unwrap();
        assert_eq!((loose.platform, loose.source), (UNCLASSIFIED, HintSource::Ambiguous), "a .cso alone could be PS2 or PSP");
    }

    #[test]
    fn unknown_extensions_are_skipped() {
        assert_eq!(platform(&["Downloads"], "exe"), None);
        assert_eq!(platform(&["Downloads"], "bin"), None);
        assert_eq!(platform(&["Downloads"], ""), None);
    }

    #[test]
    fn more_systems_and_the_owners_folder_names_are_recognized() {
        for (folder, ext, expected) in [
            ("amstradcpc", "zip", "Amstrad CPC"),
            ("atari800", "zip", "Atari 800"),
            ("atarist", "zip", "Atari ST"),
            ("c64", "zip", "Commodore 64"),
            ("amiga", "zip", "Amiga"),
            ("amiga1200", "adf", "Amiga 1200"),
            ("x68000", "m3u", "Sharp X68000"),
            ("nesh", "zip", "NES"),
            ("snesh", "zip", "SNES"),
            ("genh", "zip", "Genesis"),
            ("megadrive-japan", "zip", "Genesis"),
            ("msx", "zip", "MSX"),
            ("fds", "zip", "Famicom Disk System"),
            ("neocd", "chd", "Neo Geo CD"),
            ("segacd", "chd", "Sega CD"),
            ("sgfx", "zip", "SuperGrafx"),
            ("sg-1000", "zip", "SG-1000"),
            ("wonderswancolor", "zip", "WonderSwan Color"),
            ("cps2", "zip", "CPS2"),
            ("bios", "zip", "BIOS"),
        ] {
            assert_eq!(platform(&["roms II", folder], ext), Some(expected), "{folder}/{ext}");
        }
        assert_eq!(platform(&["roms"], "d64"), Some("Commodore 64"), "a unique extension needs no folder");
        assert_eq!(platform(&["roms"], "fds"), Some("Famicom Disk System"));
        assert_eq!(platform(&["roms", "nes"], "fds"), Some("NES"), "a .fds file in an NES folder stays NES");
        assert_eq!(platform(&["roms"], "dsk"), None, "disk images shared by several computers need a folder");
        assert_eq!(platform(&["roms", "segacd"], "bin"), None, "disc tracks aren't counted as games");
        assert!(looks_like_bios("[BIOS] Game Boy Advance (World).zip"));
        assert!(!looks_like_bios("Bioshock.iso"));
    }
}
