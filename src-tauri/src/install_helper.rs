//! Install helper (RDS-027; owner request 2026-11-07, guided approach approved the
//! same day; docs/18 §24): help the user put Batocera or RetroPie on a card or USB
//! stick from an image they downloaded themselves.
//!
//! RetroDrive never writes to the card: it reads the image (type, name, optional
//! checksum), then opens Raspberry Pi Imager or balenaEtcher, which the user installed,
//! and the user picks the drive there. Nothing here writes to any drive.
//!
//! - Raspberry Pi Imager takes an image path as its last argument
//!   (`rpi-imager [options] [image-uri]`, its manual); without `--cli` it only opens the
//!   app with the image chosen. balenaEtcher is opened plainly; the user picks the file.
//! - Writers are only started from their normal install folders, never from a path the
//!   screen sends, and the image path is passed as one argument (no shell).
//! - Download pages are fixed official addresses.

use md5::Md5;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Official download pages, by system.
pub fn download_page(system: &str) -> Option<&'static str> {
    match system {
        "batocera" => Some("https://batocera.org/download"),
        "retropie" => Some("https://retropie.org.uk/download/"),
        "rpi-imager" => Some("https://www.raspberrypi.com/software/"),
        "etcher" => Some("https://etcher.balena.io/"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageInfo {
    pub path: String,
    pub file_name: String,
    pub bytes: u64,
    /// `gzip`, `xz`, `zip` or `img` (an uncompressed disk image).
    pub format: String,
    /// `batocera` or `retropie`, from the file name.
    pub system: Option<String>,
    pub version: Option<String>,
    /// The hardware the image is for, as the user would say it.
    pub target: Option<String>,
    /// Something that makes the file unusable (wrong type, cut short).
    pub problem: Option<String>,
    /// Things worth knowing (e.g. the file looks like a different system).
    pub notes: Vec<String>,
}

const GZIP: &[u8] = &[0x1f, 0x8b];
const XZ: &[u8] = &[0xfd, b'7', b'z', b'X', b'Z', 0x00];
const ZIP: &[u8] = &[b'P', b'K', 0x03, 0x04];

/// Which kind of file this is, from its first bytes, and whether its end looks complete.
fn format_of(file: &mut File, bytes: u64) -> Result<(String, Option<String>), String> {
    let mut head = [0u8; 1024];
    let n = file
        .read(&mut head)
        .map_err(|e| format!("The file can't be read: {e}"))?;
    let head = &head[..n];
    if head.starts_with(GZIP) {
        return Ok(("gzip".into(), None));
    }
    if head.starts_with(XZ) {
        // An xz file ends with the footer magic "YZ"; a cut-short download doesn't.
        let mut tail = [0u8; 2];
        file.seek(SeekFrom::End(-2))
            .and_then(|_| file.read_exact(&mut tail))
            .map_err(|e| format!("The file can't be read: {e}"))?;
        let problem = (tail != *b"YZ").then(|| {
            "The file ends early: the download probably didn't finish. Download it again."
                .to_string()
        });
        return Ok(("xz".into(), problem));
    }
    if head.starts_with(ZIP) {
        return Ok(("zip".into(), None));
    }
    // An uncompressed disk image starts with a partition table: an MBR ends its first
    // 512 bytes with 55 AA (GPT disks keep a protective MBR too).
    if n >= 512 && head[510] == 0x55 && head[511] == 0xAA {
        let problem = (bytes < 64 << 20)
            .then(|| "This disk image is too small to be a system image.".to_string());
        return Ok(("img".into(), problem));
    }
    Err("This isn't a disk image RetroDrive recognizes. Batocera and RetroPie images end in .img.gz (sometimes .img.xz, .zip or .img).".into())
}

/// Batocera: `batocera-<board…>-<version>-<yyyymmdd>.img.gz`; RetroPie:
/// `retropie-<debian>-<version>-<boards>.img.gz`.
fn name_facts(file_name: &str) -> (Option<String>, Option<String>, Option<String>) {
    let lower = file_name.to_lowercase();
    let stem = lower
        .split(".img")
        .next()
        .unwrap_or(&lower)
        .trim_end_matches(".zip");
    let tokens: Vec<&str> = stem.split('-').collect();
    let is_version = |t: &str| {
        !t.is_empty()
            && t.chars().all(|c| c.is_ascii_digit() || c == '.')
            && t.chars().next().is_some_and(|c| c.is_ascii_digit())
            && t.len() < 8
    };
    match tokens.first() {
        Some(&"batocera") => {
            let version = tokens
                .iter()
                .skip(1)
                .find(|t| is_version(t))
                .map(|t| t.to_string());
            let mut boards: Vec<&str> = tokens
                .iter()
                .skip(1)
                .take_while(|t| !is_version(t))
                .copied()
                .collect();
            boards.dedup();
            let label = boards
                .iter()
                .find_map(|b| batocera_board(b))
                .map(str::to_string)
                .or_else(|| (!boards.is_empty()).then(|| boards.join(" ")));
            (Some("batocera".into()), version, label)
        }
        Some(&"retropie") => {
            let version = tokens
                .iter()
                .skip(1)
                .find(|t| is_version(t))
                .map(|t| t.to_string());
            let boards = tokens
                .iter()
                .rev()
                .find(|t| t.starts_with("rpi") || t.starts_with("x86"))
                .copied();
            (
                Some("retropie".into()),
                version,
                boards.map(retropie_boards),
            )
        }
        _ => (None, None, None),
    }
}

fn batocera_board(token: &str) -> Option<&'static str> {
    Some(match token {
        "x86_64" => "PCs (64-bit)",
        "x86" => "older PCs (32-bit)",
        "bcm2712" => "Raspberry Pi 5",
        "bcm2711" => "Raspberry Pi 4 / 400",
        "bcm2837" => "Raspberry Pi 3",
        "bcm2836" => "Raspberry Pi 2",
        "bcm2835" => "Raspberry Pi 1 / Zero",
        _ => return None,
    })
}

fn retropie_boards(token: &str) -> String {
    let parts: Vec<String> = token
        .trim_start_matches("rpi")
        .split('_')
        .map(|p| match p {
            "zero" => "Zero".to_string(),
            "zero2w" => "Zero 2 W".to_string(),
            other => other.to_string(),
        })
        .collect();
    format!("Raspberry Pi {}", parts.join(" / "))
}

/// Look at an image the user downloaded (reads only).
pub fn inspect(path: &str, expected_system: Option<&str>) -> Result<ImageInfo, String> {
    let p = Path::new(path);
    let meta = std::fs::metadata(p).map_err(|_| "That file can't be found.".to_string())?;
    if !meta.is_file() {
        return Err("Choose the image file itself, not a folder.".into());
    }
    let mut file = File::open(p).map_err(|e| format!("The file can't be opened: {e}"))?;
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let (format, mut problem) = match format_of(&mut file, meta.len()) {
        Ok(found) => found,
        Err(e) => (String::new(), Some(e)),
    };
    if problem.is_none() && meta.len() < 50 << 20 {
        problem = Some("The file is much smaller than a system image: the download probably didn't finish. Download it again.".into());
    }
    let (system, version, target) = name_facts(&file_name);
    let mut notes = Vec::new();
    match (expected_system, system.as_deref()) {
        (Some(want), Some(got)) if want != got => notes.push(format!(
            "This looks like a {} image, not {}.",
            label(got),
            label(want)
        )),
        (Some(want), None) => notes.push(format!(
            "The file name doesn't say it's {}; make sure it's the image from {}.",
            label(want),
            download_page(want).unwrap_or("the official site")
        )),
        _ => {}
    }
    if format == "zip" {
        notes.push(
            "Zip files: Raspberry Pi Imager and balenaEtcher can usually write them directly."
                .into(),
        );
    }
    Ok(ImageInfo {
        path: path.to_string(),
        file_name,
        bytes: meta.len(),
        format,
        system,
        version,
        target,
        problem,
        notes,
    })
}

fn label(system: &str) -> &'static str {
    match system {
        "batocera" => "Batocera",
        "retropie" => "RetroPie",
        _ => "an emulation system",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecksumResult {
    pub algorithm: &'static str,
    pub actual: String,
    pub matches: bool,
}

/// Compare the file with the checksum from the download page: SHA-256 (64 hex
/// characters, Batocera) or MD5 (32, RetroPie). Reads only. `progress(done, total)`.
pub fn checksum(
    path: &str,
    expected: &str,
    progress: &dyn Fn(u64, u64),
) -> Result<ChecksumResult, String> {
    let want: String = expected
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    if !want.chars().all(|c| c.is_ascii_hexdigit()) || !(want.len() == 64 || want.len() == 32) {
        return Err("Paste the checksum from the download page: 64 characters (SHA-256) or 32 (MD5), letters a-f and digits.".into());
    }
    let mut file = File::open(path).map_err(|e| format!("The file can't be opened: {e}"))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut buffer = vec![0u8; 4 << 20];
    let mut done = 0u64;
    let (mut sha, mut md5) = (Sha256::new(), Md5::new());
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("The file can't be read: {e}"))?;
        if n == 0 {
            break;
        }
        if want.len() == 64 {
            sha.update(&buffer[..n]);
        } else {
            md5.update(&buffer[..n]);
        }
        done += n as u64;
        progress(done, total);
    }
    let (algorithm, actual) = if want.len() == 64 {
        ("SHA-256", hex(&sha.finalize()))
    } else {
        ("MD5", hex(&md5.finalize()))
    };
    Ok(ChecksumResult {
        algorithm,
        matches: actual == want,
        actual,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Writer {
    /// `rpi-imager` or `etcher`.
    pub id: &'static str,
    pub name: &'static str,
    /// Whether it opens with the image already chosen.
    pub takes_image: bool,
}

/// Where each writer installs itself on Windows (machine-wide and per-user).
fn writer_paths(id: &str) -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut out = Vec::new();
    match id {
        "rpi-imager" => {
            for base in [env("ProgramFiles(x86)"), env("ProgramFiles")]
                .into_iter()
                .flatten()
            {
                out.push(base.join("Raspberry Pi Imager").join("rpi-imager.exe"));
            }
        }
        "etcher" => {
            if let Some(local) = env("LOCALAPPDATA") {
                out.push(
                    local
                        .join("Programs")
                        .join("balena-etcher")
                        .join("balenaEtcher.exe"),
                );
            }
            for base in [env("ProgramFiles"), env("ProgramFiles(x86)")]
                .into_iter()
                .flatten()
            {
                out.push(base.join("balenaEtcher").join("balenaEtcher.exe"));
            }
        }
        _ => {}
    }
    out
}

fn writer_path(id: &str) -> Option<PathBuf> {
    writer_paths(id).into_iter().find(|p| p.is_file())
}

/// The writers installed on this PC.
pub fn writers() -> Vec<Writer> {
    let mut out = Vec::new();
    if writer_path("rpi-imager").is_some() {
        out.push(Writer {
            id: "rpi-imager",
            name: "Raspberry Pi Imager",
            takes_image: true,
        });
    }
    if writer_path("etcher").is_some() {
        out.push(Writer {
            id: "etcher",
            name: "balenaEtcher",
            takes_image: false,
        });
    }
    out
}

/// Open a writer from its install folder; Raspberry Pi Imager gets the image path
/// (one argument, no shell). The user picks the drive inside the writer.
pub fn open_writer(id: &str, image: &str) -> Result<String, String> {
    let exe = writer_path(id).ok_or("That program isn't installed in its usual folder. Install it from its official site first.")?;
    if !Path::new(image).is_file() {
        return Err("The image file can't be found any more.".into());
    }
    let mut command = std::process::Command::new(&exe);
    if id == "rpi-imager" {
        command.arg(image);
    }
    command
        .spawn()
        .map_err(|e| format!("Couldn't start it: {e}"))?;
    Ok(if id == "rpi-imager" {
        "Raspberry Pi Imager is open with your image. Choose your card or USB stick there, then Write. It shows its own warning before erasing anything.".into()
    } else {
        "balenaEtcher is open. Choose Flash from file and pick your image (its path is copied), then Select target, then Flash. It warns before erasing anything.".into()
    })
}

/// Open an official download page in the browser (fixed addresses only).
pub fn open_download_page(id: &str) -> Result<(), String> {
    let url = download_page(id).ok_or("Unknown page.")?;
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't open the browser: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn file(dir: &Path, name: &str, head: &[u8], size: u64, tail: &[u8]) -> String {
        let path = dir.join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(head).unwrap();
        f.set_len(size - tail.len() as u64).unwrap();
        f.seek(SeekFrom::End(0)).unwrap();
        f.write_all(tail).unwrap();
        path.display().to_string()
    }

    #[test]
    fn image_names_say_what_they_are_for() {
        assert_eq!(
            name_facts("batocera-x86_64-41-20241120.img.gz"),
            (
                Some("batocera".into()),
                Some("41".into()),
                Some("PCs (64-bit)".into())
            )
        );
        assert_eq!(
            name_facts("batocera-bcm2711-bcm2711-40-20240801.img.gz"),
            (
                Some("batocera".into()),
                Some("40".into()),
                Some("Raspberry Pi 4 / 400".into())
            )
        );
        assert_eq!(
            name_facts("retropie-buster-4.8-rpi4_400.img.gz"),
            (
                Some("retropie".into()),
                Some("4.8".into()),
                Some("Raspberry Pi 4 / 400".into())
            )
        );
        assert_eq!(
            name_facts("retropie-buster-4.8-rpi2_3_zero2w.img.gz")
                .2
                .as_deref(),
            Some("Raspberry Pi 2 / 3 / Zero 2 W")
        );
        assert_eq!(name_facts("ubuntu-24.04.img.xz"), (None, None, None));
    }

    #[test]
    fn images_are_recognized_and_short_downloads_caught() {
        let dir = tempfile::tempdir().unwrap();
        let gz = file(
            dir.path(),
            "batocera-x86_64-41-20241120.img.gz",
            GZIP,
            60 << 20,
            b"",
        );
        let i = inspect(&gz, Some("batocera")).unwrap();
        assert_eq!(
            (i.format.as_str(), i.problem.clone(), i.notes.len()),
            ("gzip", None, 0)
        );
        // A RetroPie image chosen for Batocera: noted, not refused.
        let rp = file(
            dir.path(),
            "retropie-buster-4.8-rpi4_400.img.gz",
            GZIP,
            60 << 20,
            b"",
        );
        assert!(inspect(&rp, Some("batocera")).unwrap().notes[0]
            .contains("RetroPie image, not Batocera"));
        // xz must end with its footer; a cut-short one is caught.
        let xz_ok = file(dir.path(), "a.img.xz", XZ, 60 << 20, b"YZ");
        assert_eq!(inspect(&xz_ok, None).unwrap().problem, None);
        let xz_cut = file(dir.path(), "b.img.xz", XZ, 60 << 20, b"\0\0");
        assert!(inspect(&xz_cut, None)
            .unwrap()
            .problem
            .unwrap()
            .contains("ends early"));
        // A raw disk image has a partition table.
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        assert_eq!(
            inspect(&file(dir.path(), "c.img", &mbr, 70 << 20, b""), None)
                .unwrap()
                .format,
            "img"
        );
        // Tiny or unknown files.
        assert!(
            inspect(&file(dir.path(), "d.img.gz", GZIP, 1 << 20, b""), None)
                .unwrap()
                .problem
                .unwrap()
                .contains("smaller")
        );
        assert!(
            inspect(&file(dir.path(), "e.iso", b"hello", 60 << 20, b""), None)
                .unwrap()
                .problem
                .unwrap()
                .contains("isn't a disk image")
        );
        assert!(
            inspect(&dir.path().display().to_string(), None).is_err(),
            "a folder isn't an image"
        );
    }

    #[test]
    fn checksums_from_the_download_page_are_compared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.img.gz");
        std::fs::write(&path, b"abc").unwrap();
        let p = path.display().to_string();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let r = checksum(
            &p,
            &format!("  {}  batocera.img.gz\n", sha.to_uppercase()),
            &|_, _| {},
        )
        .unwrap();
        assert_eq!(
            (r.algorithm, r.matches),
            ("SHA-256", true),
            "pasted with spaces, upper case and a file name"
        );
        let r = checksum(&p, "900150983cd24fb0d6963f7d28e17f72", &|_, _| {}).unwrap();
        assert_eq!((r.algorithm, r.matches), ("MD5", true));
        assert!(!checksum(&p, &"0".repeat(64), &|_, _| {}).unwrap().matches);
        assert!(checksum(&p, "not a checksum", &|_, _| {}).is_err());
    }

    #[test]
    fn only_known_programs_and_pages_are_opened() {
        assert_eq!(
            download_page("batocera"),
            Some("https://batocera.org/download")
        );
        assert_eq!(download_page("https://evil.example"), None);
        assert!(open_download_page("https://evil.example").is_err());
        assert!(
            open_writer("notepad", "C:\\x.img").is_err(),
            "only the two writers, from their install folders"
        );
        assert!(writer_paths("rpi-imager")
            .iter()
            .all(|p| p.ends_with("Raspberry Pi Imager\\rpi-imager.exe")));
    }
}
