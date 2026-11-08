//! Box art and other game images (docs/07 "Game media").
//!
//! RetroDrive never ships or downloads art. It links each game to images the user
//! already has: entries in an EmulationStation `gamelist.xml` (written by scrapers
//! such as Skraper), images following common folder conventions next to the ROMs,
//! or an image the user picks. Only the image's **location** is stored; images are
//! read from where they are when shown, never copied.
//!
//! Images reach the UI through the `rdmedia` protocol, which serves **only** images
//! recorded in `game_media` for the requested game file, only with image file
//! extensions, and only up to `MAX_IMAGE_BYTES`. It never serves arbitrary paths.

use crate::volumes::Presence;
use crate::{db, paths};
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
pub const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
/// Largest `gamelist.xml` RetroDrive will read (real ones are a few MB).
const MAX_GAMELIST_BYTES: u64 = 64 * 1024 * 1024;

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;

pub fn is_image(extension: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&extension)
}

pub fn mime(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/bmp",
    }
}

/// One `<game>` of a gamelist: the ROM path and its images, as written (relative
/// to the gamelist's folder, usually `./…`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GamelistEntry {
    pub path: String,
    /// `(kind, path)`: kinds are `image`, `thumbnail` (often the box) and `marquee`.
    pub images: Vec<(&'static str, String)>,
    /// Details a scraper wrote (ADR-079): description, players (`1-2`), rating (`0.8`).
    pub description: Option<String>,
    pub players: Option<String>,
    pub rating: Option<String>,
}

/// The most players a gamelist `players` value allows (`1-2` → 2, `4` → 4, `2+` → 2).
pub fn players_max(players: &str) -> Option<i64> {
    players
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|n| n.parse::<i64>().ok())
        .filter(|n| (1..=64).contains(n))
        .max()
}

/// A rating as 0–1 (EmulationStation writes 0–1; some tools write 0–5 or 0–10).
pub fn rating_0_to_1(rating: &str) -> Option<f64> {
    let v: f64 = rating.trim().parse().ok()?;
    let v = if v <= 1.0 {
        v
    } else if v <= 5.0 {
        v / 5.0
    } else if v <= 10.0 {
        v / 10.0
    } else {
        return None;
    };
    (v >= 0.0).then_some(v)
}

/// Read the games and image paths from an EmulationStation `gamelist.xml`.
/// Other elements are skipped.
pub fn parse_gamelist(path: &Path) -> Result<Vec<GamelistEntry>, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_GAMELIST_BYTES {
        return Err(format!(
            "{} is too large to read ({size} bytes)",
            path.display()
        ));
    }
    let text = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut reader = Reader::from_reader(text.as_slice());
    let mut entries = Vec::new();
    let mut current: Option<GamelistEntry> = None;
    // The element being read inside a <game>, and its text so far (text arrives in
    // pieces: plain text, entity references such as &amp;, CDATA).
    let mut field: Option<(String, String)> = None;
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(e)) => {
                let name = AsRef::<str>::as_ref(&e.name()).to_lowercase();
                if name == "game" {
                    current = Some(GamelistEntry::default());
                } else if current.is_some() {
                    field = Some((name, String::new()));
                }
            }
            Ok(Event::Text(t)) => {
                if let Some((_, value)) = field.as_mut() {
                    value.push_str(&t.xml10_content());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some((_, value)) = field.as_mut() {
                    value.push_str(&t.xml10_content());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, value)) = field.as_mut() {
                    if let Ok(Some(c)) = r.resolve_char_ref() {
                        value.push(c);
                    } else {
                        match r.clone().into_inner().as_ref() {
                            "amp" => value.push('&'),
                            "lt" => value.push('<'),
                            "gt" => value.push('>'),
                            "quot" => value.push('"'),
                            "apos" => value.push('\''),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = AsRef::<str>::as_ref(&e.name()).to_lowercase();
                if name == "game" {
                    if let Some(entry) = current.take() {
                        if !entry.path.is_empty() {
                            entries.push(entry);
                        }
                    }
                } else if let (Some(entry), Some((field_name, value))) =
                    (current.as_mut(), field.take())
                {
                    let value = value.trim().to_string();
                    if !value.is_empty() && field_name == name {
                        match name.as_str() {
                            "path" => entry.path = value,
                            "image" => entry.images.push(("image", value)),
                            "thumbnail" => entry.images.push(("thumbnail", value)),
                            "marquee" => entry.images.push(("marquee", value)),
                            "desc" => entry.description = Some(value),
                            "players" => entry.players = Some(value),
                            "rating" => entry.rating = Some(value),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("{} isn't valid XML: {e}", path.display())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(entries)
}

/// Resolve `relative` (e.g. `./media/screenshot/x.png`) against the volume-relative
/// folder `base`. Returns `None` if it is absolute or climbs above `base`'s volume.
pub fn resolve_relative(base: &str, relative: &str) -> Option<String> {
    let relative = relative.replace('\\', "/");
    if relative.starts_with('/') || relative.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Common places scrapers and frontends put images, relative to the ROM's folder.
/// `{stem}` is the ROM file name without its extension.
pub const CONVENTIONS: &[(&str, &str)] = &[
    ("boxart", "media/box2dfront/{stem}"),
    ("boxart", "media/boxart/{stem}"),
    ("boxart", "media/covers/{stem}"),
    ("boxart", "boxart/{stem}"),
    ("boxart", "covers/{stem}"),
    ("image", "media/images/{stem}"),
    ("image", "media/screenshot/{stem}"),
    ("image", "media/screenshots/{stem}"),
    ("image", "images/{stem}-image"),
    ("image", "images/{stem}"),
    ("thumbnail", "images/{stem}-thumb"),
    ("image", "{stem}"),
];

/// Where to find an image for a game file, best first.
fn priority(table: &str) -> String {
    format!("CASE {table}.source WHEN 'user' THEN 0 ELSE 1 END, CASE {table}.kind WHEN 'boxart' THEN 0 WHEN 'thumbnail' THEN 1 WHEN 'image' THEN 2 ELSE 3 END, {table}.id")
}

/// SQL expression giving `kind|source|added_at` of a game file's best image (for listings).
pub fn best_media_sql(file_column: &str) -> String {
    format!("(SELECT gm.kind || '|' || gm.source || '|' || gm.added_at FROM game_media gm WHERE gm.game_file_id = {file_column} AND gm.kind <> 'marquee' ORDER BY {} LIMIT 1)", priority("gm"))
}

/// The image to show for a game file: its full path and MIME type, if the image's
/// drive is connected, the file is an image within the size limit.
pub fn resolve(
    connection: &Connection,
    game_file_id: &str,
    presence: PresenceFn,
) -> Result<Option<(PathBuf, &'static str)>, String> {
    let row: Option<(String, Option<String>, String)> = connection
        .query_row(
            &format!(
                "SELECT s.root_path, s.stable_id, m.relative_path FROM game_media m JOIN storage_locations s ON s.id = m.storage_location_id
                 WHERE m.game_file_id = ?1 AND m.kind <> 'marquee' ORDER BY {} LIMIT 1",
                priority("m")
            ),
            [game_file_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((root, stable, relative)) = row else {
        return Ok(None);
    };
    if presence(&root, stable.as_deref()) != Presence::Connected {
        return Ok(None);
    }
    let path = paths::join(&root, &relative);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_image(&extension) {
        return Ok(None);
    }
    match std::fs::metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= MAX_IMAGE_BYTES => Ok(Some((path, mime(&extension)))),
        _ => Ok(None),
    }
}

/// Use an image the user picked as a game's box art (replaces their previous pick).
pub fn set_user_art(
    connection: &Connection,
    game_file_id: &str,
    image: &Path,
    probe: &dyn Fn(&str) -> crate::volumes::VolumeProbe,
) -> Result<(), String> {
    let path =
        paths::normalize(image).map_err(|e| format!("Cannot open {}: {e}", image.display()))?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !is_image(&extension) {
        return Err(format!(
            "Choose an image file ({}).",
            IMAGE_EXTENSIONS.join(", ")
        ));
    }
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err("That image is too large (25 MB at most).".into());
    }
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM game_files WHERE id = ?1)",
            [game_file_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Game not found".into());
    }
    let (volume_root, relative) = paths::split_volume(&path)
        .ok_or_else(|| format!("Unsupported path: {}", path.display()))?;
    let location = db::resolve_storage_location(connection, &volume_root, &probe(&volume_root))?;
    connection
        .execute(
            "INSERT INTO game_media (game_file_id, kind, source, storage_location_id, relative_path) VALUES (?1, 'boxart', 'user', ?2, ?3)
             ON CONFLICT (game_file_id, source, kind) DO UPDATE SET storage_location_id = excluded.storage_location_id,
                 relative_path = excluded.relative_path, added_at = CURRENT_TIMESTAMP",
            params![game_file_id, location.id, relative],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Forget the user's own pick for a game (the image file itself is untouched).
pub fn remove_user_art(connection: &Connection, game_file_id: &str) -> Result<bool, String> {
    connection
        .execute(
            "DELETE FROM game_media WHERE game_file_id = ?1 AND source = 'user'",
            [game_file_id],
        )
        .map(|n| n > 0)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_games_and_images_from_a_gamelist() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gamelist.xml");
        std::fs::write(
            &file,
            r#"<?xml version="1.0"?>
<gameList>
  <provider><System>NES</System><software>Skraper</software></provider>
  <game id="1" source="ScreenScraper.fr">
    <path>./Tom &amp; Jerry (USA).zip</path>
    <name>Tom &amp; Jerry</name>
    <desc>Long text with <![CDATA[odd]]> bits</desc>
    <image>./media/screenshot/Tom &amp; Jerry (USA).png</image>
    <thumbnail>./media/box2dfront/Tom &amp; Jerry (USA).png</thumbnail>
    <marquee>./media/wheel/x.png</marquee>
  </game>
  <game><name>No path, ignored</name></game>
  <folder><path>./sub</path></folder>
</gameList>"#,
        )
        .unwrap();
        let games = parse_gamelist(&file).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].path, "./Tom & Jerry (USA).zip");
        assert_eq!(
            games[0].images,
            [
                (
                    "image",
                    "./media/screenshot/Tom & Jerry (USA).png".to_string()
                ),
                (
                    "thumbnail",
                    "./media/box2dfront/Tom & Jerry (USA).png".to_string()
                ),
                ("marquee", "./media/wheel/x.png".to_string())
            ]
        );
        std::fs::write(&file, "<gameList><game><path>x</gameList>").unwrap();
        assert!(
            parse_gamelist(&file).is_err(),
            "broken XML is reported, not guessed at"
        );
    }

    #[test]
    fn relative_paths_stay_on_their_volume() {
        assert_eq!(
            resolve_relative("roms II/nes", "./media/screenshot/a.png").as_deref(),
            Some("roms II/nes/media/screenshot/a.png")
        );
        assert_eq!(
            resolve_relative("roms II/nes", "..\\shared\\a.png").as_deref(),
            Some("roms II/shared/a.png")
        );
        assert_eq!(
            resolve_relative("nes", "../../a.png"),
            None,
            "can't climb above the volume"
        );
        assert_eq!(resolve_relative("nes", "C:/a.png"), None);
        assert_eq!(resolve_relative("nes", "/a.png"), None);
    }
}
