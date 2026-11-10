//! Save for RetroBat (ADR-113, docs/18 §67): build a theme from an export document and write
//! it to a brand-new folder. The export document is the user's design with its static layers
//! baked into full-screen pictures by the editor; only the live parts (game list, game video,
//! text linked to game data) stay as RetroBat elements.
//!
//! - Baked pictures arrive with the request and are checked like any imported PNG; they are
//!   never added to the picture store.
//! - Every other picture must be declared by the stored theme (same path, hash and size) and
//!   is read from the store with its integrity checks.
//! - Files are written into "<name> (incomplete)" and the folder is renamed only when every
//!   file is in. Nothing is replaced or deleted; a failed save leaves the "(incomplete)" folder.
use super::package::{FileEntry, MAX_PACKAGE_BYTES};
use super::retrobat::compile_candidate;
use crate::theme_studio::{
    image_import::normalize_png,
    image_store::verified_png,
    model::ThemeProject,
    projects::{self, error_message, Result},
    validation::parse_project,
};
use base64::prelude::*;
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// At most this many baked pictures per save (a few per screen is typical).
pub const MAX_BAKED: usize = 16;

/// What a baked picture becomes once checked (no storage).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BakedInfo {
    pub sha256: String,
    pub path: String,
    pub bytes: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedTheme {
    pub folder: String,
    pub name: String,
    pub files: usize,
    pub bytes: u64,
}

fn decode(encoded: &str) -> Result<crate::theme_studio::image_import::NormalizedImage> {
    let input = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| error_message("image_format", "Invalid picture encoding."))?;
    normalize_png(&input).map_err(|e| error_message("image_format", &format!("Picture rejected: {e:?}.")))
}

/// Check a baked picture and say how the export document should declare it.
pub fn prepare_baked(encoded: &str) -> Result<BakedInfo> {
    let image = decode(encoded)?;
    Ok(BakedInfo { sha256: image.sha256, path: image.path, bytes: image.bytes.len(), width: image.width, height: image.height })
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Relative, plain file names only: no drive letters, no `..`, no leading slash.
fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 200
        && !key.starts_with('/')
        && key.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-/".contains(&b))
}

#[derive(Serialize)]
struct Credit<'a> {
    license: &'a str,
    attribution: &'a str,
}
#[derive(Serialize)]
struct Credits<'a> {
    theme: &'a str,
    theme_author: &'a str,
    theme_license: &'a str,
    note: &'static str,
    art: Vec<Credit<'a>>,
}
#[derive(Serialize)]
struct ThemeManifest {
    format: &'static str,
    made_with: &'static str,
    target: &'static str,
    source_sha256: String,
    files: Vec<FileEntry>,
    notes: Vec<String>,
}

/// Credits for the art on the screens being saved: every picture the stored theme uses there.
fn credits<'a>(stored: &'a ThemeProject, roles: &BTreeSet<&str>) -> Vec<Credit<'a>> {
    let used: BTreeSet<&str> = stored
        .scenes
        .iter()
        .filter(|s| roles.contains(s.role.as_str()))
        .flat_map(|s| s.nodes.iter().flat_map(|n| n.assets.iter().map(String::as_str)))
        .collect();
    let mut art: Vec<Credit> = stored
        .assets
        .iter()
        .filter(|a| used.contains(a.id.as_str()))
        .map(|a| Credit { license: &a.license, attribution: if a.attribution.trim().is_empty() { "No credit given" } else { &a.attribution } })
        .collect();
    art.sort_by(|a, b| (a.attribution, a.license).cmp(&(b.attribution, b.license)));
    art.dedup_by(|a, b| a.attribution == b.attribution && a.license == b.license);
    art
}

/// Build every file of the theme in memory (nothing is written here).
pub fn build(
    connection: &Connection,
    project_id: &str,
    expected_revision: i64,
    export_json: &str,
    baked: &[String],
) -> Result<BTreeMap<String, Vec<u8>>> {
    if baked.len() > MAX_BAKED {
        return Err(error_message("package_limit", "Too many baked pictures."));
    }
    let tx = connection.unchecked_transaction()?;
    let record = projects::open(&tx, project_id)?;
    if record.revision != expected_revision {
        return Err(error_message("conflict", "The theme changed while saving; try again."));
    }
    let stored = parse_project(record.draft_json.as_deref().unwrap_or(&record.saved_json))
        .map_err(|_| error_message("invalid_document", "The saved theme is invalid."))?;
    let export = parse_project(export_json)
        .map_err(|issues| error_message("invalid_document", &format!("The theme to save is invalid: {issues:?}")))?;
    let candidate = compile_candidate(export_json).map_err(|e| error_message("compile_blocked", &e.0))?;
    let mut baked_bytes = BTreeMap::new();
    for encoded in baked {
        let image = decode(encoded)?;
        baked_bytes.insert(image.sha256, image.bytes);
    }
    let mut files = BTreeMap::new();
    let mut total = candidate.xml.len();
    files.insert(candidate.path.clone(), candidate.xml.into_bytes());
    for required in &candidate.required_assets {
        let declared = export
            .assets
            .iter()
            .find(|a| a.path == required.path && a.sha256 == required.sha256 && a.bytes == required.bytes)
            .ok_or_else(|| error_message("image_authorization", "A picture isn't declared by the theme."))?;
        let bytes = match baked_bytes.get(&declared.sha256) {
            Some(bytes) if bytes.len() as u64 == declared.bytes && declared.path == format!("assets/images/{}.png", declared.sha256) => bytes.clone(),
            _ => {
                let owned = stored
                    .assets
                    .iter()
                    .find(|a| a.path == declared.path && a.sha256 == declared.sha256 && a.bytes == declared.bytes)
                    .ok_or_else(|| error_message("image_authorization", "A picture doesn't belong to this theme."))?;
                verified_png(&tx, owned)?
            }
        };
        total = total.saturating_add(bytes.len());
        if total > MAX_PACKAGE_BYTES {
            return Err(error_message("package_limit", "The theme is larger than 32 MB."));
        }
        files.insert(declared.path.clone(), bytes);
    }
    let roles: BTreeSet<&str> = export.scenes.iter().map(|s| s.role.as_str()).collect();
    let credits = serde_json::to_vec_pretty(&Credits {
        theme: &stored.metadata.name,
        theme_author: &stored.metadata.author,
        theme_license: &stored.metadata.license,
        note: "Pictures in this theme were combined by RetroDrive Theme Studio from the art credited below.",
        art: credits(&stored, &roles),
    })
    .map_err(|_| error_message("package_json", "Credits couldn't be written."))?;
    files.insert("credits.json".into(), credits);
    let mut notes = vec!["Made with RetroDrive Theme Studio. Check it in RetroBat before sharing it.".to_string()];
    if stored.metadata.license.trim().eq_ignore_ascii_case("unknown") {
        notes.push("The theme's license is \"unknown\": fine for your own use; set one before sharing.".into());
    }
    let manifest = ThemeManifest {
        format: "retrodrive.retrobat-theme.v1",
        made_with: "RetroDrive Theme Studio",
        target: "retrobat-format7",
        source_sha256: digest(export_json.as_bytes()),
        files: files.iter().map(|(path, bytes)| FileEntry { path: path.clone(), bytes: bytes.len(), sha256: digest(bytes) }).collect(),
        notes,
    };
    files.insert(
        "retrodrive-theme.json".into(),
        serde_json::to_vec_pretty(&manifest).map_err(|_| error_message("package_json", "The file list couldn't be written."))?,
    );
    if let Some(bad) = files.keys().find(|k| !safe_key(k)) {
        return Err(error_message("package_path", &format!("Unsafe file name in the theme: {bad}")));
    }
    // No commit: a read transaction can't change the theme or the picture store.
    Ok(files)
}

/// Write the files to a new folder in `destination`, named after the theme (never replacing one).
pub fn write_new_folder(destination: &Path, theme_name: &str, files: &BTreeMap<String, Vec<u8>>) -> std::result::Result<SavedTheme, String> {
    if let Some(bad) = files.keys().find(|k| !safe_key(k)) {
        return Err(format!("Unsafe file name in the theme: {bad}"));
    }
    fs::create_dir_all(destination).map_err(|e| format!("Couldn't create {}: {e}", destination.display()))?;
    let base = crate::system_backup::safe_name(theme_name.trim());
    let base = if base.is_empty() { "RetroDrive theme".to_string() } else { base };
    let name = crate::system_backup::free_folder(destination, &base)?;
    let working = destination.join(format!("{name}{}", crate::system_backup::INCOMPLETE));
    fs::create_dir(&working).map_err(|e| format!("Couldn't create the theme folder: {e}"))?;
    let mut bytes = 0u64;
    for (key, content) in files {
        let target = key.split('/').fold(working.clone(), |path, part| path.join(part));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Couldn't create a folder for {key}: {e}. The partly saved theme is \"{name}{}\".", crate::system_backup::INCOMPLETE))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| format!("Couldn't write {key}: {e}. The partly saved theme is \"{name}{}\".", crate::system_backup::INCOMPLETE))?;
        file.write_all(content).and_then(|_| file.sync_all()).map_err(|e| format!("Couldn't write {key}: {e}"))?;
        if fs::read(&target).map_err(|e| e.to_string())? != *content {
            return Err(format!("{key} didn't read back correctly. The partly saved theme is \"{name}{}\".", crate::system_backup::INCOMPLETE));
        }
        bytes += content.len() as u64;
    }
    let done = destination.join(&name);
    crate::copier::rename_no_replace(&working, &done).map_err(|e| format!("The theme was saved but couldn't be renamed: {e}. It's in {}.", working.display()))?;
    Ok(SavedTheme { folder: done.display().to_string(), name, files: files.len(), bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const SOURCE: &str = include_str!("../../../tests/fixtures/theme_studio/compiler-minimal.json");

    fn png(rgba: [u8; 4]) -> String {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
            writer.finish().unwrap();
        }
        BASE64_STANDARD.encode(bytes)
    }

    /// An export document: one baked full-screen picture plus the game title as a live element.
    fn export_doc(baked: &BakedInfo) -> String {
        let mut doc: Value = serde_json::from_str(SOURCE).unwrap();
        let mut title = doc["scenes"][0]["nodes"][0].clone();
        title["id"] = json!("title");
        title["bindings"] = json!({"text": {"source": "game.title", "kind": "text"}});
        let mut picture = title.clone();
        picture["id"] = json!("baked-game_list-1");
        picture["type"] = json!("image");
        picture["style"] = json!({"imageFit": "contain"});
        picture["bindings"] = json!({});
        picture["assets"] = json!(["baked-1"]);
        doc["scenes"][0]["nodes"] = json!([picture, title]);
        doc["assets"] = json!([{"id": "baked-1", "path": baked.path, "sha256": baked.sha256, "bytes": baked.bytes, "mediaType": "image/png", "license": "unknown", "attribution": "This theme's own art", "source": "Baked by RetroDrive Theme Studio"}]);
        doc.to_string()
    }

    #[test]
    fn a_theme_is_built_from_baked_pictures_and_live_parts_without_storing_anything() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut connection, None).unwrap();
        let record = projects::create(&connection, SOURCE).unwrap();
        let encoded = png([200, 40, 90, 255]);
        let baked = prepare_baked(&encoded).unwrap();
        let export = export_doc(&baked);
        let files = build(&connection, &record.id, record.revision, &export, std::slice::from_ref(&encoded)).unwrap();
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(names, vec![baked.path.as_str(), "credits.json", "retrodrive-theme.json", "theme.xml"]);
        let xml = String::from_utf8(files["theme.xml"].clone()).unwrap();
        assert!(xml.contains("name=\"md_name\"") && xml.contains(&format!("./{}", baked.path)), "{xml}");
        assert_eq!(digest(&files[&baked.path]), baked.sha256);
        let manifest: Value = serde_json::from_slice(&files["retrodrive-theme.json"]).unwrap();
        assert_eq!(manifest["files"].as_array().unwrap().len(), 3, "the file list covers every other file");
        // Nothing was added to the picture store.
        let stored: i64 = connection.query_row("SELECT COUNT(*) FROM theme_image_assets", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, 0);

        // Refused: the baked picture missing, a stale revision, a picture that isn't the theme's.
        assert!(build(&connection, &record.id, record.revision, &export, &[]).is_err());
        assert!(build(&connection, &record.id, record.revision + 1, &export, std::slice::from_ref(&encoded)).is_err());
        let other = prepare_baked(&png([1, 2, 3, 255])).unwrap();
        assert!(build(&connection, &record.id, record.revision, &export_doc(&other), &[encoded]).is_err());
        assert!(build(&connection, &record.id, record.revision, &export, &vec![String::new(); MAX_BAKED + 1]).is_err());
    }

    #[test]
    fn only_plain_relative_file_names_are_written() {
        for good in ["theme.xml", "assets/images/abc.png", "credits.json"] {
            assert!(safe_key(good), "{good}");
        }
        for bad in ["", "/theme.xml", "../x.xml", "assets/../../x", "C:/x", "a//b", "Theme.xml", "a\\b", "./x"] {
            assert!(!safe_key(bad), "{bad}");
        }
    }

    #[test]
    fn a_theme_goes_to_a_new_folder_and_never_replaces_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = BTreeMap::new();
        files.insert("theme.xml".to_string(), b"<theme/>".to_vec());
        files.insert("assets/images/a.png".to_string(), b"png".to_vec());
        let first = write_new_folder(dir.path(), "My Theme", &files).unwrap();
        assert_eq!(first.name, "My Theme");
        assert_eq!(fs::read(dir.path().join("My Theme/assets/images/a.png")).unwrap(), b"png");
        assert_eq!((first.files, first.bytes), (2, 11));
        // The existing folder is left alone; the second save gets a new name.
        fs::write(dir.path().join("My Theme/theme.xml"), b"user edit").unwrap();
        let second = write_new_folder(dir.path(), "My Theme", &files).unwrap();
        assert_eq!(second.name, "My Theme (2)");
        assert_eq!(fs::read(dir.path().join("My Theme/theme.xml")).unwrap(), b"user edit");
        assert!(!fs::read_dir(dir.path()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("incomplete")));
        // Unsafe names are refused before anything is written.
        let mut bad = files.clone();
        bad.insert("../escape.txt".into(), b"x".to_vec());
        assert!(write_new_folder(dir.path(), "Other", &bad).is_err());
        assert!(!dir.path().join("Other").exists() && !dir.path().join("Other (incomplete)").exists());
        assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
    }
}
