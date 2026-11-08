//! RetroBat theme builder, step 1: remix an installed theme (docs/18 §13).
//!
//! RetroBat themes live in `emulationstation\.emulationstation\themes\<name>`; the
//! active one is the `ThemeSet` setting. Themes such as Carbon are driven by named
//! variables in `theme.xml` (`baseColor`, `backgroundColor`, `backgroundImage`, …),
//! and *subsets* (e.g. Carbon's colorsets) include files that override those
//! variables; when the user hasn't chosen one, the first entry applies.
//!
//! A remix never touches the original: the whole base theme is copied to a new
//! folder (under a temporary name, renamed when complete), then only the copy is
//! edited: chosen color and image variables in `theme.xml`, a "RetroDrive" entry
//! added first to every subset that sets those variables (so the new colors apply
//! by default), the user's own pictures and fonts copied in, and a note added next
//! to the original credits (theme licenses usually require them; Carbon is
//! CC BY-NC-SA). Making it RetroBat's theme goes through the reviewed, backed-up,
//! undoable settings change in `customizer.rs`.

use crate::copier::TEMP_MARKER;
use crate::volumes::Presence;
use crate::{db, installations};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;
pub type SpaceFn<'a> = &'a dyn Fn(&Path) -> Option<u64>;

const THEMES: &str = "emulationstation/.emulationstation/themes";
const MANIFEST: &str = "retrodrive-theme.json";
const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "svg"];
const FONT_EXTS: [&str; 2] = ["ttf", "otf"];
const MAX_ASSET: u64 = 25 << 20;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ThemeVar {
    pub name: String,
    /// The value RetroBat uses by default (after the first entry of each subset).
    pub value: String,
    /// `color` (RRGGBB or RRGGBBAA) or `image` (a path inside the theme).
    pub kind: String,
    /// For images: the full path, for the preview.
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ThemeInfo {
    pub folder: String,
    pub path: String,
    pub active: bool,
    /// The credits comment at the top of theme.xml (authors, license).
    pub credits: Vec<String>,
    pub variables: Vec<ThemeVar>,
    /// Font files in the theme, relative paths (e.g. `art/fonts/Cabin-Bold.ttf`).
    pub fonts: Vec<String>,
    pub bytes: i64,
    pub files: i64,
    /// Set when RetroDrive made this theme.
    pub remixed_from: Option<String>,
    /// Why it can't be remixed (e.g. no theme.xml).
    pub problem: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThemeList {
    pub installation_path: String,
    pub themes_path: String,
    pub active: Option<String>,
    pub themes: Vec<ThemeInfo>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Remix {
    pub base: String,
    pub name: String,
    /// Color variable -> RRGGBB or RRGGBBAA.
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    /// Image variable -> the user's own picture file.
    #[serde(default)]
    pub images: BTreeMap<String, String>,
    /// Font file in the theme (relative path) -> the user's own font file.
    #[serde(default)]
    pub fonts: BTreeMap<String, String>,
    /// Set when a design creates the remix (recorded in the manifest).
    #[serde(default)]
    pub design: Option<String>,
}

impl Remix {
    /// A stable fingerprint of what the remix contains (to recognise a remix a design already made).
    pub fn recipe(&self) -> String {
        let text = serde_json::json!({ "base": self.base, "name": self.name, "colors": self.colors, "images": self.images, "fonts": self.fonts }).to_string();
        crate::customizer::hex_sha(text.as_bytes())
    }
}

/// The design id and recipe recorded in a theme RetroDrive made, if any.
pub fn made_by(theme_dir: &Path) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(theme_dir.join(MANIFEST)).ok()?).ok()?;
    Some((v["design"].as_str()?.to_string(), v["recipe"].as_str()?.to_string()))
}

/// Size in bytes of a theme folder (what a remix copies).
pub fn theme_bytes(dir: &Path) -> i64 {
    walk_size(dir).0
}

// ---------- reading theme.xml ----------

/// A simple `<name>value</name>` element and where its value sits.
#[derive(Debug, Clone)]
struct Var {
    name: String,
    value: String,
    start: usize,
    end: usize,
}

fn block(text: &str, open: &str, close: &str, from: usize) -> Option<(usize, usize)> {
    let start = text[from..].find(open)? + from + open.len();
    let end = text[start..].find(close)? + start;
    Some((start, end))
}

/// Simple child elements of the first `<variables>` block (comments skipped).
fn variables(text: &str) -> Vec<Var> {
    let Some((start, end)) = block(text, "<variables>", "</variables>", 0) else { return Vec::new() };
    let mut found = Vec::new();
    let mut i = start;
    while let Some(off) = text[i..end].find('<') {
        let open = i + off;
        if text[open..].starts_with("<!--") {
            i = text[open..].find("-->").map(|p| open + p + 3).unwrap_or(end);
            continue;
        }
        let Some(gt) = text[open..end].find('>') else { break };
        let name = &text[open + 1..open + gt];
        if name.is_empty() || name.starts_with('/') || name.contains(|c: char| c.is_whitespace() || c == '/') {
            i = open + gt + 1;
            continue;
        }
        let value_start = open + gt + 1;
        let close = format!("</{name}>");
        let Some(len) = text[value_start..end].find(&close) else {
            i = value_start;
            continue;
        };
        let value = &text[value_start..value_start + len];
        if !value.contains('<') {
            found.push(Var { name: name.to_string(), value: value.trim().to_string(), start: value_start, end: value_start + len });
        }
        i = value_start + len + close.len();
    }
    found
}

fn is_color(value: &str) -> bool {
    (value.len() == 6 || value.len() == 8) && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_image(value: &str) -> bool {
    !value.contains("${") && IMAGE_EXTS.iter().any(|e| value.to_lowercase().ends_with(&format!(".{e}")))
}

/// A `<subset name="…">` block in theme.xml and its include files.
struct Subset {
    name: String,
    /// Byte position of the first `<include` inside it (where ours goes).
    first_include: usize,
    indent: String,
    includes: Vec<String>,
}

fn attribute(tag: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    Some(tag[start..start + tag[start..].find('"')?].to_string())
}

fn subsets(text: &str) -> Vec<Subset> {
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("<subset ") {
        let start = i + off;
        let Some(tag_end) = text[start..].find('>').map(|p| start + p) else { break };
        let Some(end) = text[tag_end..].find("</subset>").map(|p| tag_end + p) else { break };
        let name = attribute(&text[start..tag_end], "name").unwrap_or_default();
        let mut includes = Vec::new();
        let mut first_include = None;
        let mut j = tag_end;
        while let Some(o) = text[j..end].find("<include") {
            let inc = j + o;
            first_include.get_or_insert(inc);
            let Some(gt) = text[inc..end].find('>').map(|p| inc + p) else { break };
            let Some(close) = text[gt..end].find("</include>").map(|p| gt + p) else { break };
            includes.push(text[gt + 1..close].trim().to_string());
            j = close;
        }
        if let (Some(first), false) = (first_include, name.is_empty()) {
            let line_start = text[..first].rfind('\n').map(|p| p + 1).unwrap_or(first);
            let indent = text[line_start..first].to_string();
            found.push(Subset { name, first_include: first, indent: if indent.trim().is_empty() { indent } else { "    ".into() }, includes });
        }
        i = end;
    }
    found
}

/// Resolve a theme-relative path (`./x`, `x`, `${themePath}/x`) safely inside `theme`.
fn inside(theme: &Path, relative: &str) -> Option<PathBuf> {
    let clean = relative.trim().trim_start_matches("${themePath}/").trim_start_matches("./").replace('\\', "/");
    if clean.is_empty() || clean.split('/').any(|p| p == ".." || p.contains(':')) || clean.starts_with('/') {
        return None;
    }
    Some(theme.join(clean))
}

fn credits(text: &str) -> Vec<String> {
    let Some((start, end)) = block(text, "<!--", "-->", 0) else { return Vec::new() };
    if text[..start].contains("<theme") {
        return Vec::new();
    }
    text[start..end].lines().map(str::trim).filter(|l| !l.is_empty()).take(10).map(str::to_string).collect()
}

/// Theme variables as RetroBat uses them by default: theme.xml, then the first
/// include of each subset.
fn effective(theme: &Path, text: &str) -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = variables(text).into_iter().map(|v| (v.name, v.value)).collect();
    for subset in subsets(text) {
        if let Some(path) = subset.includes.first().and_then(|p| inside(theme, p)) {
            if let Ok(included) = fs::read_to_string(path) {
                for v in variables(&included) {
                    map.insert(v.name, v.value);
                }
            }
        }
    }
    map
}

fn walk_size(dir: &Path) -> (i64, i64) {
    let mut bytes = 0;
    let mut files = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() {
            files += 1;
            bytes += entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        }
    }
    (bytes, files)
}

fn info(theme: &Path, folder: &str, active: bool, with_size: bool) -> ThemeInfo {
    let (bytes, files) = if with_size { walk_size(theme) } else { (0, 0) };
    let remixed_from = fs::read_to_string(theme.join(MANIFEST)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["base"].as_str().map(str::to_string));
    let mut out = ThemeInfo { folder: folder.into(), path: theme.display().to_string(), active, credits: Vec::new(), variables: Vec::new(), fonts: Vec::new(), bytes, files, remixed_from, problem: None };
    let Ok(text) = fs::read_to_string(theme.join("theme.xml")) else {
        out.problem = Some("No theme.xml, so it can't be remixed.".into());
        return out;
    };
    out.credits = credits(&text);
    out.variables = effective(theme, &text)
        .into_iter()
        .filter_map(|(name, value)| {
            if is_color(&value) {
                Some(ThemeVar { name, value, kind: "color".into(), path: None })
            } else if is_image(&value) {
                let path = inside(theme, &value).filter(|p| p.is_file()).map(|p| p.display().to_string());
                Some(ThemeVar { name, value, kind: "image".into(), path })
            } else {
                None
            }
        })
        .collect();
    let mut fonts: Vec<String> = walkdir::WalkDir::new(theme)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && FONT_EXTS.iter().any(|x| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case(x))))
        .filter_map(|e| e.path().strip_prefix(theme).ok().map(|p| p.display().to_string().replace('\\', "/")))
        .collect();
    fonts.sort();
    out.fonts = fonts;
    if out.variables.is_empty() {
        out.problem = Some("This theme has no color or picture settings RetroDrive can change.".into());
    }
    out
}

struct Install {
    root: PathBuf,
}

fn install(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Install, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if located.adapter_id != "retrobat" {
        return Err("Themes are only available for RetroBat so far.".into());
    }
    if located.presence != Presence::Connected {
        return Err("The drive isn't connected.".into());
    }
    Ok(Install { root: located.root })
}

/// The installation's themes with what can be changed in each.
pub fn list(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<ThemeList, String> {
    let install = install(connection, installation_id, presence)?;
    let active = crate::customizer::current_theme(&install.root);
    let dir = install.root.join(THEMES);
    let themes = crate::customizer::theme_folders(&install.root)
        .into_iter()
        .map(|folder| {
            let path = if dir.join(&folder).is_dir() { dir.join(&folder) } else { install.root.join("emulationstation/themes").join(&folder) };
            info(&path, &folder, active.as_deref() == Some(folder.as_str()), true)
        })
        .collect();
    Ok(ThemeList { installation_path: install.root.display().to_string(), themes_path: dir.display().to_string(), active, themes })
}

/// A picture or font as a data URL, for the preview (images and fonts only, size-limited).
pub fn preview_asset(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => return Err("Only pictures and fonts can be previewed.".into()),
    };
    let size = fs::metadata(path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?.len();
    if size > MAX_ASSET {
        return Err("That file is too large to preview (25 MB at most).".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("data:{mime};base64,{}", base64(&bytes)))
}

/// Standard base64 (for data URLs).
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A folder name for the new theme (letters, digits, spaces, dashes, underscores).
pub fn folder_for(name: &str) -> Result<String, String> {
    let cleaned: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return Err("Give the new theme a name (letters, numbers, spaces, - or _).".into());
    }
    if cleaned.len() > 60 {
        return Err("Theme names can be up to 60 characters.".into());
    }
    Ok(cleaned)
}

fn user_file(path: &str, exts: &[&str], what: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    let ext = p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    if !exts.contains(&ext.as_str()) {
        return Err(format!("{} isn't a {what} file ({}).", p.display(), exts.join(", ")));
    }
    let meta = fs::metadata(&p).map_err(|e| format!("Couldn't open {}: {e}", p.display()))?;
    if !meta.is_file() || meta.len() > MAX_ASSET {
        return Err(format!("{} must be a file of 25 MB or less.", p.display()));
    }
    Ok(p)
}

/// Copy `from` into the new folder `to` (which must not exist). Links are skipped.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir(to).map_err(|e| format!("Couldn't create {}: {e}", to.display()))?;
    for entry in walkdir::WalkDir::new(from).follow_links(false).min_depth(1) {
        let entry = entry.map_err(|e| e.to_string())?;
        let relative = entry.path().strip_prefix(from).map_err(|e| e.to_string())?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir(&target).map_err(|e| format!("Couldn't create {}: {e}", target.display()))?;
        } else if entry.file_type().is_file() {
            copy_new(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Copy a file to a name that must not exist yet.
fn copy_new(from: &Path, to: &Path) -> Result<(), String> {
    let mut source = fs::File::open(from).map_err(|e| format!("Couldn't read {}: {e}", from.display()))?;
    let mut out = fs::OpenOptions::new().write(true).create_new(true).open(to).map_err(|e| format!("Couldn't create {}: {e}", to.display()))?;
    std::io::copy(&mut source, &mut out).map_err(|e| format!("Couldn't copy {}: {e}", from.display()))?;
    out.sync_all().map_err(|e| e.to_string())
}

/// Replace a file *inside RetroDrive's own unfinished copy* (never the original theme).
fn replace_in_copy(copy_root: &Path, target: &Path, contents_from: Option<&Path>, contents: Option<&[u8]>) -> Result<(), String> {
    if !target.starts_with(copy_root) {
        return Err("Refused to write outside the new theme.".into());
    }
    if target.exists() {
        fs::remove_file(target).map_err(|e| format!("Couldn't update {}: {e}", target.display()))?;
    }
    match (contents_from, contents) {
        (Some(from), _) => copy_new(from, target),
        (None, Some(bytes)) => fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(bytes)?;
                f.sync_all()
            })
            .map_err(|e| format!("Couldn't write {}: {e}", target.display())),
        _ => Ok(()),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The edited theme.xml text: changed variables, a RetroDrive entry first in each
/// subset that sets them, and a note beside the original credits.
fn remixed_theme_xml(text: &str, changed: &BTreeMap<String, String>, subset_files: &[(String, String)], base: &str, date: &str) -> String {
    let mut edits: Vec<(usize, usize, String)> = variables(text).into_iter().filter_map(|v| changed.get(&v.name).map(|value| (v.start, v.end, escape(value)))).collect();
    for subset in subsets(text) {
        if let Some((_, file)) = subset_files.iter().find(|(name, _)| *name == subset.name) {
            let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
            edits.push((subset.first_include, subset.first_include, format!("<include name=\"retrodrive\" displayName=\"RetroDrive\">./{file}</include>{newline}{}", subset.indent)));
        }
    }
    let note = format!("<!-- Remixed with RetroDrive on {date} from the theme \"{}\". The original theme's credits and license below still apply. -->\n", escape(base));
    let at = text.find("?>").map(|p| text[p..].find('\n').map(|n| p + n + 1).unwrap_or(p + 2)).unwrap_or(0);
    edits.push((at, at, note));
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let mut out = text.to_string();
    for (start, end, replacement) in edits {
        out.replace_range(start..end, &replacement);
    }
    out
}

/// Create the remixed theme as a new folder. Returns its folder name.
pub fn create(connection: &Connection, installation_id: &str, remix: &Remix, presence: PresenceFn, free_space: SpaceFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let themes_dir = install.root.join(THEMES);
    if remix.base.contains(['/', '\\', ':']) || remix.base.starts_with('.') {
        return Err("That isn't a theme folder.".into());
    }
    let base_dir = themes_dir.join(&remix.base);
    let base_dir = if base_dir.is_dir() { base_dir } else { install.root.join("emulationstation/themes").join(&remix.base) };
    let base_text = fs::read_to_string(base_dir.join("theme.xml")).map_err(|_| format!("{} has no theme.xml, so it can't be remixed.", remix.base))?;
    let folder = folder_for(&remix.name)?;
    if crate::customizer::theme_folders(&install.root).iter().any(|t| t.eq_ignore_ascii_case(&folder)) {
        return Err(format!("There's already a theme called {folder}. Choose another name."));
    }

    // Validate every requested change against what the base theme has.
    let base = info(&base_dir, &remix.base, false, false);
    let kind = |name: &str| base.variables.iter().find(|v| v.name == name).map(|v| v.kind.clone());
    for (name, value) in &remix.colors {
        if kind(name).as_deref() != Some("color") || !is_color(value) {
            return Err(format!("{name} = {value} isn't a color setting of {} (use RRGGBB or RRGGBBAA).", remix.base));
        }
    }
    let mut images: Vec<(String, PathBuf, String)> = Vec::new();
    for (name, path) in &remix.images {
        if kind(name).as_deref() != Some("image") {
            return Err(format!("{name} isn't a picture setting of {}.", remix.base));
        }
        let file = user_file(path, &["png", "jpg", "jpeg"], "picture")?;
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        images.push((name.clone(), file, format!("art/retrodrive/{safe}.{ext}")));
    }
    let mut fonts: Vec<(String, PathBuf)> = Vec::new();
    for (relative, path) in &remix.fonts {
        if !base.fonts.contains(relative) {
            return Err(format!("{relative} isn't a font in {}.", remix.base));
        }
        fonts.push((relative.clone(), user_file(path, &FONT_EXTS, "font")?));
    }
    if remix.colors.is_empty() && images.is_empty() && fonts.is_empty() {
        return Err("Change at least one color, picture or font.".into());
    }

    // Room for the copy (plus a margin).
    let (bytes, _) = walk_size(&base_dir);
    if let Some(free) = free_space(&themes_dir) {
        if free < bytes as u64 + (200 << 20) {
            return Err(format!("There isn't enough free space on the drive for a copy of {} ({} MB).", remix.base, bytes >> 20));
        }
    }

    let marker = format!("{TEMP_MARKER}{}", &db::new_id()[..8]);
    let temp = themes_dir.join(format!("{folder}{marker}"));
    let result = (|| -> Result<(), String> {
        copy_tree(&base_dir, &temp)?;
        // New values: chosen colors, and images pointing at the copied pictures.
        let mut changed = remix.colors.clone();
        for (name, _, relative) in &images {
            changed.insert(name.clone(), format!("./{relative}"));
        }
        // Subsets that set any changed variable get a RetroDrive entry, used by default.
        let mut subset_files = Vec::new();
        let mut defaults = effective(&base_dir, &base_text);
        defaults.extend(changed.clone());
        for subset in subsets(&base_text) {
            let touches = subset.includes.iter().filter_map(|p| inside(&base_dir, p)).filter_map(|p| fs::read_to_string(p).ok()).any(|t| variables(&t).iter().any(|v| changed.contains_key(&v.name)));
            if !touches {
                continue;
            }
            // The entry sets every variable the subset's own entries set, with the new values.
            let mut names: Vec<String> = Vec::new();
            for include in subset.includes.iter().filter_map(|p| inside(&base_dir, p)).filter_map(|p| fs::read_to_string(p).ok()) {
                for v in variables(&include) {
                    if !names.contains(&v.name) {
                        names.push(v.name);
                    }
                }
            }
            let safe: String = subset.name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let file = format!("subsets/retrodrive/{safe}.xml");
            let mut body = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- Made by RetroDrive for this remix. -->\n<theme>\n  <formatVersion>7</formatVersion>\n  <variables>\n");
            for name in names {
                if let Some(value) = defaults.get(&name) {
                    body.push_str(&format!("    <{name}>{}</{name}>\n", escape(value)));
                }
            }
            body.push_str("  </variables>\n</theme>\n");
            let path = temp.join(&file);
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            replace_in_copy(&temp, &path, None, Some(body.as_bytes()))?;
            subset_files.push((subset.name.clone(), file));
        }
        let date = chrono_date();
        let xml = remixed_theme_xml(&base_text, &changed, &subset_files, &remix.base, &date);
        replace_in_copy(&temp, &temp.join("theme.xml"), None, Some(xml.as_bytes()))?;
        for (_, file, relative) in &images {
            let target = temp.join(relative);
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            replace_in_copy(&temp, &target, Some(file), None)?;
        }
        for (relative, file) in &fonts {
            let target = inside(&temp, relative).ok_or("Bad font path")?;
            replace_in_copy(&temp, &target, Some(file), None)?;
        }
        let manifest = serde_json::json!({
            "format": "retrodrive-theme",
            "format_version": 1,
            "base": remix.base,
            "created": date,
            "colors": remix.colors,
            "images": images.iter().map(|(n, f, r)| (n.clone(), serde_json::json!({ "file": r, "from": f.file_name().map(|x| x.to_string_lossy().into_owned()) }))).collect::<serde_json::Map<_, _>>(),
            "fonts": fonts.iter().map(|(r, f)| (r.clone(), serde_json::json!(f.file_name().map(|x| x.to_string_lossy().into_owned())))).collect::<serde_json::Map<_, _>>(),
            "design": remix.design,
            "recipe": remix.recipe(),
        });
        replace_in_copy(&temp, &temp.join(MANIFEST), None, Some(serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes()))?;
        crate::copier::rename_no_replace(&temp, &themes_dir.join(&folder)).map_err(|e| format!("Couldn't give the new theme its name: {e}"))
    })();
    if let Err(e) = result {
        // Only RetroDrive's own unfinished copy (marked name, inside the themes folder) is removed.
        if temp.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&marker)) && temp.parent() == Some(themes_dir.as_path()) {
            let _ = fs::remove_dir_all(&temp);
        }
        return Err(format!("{e} The new theme wasn't created; {} is unchanged.", remix.base));
    }
    Ok(folder)
}

fn chrono_date() -> String {
    // YYYY-MM-DD from the system clock, without another dependency.
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn roomy(_: &Path) -> Option<u64> {
        Some(1 << 40)
    }
    fn full(_: &Path) -> Option<u64> {
        Some(1 << 20)
    }

    const THEME_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!--\nTheme: es-theme-test\nauthor: Someone\nlicense: creative commons CC-BY-NC-SA\n-->\n<theme defaultView=\"Icons\">\n  <formatVersion>7</formatVersion>\n  <variables>\n    <wideScreen>${screen.ratio} == \"16/9\"</wideScreen>\n    <baseColor>8b0000</baseColor>\n    <bottomBarColor>8b0000c0</bottomBarColor>\n    <backgroundImage>./art/fiber.png</backgroundImage>\n    <logosSubsetName>Logos</logosSubsetName>\n  </variables>\n  <subset name=\"colorset\" displayName=\"Colorset\">\n    <include name=\"blue\" displayName=\"Blue\">./subsets/colorsets/blue.xml</include>\n    <include name=\"red\" displayName=\"Red\">./subsets/colorsets/red.xml</include>\n  </subset>\n  <subset name=\"logos\" displayName=\"Logos\">\n    <include name=\"on\">./subsets/logos/on.xml</include>\n  </subset>\n</theme>\n";

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
        files: PathBuf,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        let theme = root.join(THEMES).join("es-theme-test");
        for d in [theme.join("art/fonts"), theme.join("subsets/colorsets"), theme.join("subsets/logos"), root.join("roms"), root.join("emulationstation/.emulationstation")] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("emulationstation/.emulationstation/es_settings.cfg"), "<?xml version=\"1.0\"?>\n<config>\n\t<string name=\"ThemeSet\" value=\"es-theme-test\" />\n</config>\n").unwrap();
        fs::write(theme.join("theme.xml"), THEME_XML).unwrap();
        fs::write(theme.join("subsets/colorsets/blue.xml"), "<theme><variables><baseColor>3675CA</baseColor><bottomBarColor>4075D8C5</bottomBarColor><backgroundImage>./art/fiber.png</backgroundImage></variables></theme>").unwrap();
        fs::write(theme.join("subsets/colorsets/red.xml"), "<theme><variables><baseColor>aa0000</baseColor></variables></theme>").unwrap();
        fs::write(theme.join("subsets/logos/on.xml"), "<theme><variables><logoSize>1</logoSize></variables></theme>").unwrap();
        fs::write(theme.join("art/fiber.png"), b"fiber").unwrap();
        fs::write(theme.join("art/fonts/Cabin-Bold.ttf"), b"cabin").unwrap();
        let files = dir.path().join("mine");
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("stars.jpg"), b"my picture").unwrap();
        fs::write(files.join("Neon.ttf"), b"my font").unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let install = installations::add(&catalog, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let root = crate::paths::normalize(&root).unwrap();
        Setup { _dir: dir, root, catalog, install, files }
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        walkdir::WalkDir::new(dir).sort_by_file_name().into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| (e.path().to_path_buf(), fs::read(e.path()).unwrap())).collect()
    }

    #[test]
    fn themes_show_what_retrobat_uses_by_default() {
        let s = setup();
        let list = list(&s.catalog, &s.install, &connected).unwrap();
        assert_eq!(list.active.as_deref(), Some("es-theme-test"));
        let t = &list.themes[0];
        assert!(t.active);
        assert!(t.credits.iter().any(|c| c.contains("CC-BY-NC-SA")));
        let get = |n: &str| t.variables.iter().find(|v| v.name == n).cloned();
        assert_eq!(get("baseColor").unwrap().value, "3675CA", "the first colorset applies by default");
        assert_eq!(get("bottomBarColor").unwrap().kind, "color");
        let bg = get("backgroundImage").unwrap();
        assert_eq!((bg.kind.as_str(), bg.path.is_some()), ("image", true));
        assert!(get("wideScreen").is_none() && get("logosSubsetName").is_none(), "only colors and pictures");
        assert_eq!(t.fonts, ["art/fonts/Cabin-Bold.ttf"]);
    }

    #[test]
    fn a_remix_is_a_new_copy_and_the_original_is_untouched() {
        let s = setup();
        let original = s.root.join(THEMES).join("es-theme-test");
        let before = snapshot(&original);
        let remix = Remix {
            base: "es-theme-test".into(),
            name: "My Neon!".into(),
            colors: [("baseColor".to_string(), "ff00ff".to_string())].into_iter().collect(),
            images: [("backgroundImage".to_string(), s.files.join("stars.jpg").display().to_string())].into_iter().collect(),
            fonts: [("art/fonts/Cabin-Bold.ttf".to_string(), s.files.join("Neon.ttf").display().to_string())].into_iter().collect(),
            design: None,
        };
        let folder = create(&s.catalog, &s.install, &remix, &connected, &roomy).unwrap();
        assert_eq!(folder, "My Neon");
        assert_eq!(snapshot(&original), before, "the original theme is unchanged");
        let copy = s.root.join(THEMES).join("My Neon");
        let xml = fs::read_to_string(copy.join("theme.xml")).unwrap();
        assert!(xml.contains("<baseColor>ff00ff</baseColor>"));
        assert!(xml.contains("<backgroundImage>./art/retrodrive/backgroundImage.jpg</backgroundImage>"));
        assert!(xml.contains("Remixed with RetroDrive") && xml.contains("CC-BY-NC-SA"), "credits kept, with a note");
        // Our colorset comes first, so it's what RetroBat uses by default.
        let colorset = xml.find("<subset name=\"colorset\"").unwrap();
        assert!(xml[colorset..].find("retrodrive").unwrap() < xml[colorset..].find("blue.xml").unwrap());
        assert!(!xml[xml.find("<subset name=\"logos\"").unwrap()..].contains("retrodrive"), "untouched subsets stay as they are");
        let ours = fs::read_to_string(copy.join("subsets/retrodrive/colorset.xml")).unwrap();
        assert!(ours.contains("<baseColor>ff00ff</baseColor>") && ours.contains("<bottomBarColor>4075D8C5</bottomBarColor>"));
        assert_eq!(fs::read(copy.join("art/retrodrive/backgroundImage.jpg")).unwrap(), b"my picture");
        assert_eq!(fs::read(copy.join("art/fonts/Cabin-Bold.ttf")).unwrap(), b"my font");
        assert_eq!(fs::read(copy.join("art/fiber.png")).unwrap(), b"fiber", "everything else is copied as is");
        let listed = list(&s.catalog, &s.install, &connected).unwrap();
        let mine = listed.themes.iter().find(|t| t.folder == "My Neon").unwrap();
        assert_eq!(mine.remixed_from.as_deref(), Some("es-theme-test"));
        assert_eq!(mine.variables.iter().find(|v| v.name == "baseColor").unwrap().value, "ff00ff", "the remix's colors apply by default");
        assert!(walkdir::WalkDir::new(s.root.join(THEMES)).into_iter().filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().contains(TEMP_MARKER)));
        assert!(create(&s.catalog, &s.install, &remix, &connected, &roomy).unwrap_err().contains("already"), "names are never reused");
    }

    #[test]
    fn bad_requests_change_nothing() {
        let s = setup();
        let themes = s.root.join(THEMES);
        let before = snapshot(&themes);
        let base = |f: fn(&mut Remix)| {
            let mut r = Remix { base: "es-theme-test".into(), name: "X".into(), colors: [("baseColor".to_string(), "00ff00".to_string())].into_iter().collect(), ..Remix::default() };
            f(&mut r);
            create(&s.catalog, &s.install, &r, &connected, &roomy)
        };
        assert!(base(|r| r.base = "../../Windows".into()).is_err());
        assert!(base(|r| r.name = "  ***  ".into()).is_err());
        assert!(base(|r| r.colors = [("baseColor".to_string(), "red".to_string())].into_iter().collect()).is_err());
        assert!(base(|r| r.colors = [("logosSubsetName".to_string(), "00ff00".to_string())].into_iter().collect()).is_err(), "only color settings");
        assert!(base(|r| r.fonts = [("../../evil.ttf".to_string(), "x.ttf".to_string())].into_iter().collect()).is_err());
        assert!(base(|r| r.images = [("backgroundImage".to_string(), "C:/Windows/notepad.exe".to_string())].into_iter().collect()).is_err());
        assert!(base(|r| r.colors.clear()).is_err(), "something must change");
        let r = Remix { base: "es-theme-test".into(), name: "Big".into(), colors: [("baseColor".to_string(), "00ff00".to_string())].into_iter().collect(), ..Remix::default() };
        assert!(create(&s.catalog, &s.install, &r, &connected, &full).unwrap_err().contains("free space"));
        assert_eq!(snapshot(&themes), before);
    }

    #[test]
    fn previews_only_read_pictures_and_fonts() {
        let s = setup();
        assert!(preview_asset(&s.files.join("stars.jpg").display().to_string()).unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(preview_asset(&s.files.join("Neon.ttf").display().to_string()).unwrap().starts_with("data:font/ttf;base64,"));
        assert!(preview_asset(&s.root.join("RetroBat.exe").display().to_string()).is_err());
    }

    /// Reads (never writes) a real RetroBat theme:
    /// `RETRODRIVE_REAL_THEME=G:\RetroBat\emulationstation\.emulationstation\themes\es-theme-carbon cargo test real_theme -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_theme_reads_and_remixes_in_memory() {
        let Some(theme) = std::env::var_os("RETRODRIVE_REAL_THEME").map(PathBuf::from) else { return };
        let t = info(&theme, "real", false, false);
        eprintln!("credits: {:?}", t.credits);
        for v in &t.variables {
            eprintln!("  {} = {} ({}, file found: {})", v.name, v.value, v.kind, v.path.is_some());
        }
        eprintln!("fonts: {:?}, problem: {:?}", t.fonts, t.problem);
        let text = fs::read_to_string(theme.join("theme.xml")).unwrap();
        let names: Vec<String> = subsets(&text).iter().map(|s| format!("{} ({} entries)", s.name, s.includes.len())).collect();
        eprintln!("subsets: {names:?}");
        let changed: BTreeMap<String, String> = [("baseColor".to_string(), "ff00ff".to_string())].into_iter().collect();
        let out = remixed_theme_xml(&text, &changed, &[("colorset".to_string(), "subsets/retrodrive/colorset.xml".to_string())], "es-theme-carbon", "2026-11-06");
        let i = out.find("<subset name=\"colorset\"").unwrap();
        eprintln!("{}", &out[i..i + 260]);
        assert!(out.contains("<baseColor>ff00ff</baseColor>"));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn dates_are_calendar_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_763), (2026, 11, 6));
    }
}
