//! Emulator settings for RetroBat (RDS-021 version 1; docs/18 §12, owner-approved
//! 2026-11-06).
//!
//! RetroBat keeps the choices made in its menus in
//! `emulationstation\.emulationstation\es_settings.cfg` and writes them into each
//! emulator's own configuration every time a game starts
//! (https://wiki.retrobat.org/navigation/configuration-of-systems-and-games), so this
//! changes that file, never emulator files. Its entries are
//! `<string name="<system>.<key>" value="…" />`; RetroBat's launcher reads every
//! element's `name`/`value` and takes a system's settings from the keys starting with
//! `<system>.` (`emulatorLauncher/Program.cs` and `ConfigFile.cs` in
//! RetroBat-Official/emulatorlauncher, checked 2026-11-06). A missing key means AUTO.
//!
//! Version 1 sets, per system: emulator and core, shader set, decorations (bezel),
//! aspect ratio and integer scaling. The choices offered are read from the user's own
//! install (`es_systems.cfg`, `es_features.cfg`, the shader and decoration folders).
//!
//! Safety: refused while RetroBat runs (it rewrites the file when it closes); the file
//! must be unchanged since the user reviewed the change (SHA-256); only the changed
//! entries are edited, everything else stays byte for byte; the new file is written
//! to a temporary file and read back; the current file is moved (never deleted) into
//! `.retrodrive\backup\settings-<id>\…`, then the new file takes its name. Undo swaps
//! them back the same way. An interrupted change is put right at startup.

use crate::copier::{self, BACKUP_DIR, TEMP_MARKER};
use crate::volumes::Presence;
use crate::{db, installations, paths};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub type PresenceFn<'a> = &'a dyn Fn(&str, Option<&str>) -> Presence;
/// Whether RetroBat (its interface or launcher) is running.
pub type RunningFn<'a> = &'a dyn Fn() -> bool;

/// `old_sha` of a change that created its file (there was no previous version).
const NO_FILE: &str = "none";
pub(crate) const SETTINGS: &str = "emulationstation/.emulationstation/es_settings.cfg";
const SYSTEMS: &str = "emulationstation/.emulationstation/es_systems.cfg";
const FEATURES: &str = "emulationstation/.emulationstation/es_features.cfg";
/// Per-system settings version 1 can change (`<system>.<key>`).
pub const KEYS: [&str; 6] = ["emulator", "core", "shaderset", "bezel", "ratio", "integerscale"];
const MAX_FILE: u64 = 16 << 20;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct EmulatorOption {
    pub emulator: String,
    /// Cores this emulator offers (empty for standalone emulators).
    pub cores: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SystemSettings {
    /// RetroBat's system id, e.g. `snes`.
    pub system: String,
    pub name: String,
    /// Whether `roms\<system>` holds any games.
    pub has_games: bool,
    pub emulators: Vec<EmulatorOption>,
    /// Current values of the keys version 1 handles (missing = AUTO).
    pub current: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Choice {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsOptions {
    pub installation_path: String,
    pub systems: Vec<SystemSettings>,
    pub shaders: Vec<Choice>,
    pub bezels: Vec<Choice>,
    pub ratios: Vec<Choice>,
    pub integer_scaling: Vec<Choice>,
    /// SHA-256 of es_settings.cfg as read; apply refuses if it changed.
    pub file_sha: String,
    /// Why changes can't be applied right now (e.g. RetroBat is running).
    pub blocked: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Change {
    pub system: String,
    pub key: String,
    /// `None` = AUTO (remove the entry).
    pub value: Option<String>,
    /// One game of the system (its settings name, ADR-084), or the whole system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
}

/// One RetroBat system's games and the settings each already has of its own (ADR-084).
#[derive(Debug, Serialize)]
pub struct GameSettings {
    pub games: Vec<String>,
    /// Game → setting → value, for games with settings of their own.
    pub current: BTreeMap<String, BTreeMap<String, String>>,
}

/// The settings name of a change: `snes.ratio`, or `snes["F-Zero (USA).zip"].ratio`.
fn change_key(c: &Change) -> String {
    match &c.game {
        Some(game) => format!("{}[\"{game}\"].{}", c.system, c.key),
        None => format!("{}.{}", c.system, c.key),
    }
}

/// A game's settings must be for a game in that system's folder.
fn check_games(root: &Path, changes: &[Change]) -> Result<(), String> {
    for c in changes {
        if let Some(game) = &c.game {
            let ok = crate::batocera_settings::split_key(&change_key(c)).is_some_and(|(_, g, _)| g == Some(game.as_str()));
            if !ok || !crate::batocera_settings::game_names(&root.join("roms").join(&c.system)).iter().any(|n| n == game) {
                return Err(format!("{game} isn't in RetroBat's {} folder.", c.system));
            }
        }
    }
    Ok(())
}

/// The games in one of RetroBat's system folders and their own settings (reads only).
pub fn game_settings(connection: &Connection, installation_id: &str, system: &str, presence: PresenceFn) -> Result<GameSettings, String> {
    let install = install(connection, installation_id, presence)?;
    if !offer(&install.root)?.systems.iter().any(|(s, _, _)| s == system) {
        return Err(format!("{system} isn't a system in this RetroBat."));
    }
    let text = read_text(&install.root.join(SETTINGS))?;
    let mut current: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (name, value) in values(&entries(&text)?)? {
        if let Some((s, Some(game), key)) = crate::batocera_settings::split_key(&name) {
            if s == system && KEYS.contains(&key) {
                current.entry(game.to_string()).or_default().insert(key.to_string(), value);
            }
        }
    }
    Ok(GameSettings { games: crate::batocera_settings::game_names(&install.root.join("roms").join(system)), current })
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChangeRow {
    pub system: String,
    pub key: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub changes: Vec<ChangeRow>,
    pub file_sha: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryRow {
    pub id: i64,
    pub when: String,
    /// `applied`, `undone`, `failed` (or `applying`/`undoing` while interrupted).
    pub state: String,
    pub changes: Vec<ChangeRow>,
    pub message: Option<String>,
    /// Why undo isn't possible right now; `None` means it can be tried.
    pub blocked: Option<String>,
}

// ---------- es_settings.cfg: read entries with their exact positions ----------

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    name: String,
    value: String,
    /// Byte range of the element (`<string … />`).
    start: usize,
    end: usize,
}

pub(crate) fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32),
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=semi]),
        }
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Attributes of one tag's inside (`string name="a" value="b" /`).
fn attributes(tag: &str) -> HashMap<String, String> {
    let mut found = HashMap::new();
    let bytes = tag.as_bytes();
    let mut i = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !(bytes[i] as char).is_whitespace() && bytes[i] != b'/' {
            i += 1;
        }
        let name = &tag[name_start..i];
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            i += 1;
            continue;
        }
        i += 1;
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || (bytes[i] != b'"' && bytes[i] != b'\'') {
            continue;
        }
        let quote = bytes[i];
        let value_start = i + 1;
        let Some(len) = bytes[value_start..].iter().position(|&b| b == quote) else { break };
        found.insert(name.to_string(), unescape(&tag[value_start..value_start + len]));
        i = value_start + len + 1;
    }
    found
}

/// Every `<string|bool|int|float name=… value=… />` in the file, with its position.
pub(crate) fn entries(text: &str) -> Result<Vec<Entry>, String> {
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(offset) = text[i..].find('<') {
        let start = i + offset;
        if text[start..].starts_with("<!--") {
            i = start + text[start..].find("-->").ok_or("An unfinished comment in es_settings.cfg.")? + 3;
            continue;
        }
        // Find the closing '>' outside quotes.
        let bytes = text.as_bytes();
        let mut j = start + 1;
        let mut quote: Option<u8> = None;
        while j < bytes.len() {
            match (quote, bytes[j]) {
                (None, b'"' | b'\'') => quote = Some(bytes[j]),
                (Some(q), b) if b == q => quote = None,
                (None, b'>') => break,
                _ => {}
            }
            j += 1;
        }
        if j >= bytes.len() {
            return Err("es_settings.cfg ends in the middle of an element.".into());
        }
        let inside = &text[start + 1..j];
        let element = inside.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        if ["string", "bool", "int", "float"].contains(&element) {
            let attrs = attributes(inside);
            if let (Some(name), Some(value)) = (attrs.get("name"), attrs.get("value")) {
                if !inside.trim_end().ends_with('/') {
                    return Err(format!("The setting {name} in es_settings.cfg isn't written the usual way, so RetroDrive won't edit the file."));
                }
                found.push(Entry { name: name.clone(), value: value.clone(), start, end: j + 1 });
            }
        }
        i = j + 1;
    }
    Ok(found)
}

/// Parses with a real XML reader, as RetroBat does: a file it couldn't read would
/// make RetroBat ignore every setting.
pub(crate) fn well_formed(text: &str) -> Result<(), String> {
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => return Ok(()),
            Err(e) => return Err(format!("not valid XML: {e}")),
            _ => {}
        }
    }
}

pub(crate) fn values(list: &[Entry]) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for e in list {
        if map.insert(e.name.clone(), e.value.clone()).is_some() {
            return Err(format!("es_settings.cfg lists {} twice, so RetroDrive won't edit it.", e.name));
        }
    }
    Ok(map)
}

/// Apply changes to the file text, touching only the changed entries. Returns the new text.
fn edit(text: &str, wanted: &BTreeMap<String, Option<String>>) -> Result<String, String> {
    edit_typed(text, wanted, &BTreeMap::new())
}

/// Like `edit`, with the element type per key (`string` when not listed): RetroBat
/// reads `<bool>` and `<int>` settings only from elements of that type.
pub(crate) fn edit_typed(text: &str, wanted: &BTreeMap<String, Option<String>>, tags: &BTreeMap<String, &str>) -> Result<String, String> {
    let tag = |name: &str| -> &str { tags.get(name).copied().unwrap_or("string") };
    let list = entries(text)?;
    let before = values(&list)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    // Replace or remove existing entries, last first so positions stay valid.
    let mut out = text.to_string();
    let mut existing: Vec<&Entry> = list.iter().filter(|e| wanted.contains_key(&e.name)).collect();
    existing.sort_by_key(|e| std::cmp::Reverse(e.start));
    for e in existing {
        match &wanted[&e.name] {
            Some(value) => out.replace_range(e.start..e.end, &format!("<{} name=\"{}\" value=\"{}\" />", tag(&e.name), escape(&e.name), escape(value))),
            None => {
                // Remove the whole line when the element is alone on it.
                let line_start = out[..e.start].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let alone_before = out[line_start..e.start].trim().is_empty();
                let after = &out[e.end..];
                let line_end = after.find('\n').map(|p| e.end + p + 1).unwrap_or(out.len());
                let alone_after = out[e.end..line_end].trim().is_empty();
                if alone_before && alone_after {
                    out.replace_range(line_start..line_end, "");
                } else {
                    out.replace_range(e.start..e.end, "");
                }
            }
        }
    }
    // Add new entries before </config>, indented like the others. ES-DE's
    // es_settings.xml has no <config> wrapper (a flat list), so there they go at the end.
    let additions: Vec<(&String, &String)> = wanted.iter().filter(|(k, _)| !before.contains_key(*k)).filter_map(|(k, v)| v.as_ref().map(|v| (k, v))).collect();
    if !additions.is_empty() {
        let indent = list.first().map(|e| {
            let s = text[..e.start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            text[s..e.start].to_string()
        });
        let indent = indent.filter(|i| i.trim().is_empty()).unwrap_or_else(|| "\t".to_string());
        let mut block = String::new();
        for (name, value) in additions {
            block.push_str(&format!("{indent}<{} name=\"{}\" value=\"{}\" />{newline}", tag(name), escape(name), escape(value)));
        }
        match out.rfind("</config>") {
            Some(close) => {
                let line_start = out[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
                if out[line_start..close].trim().is_empty() {
                    out.insert_str(line_start, &block);
                } else {
                    out.insert_str(close, &format!("{newline}{block}"));
                }
            }
            None if !out.contains("<config") => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push_str(newline);
                }
                out.push_str(&block);
            }
            None => return Err("es_settings.cfg has no </config>, so RetroDrive won't edit it.".into()),
        }
    }
    // Check the result: valid XML, wanted values in place, every other entry unchanged.
    well_formed(&out).map_err(|e| format!("The edited settings would be {e}; nothing was changed."))?;
    let after = values(&entries(&out)?)?;
    for (name, value) in &before {
        if !wanted.contains_key(name) && after.get(name) != Some(value) {
            return Err(format!("Editing would have changed {name}; nothing was changed."));
        }
    }
    for (name, value) in wanted {
        if after.get(name) != value.as_ref() {
            return Err(format!("Editing didn't set {name} as intended; nothing was changed."));
        }
    }
    if after.len() != before.len() + wanted.iter().filter(|(k, v)| !before.contains_key(*k) && v.is_some()).count() - wanted.iter().filter(|(k, v)| before.contains_key(*k) && v.is_none()).count() {
        return Err("Editing would have added or lost other settings; nothing was changed.".into());
    }
    Ok(out)
}

// ---------- What the install offers ----------

pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    let size = fs::metadata(path).map_err(|_| format!("{} wasn't found. Start RetroBat once so it creates it.", path.display()))?.len();
    if size > MAX_FILE {
        return Err(format!("{} is unexpectedly large, so RetroDrive won't read it.", path.display()));
    }
    fs::read_to_string(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))
}

fn attribute(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| AsRef::<str>::as_ref(&a.key) == key)
        .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

/// Systems, their names and emulators/cores from es_systems.cfg.
fn read_systems(path: &Path) -> Result<Vec<(String, String, Vec<EmulatorOption>)>, String> {
    let text = read_text(path)?;
    let mut reader = Reader::from_str(&text);
    let mut systems = Vec::new();
    let (mut in_system, mut field, mut name, mut full) = (false, String::new(), String::new(), String::new());
    let mut emulators: Vec<EmulatorOption> = Vec::new();
    let mut buffer = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = AsRef::<str>::as_ref(&e.name()).to_string();
                match tag.as_str() {
                    "system" => {
                        in_system = true;
                        name.clear();
                        full.clear();
                        emulators.clear();
                    }
                    "emulator" if in_system => emulators.push(EmulatorOption { emulator: attribute(&e, "name").unwrap_or_default(), cores: Vec::new() }),
                    _ => {}
                }
                field = tag;
                buffer.clear();
            }
            Ok(Event::Empty(e)) => {
                if in_system && AsRef::<str>::as_ref(&e.name()) == "emulator" {
                    emulators.push(EmulatorOption { emulator: attribute(&e, "name").unwrap_or_default(), cores: Vec::new() });
                }
            }
            Ok(Event::Text(t)) => buffer.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    buffer.push(c);
                } else if r.clone().into_inner().as_ref() == "amp" {
                    buffer.push('&');
                }
            }
            Ok(Event::End(e)) => {
                let tag = AsRef::<str>::as_ref(&e.name()).to_string();
                if in_system && tag == field {
                    match tag.as_str() {
                        "name" => name = buffer.trim().to_string(),
                        "fullname" => full = buffer.trim().to_string(),
                        "core" => {
                            if let Some(last) = emulators.last_mut() {
                                last.cores.push(buffer.trim().to_string());
                            }
                        }
                        _ => {}
                    }
                }
                if tag == "system" && in_system {
                    in_system = false;
                    if !name.is_empty() {
                        emulators.retain(|e| !e.emulator.is_empty());
                        systems.push((name.clone(), if full.is_empty() { name.clone() } else { full.clone() }, emulators.clone()));
                    }
                }
                field.clear();
                buffer.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("{} isn't valid XML: {e}", path.display())),
            _ => {}
        }
    }
    Ok(systems)
}

/// The aspect-ratio choices RetroBat offers (the shared `ratio` feature in es_features.cfg).
fn read_ratios(path: &Path) -> Vec<Choice> {
    let Ok(text) = read_text(path) else { return Vec::new() };
    let mut reader = Reader::from_str(&text);
    let mut inside = false;
    let mut found = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if AsRef::<str>::as_ref(&e.name()) == "feature" && found.is_empty() => inside = attribute(&e, "value").as_deref() == Some("ratio"),
            Ok(Event::Empty(e)) if inside && AsRef::<str>::as_ref(&e.name()) == "choice" => {
                if let (Some(label), Some(value)) = (attribute(&e, "name"), attribute(&e, "value")) {
                    found.push(Choice { label, value });
                }
            }
            Ok(Event::End(e)) if AsRef::<str>::as_ref(&e.name()) == "feature" => {
                if inside {
                    break;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    found
}

fn folders(paths: &[PathBuf]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for path in paths {
        if let Ok(read) = fs::read_dir(path) {
            for entry in read.filter_map(Result::ok) {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    names.insert(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    names.into_iter().collect()
}

fn has_games(root: &Path, system: &str) -> bool {
    fs::read_dir(root.join("roms").join(system)).map(|read| read.filter_map(Result::ok).any(|e| !e.file_name().to_string_lossy().eq_ignore_ascii_case("_info.txt"))).unwrap_or(false)
}

pub(crate) struct Install {
    pub id: String,
    pub root: PathBuf,
    pub root_text: String,
    pub stable: Option<String>,
}

pub(crate) fn install(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Install, String> {
    install_for(connection, installation_id, presence, &["retrobat"], "Emulator settings are only available for RetroBat so far.")
}

/// The installation, if its platform is one of `adapters` (e.g. game lists: RetroBat and Batocera).
pub(crate) fn install_for(connection: &Connection, installation_id: &str, presence: PresenceFn, adapters: &[&str], refusal: &str) -> Result<Install, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if !adapters.contains(&located.adapter_id.as_str()) {
        return Err(refusal.into());
    }
    if located.presence != Presence::Connected {
        return Err("The drive isn't connected.".into());
    }
    let stable: Option<String> = connection
        .query_row("SELECT s.stable_id FROM installations i JOIN storage_locations s ON s.id = i.storage_location_id WHERE i.id = ?1", [installation_id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let root_text = located.root.display().to_string();
    Ok(Install { id: located.id, root: located.root, root_text, stable })
}

pub(crate) fn running_message(running: RunningFn) -> Option<String> {
    running().then(|| "RetroBat or ES-DE is running. Close it first: it saves its own settings and game lists when it closes, which would undo these changes.".to_string())
}

pub(crate) struct Offer {
    pub systems: Vec<(String, String, Vec<EmulatorOption>)>,
    pub shaders: Vec<Choice>,
    pub bezels: Vec<Choice>,
    pub ratios: Vec<Choice>,
    pub integer: Vec<Choice>,
}

pub(crate) fn offer(root: &Path) -> Result<Offer, String> {
    let systems = read_systems(&root.join(SYSTEMS))?;
    let mut shaders = vec![Choice { label: "None".into(), value: "none".into() }];
    shaders.extend(folders(&[root.join("system/shaders/configs")]).into_iter().map(|f| Choice { label: f.clone(), value: f }));
    let mut bezels = vec![Choice { label: "None".into(), value: "none".into() }];
    bezels.extend(folders(&[root.join("system/decorations"), root.join("decorations")]).into_iter().map(|f| Choice { label: f.clone(), value: f }));
    let integer = vec![Choice { label: "On".into(), value: "1".into() }, Choice { label: "Off".into(), value: "0".into() }];
    Ok(Offer { systems, shaders, bezels, ratios: read_ratios(&root.join(FEATURES)), integer })
}

/// Current settings and the choices this install offers, for every system.
pub fn options(connection: &Connection, installation_id: &str, presence: PresenceFn, running: RunningFn) -> Result<SettingsOptions, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read_text(&install.root.join(SETTINGS))?;
    let current = values(&entries(&text)?)?;
    let offer = offer(&install.root)?;
    let systems = offer
        .systems
        .into_iter()
        .map(|(system, name, emulators)| {
            let mine = KEYS.iter().filter_map(|k| current.get(&format!("{system}.{k}")).map(|v| (k.to_string(), v.clone()))).collect();
            SystemSettings { has_games: has_games(&install.root, &system), system, name, emulators, current: mine }
        })
        .collect();
    Ok(SettingsOptions {
        installation_path: install.root_text,
        systems,
        shaders: offer.shaders,
        bezels: offer.bezels,
        ratios: offer.ratios,
        integer_scaling: offer.integer,
        file_sha: hex_sha(text.as_bytes()),
        blocked: running_message(running),
    })
}

pub(crate) fn hex_sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Check every change against what the install offers; returns the entries to write.
fn validate(changes: &[Change], offer: &Offer, current: &BTreeMap<String, String>) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("There are no changes to apply.".into());
    }
    let wanted = validate_systems(changes, offer, current)?;
    if wanted.is_empty() {
        return Err("Those are already the current settings.".into());
    }
    Ok(wanted)
}

/// Per-system changes checked against the install's choices; only real changes are returned (may be empty).
pub(crate) fn validate_systems(changes: &[Change], offer: &Offer, current: &BTreeMap<String, String>) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut wanted: BTreeMap<String, Option<String>> = BTreeMap::new();
    for c in changes {
        if !KEYS.contains(&c.key.as_str()) {
            return Err(format!("RetroDrive doesn't change the setting {} yet.", c.key));
        }
        let Some((_, _, emulators)) = offer.systems.iter().find(|(s, _, _)| *s == c.system) else {
            return Err(format!("{} isn't a system in this RetroBat.", c.system));
        };
        if let Some(value) = &c.value {
            let ok = match c.key.as_str() {
                "emulator" => emulators.iter().any(|e| &e.emulator == value),
                "core" => emulators.iter().any(|e| e.cores.contains(value)),
                "shaderset" => offer.shaders.iter().any(|s| &s.value == value),
                "bezel" => offer.bezels.iter().any(|s| &s.value == value),
                "ratio" => offer.ratios.iter().any(|s| &s.value == value),
                "integerscale" => offer.integer.iter().any(|s| &s.value == value),
                _ => false,
            };
            if !ok || value.is_empty() || value.contains(['\r', '\n']) {
                return Err(format!("{value} isn't one of this RetroBat's choices for {} {}.", c.system, c.key));
            }
        }
        wanted.insert(change_key(c), c.value.clone());
    }
    // The emulator and core must belong together after the change (for a system, or
    // for one game of it).
    let mut scopes: Vec<(&str, Option<&str>)> = changes.iter().filter(|c| c.key == "emulator" || c.key == "core").map(|c| (c.system.as_str(), c.game.as_deref())).collect();
    scopes.dedup();
    for (system, game) in scopes {
        let Some((_, _, emulators)) = offer.systems.iter().find(|(s, _, _)| s == system) else { continue };
        let get = |key: &str| -> Option<String> {
            let k = change_key(&Change { system: system.to_string(), key: key.to_string(), value: None, game: game.map(str::to_string) });
            match wanted.get(&k) {
                Some(v) => v.clone(),
                None => current.get(&k).cloned(),
            }
        };
        let system = game.map(|g| format!("{system}: {g}")).unwrap_or_else(|| system.to_string());
        match (get("emulator"), get("core")) {
            (None, Some(core)) => return Err(format!("{system}: the core {core} needs an emulator chosen too.")),
            (Some(emulator), core) => {
                let option = emulators.iter().find(|e| e.emulator == emulator).ok_or(format!("{system}: {emulator} isn't available."))?;
                match core {
                    Some(core) if !option.cores.contains(&core) => return Err(format!("{system}: {emulator} has no core {core}.")),
                    None if !option.cores.is_empty() => return Err(format!("{system}: choose one of {emulator}'s cores.")),
                    _ => {}
                }
            }
            (None, None) => {}
        }
    }
    // Drop entries that wouldn't change anything.
    wanted.retain(|k, v| current.get(k) != v.as_ref());
    Ok(wanted)
}

pub(crate) fn rows(wanted: &BTreeMap<String, Option<String>>, current: &BTreeMap<String, String>) -> Vec<ChangeRow> {
    wanted
        .iter()
        .map(|(k, v)| {
            let (system, key) = match crate::batocera_settings::split_key(k) {
                Some((s, Some(game), key)) => (format!("{s}: {game}"), key),
                _ => k.rsplit_once('.').map(|(s, key)| (s.to_string(), key)).unwrap_or((String::new(), k)),
            };
            ChangeRow { system, key: key.to_string(), before: current.get(k).cloned(), after: v.clone() }
        })
        .collect()
}

/// The exact before → after values, without writing anything.
pub fn preview(connection: &Connection, installation_id: &str, changes: &[Change], presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read_text(&install.root.join(SETTINGS))?;
    let current = values(&entries(&text)?)?;
    check_games(&install.root, changes)?;
    let wanted = validate(changes, &offer(&install.root)?, &current)?;
    edit(&text, &wanted)?;
    Ok(Preview { changes: rows(&wanted, &current), file_sha: hex_sha(text.as_bytes()) })
}

fn set_state(connection: &Connection, id: i64, state: &str, message: Option<&str>) -> Result<(), String> {
    connection
        .execute("UPDATE settings_changes SET state = ?2, message = COALESCE(?3, message), updated_at = CURRENT_TIMESTAMP WHERE id = ?1", params![id, state, message])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Write the change: new file read back, current file moved to the backup folder, new file takes its name.
pub fn apply(connection: &Connection, installation_id: &str, changes: &[Change], expected_sha: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    if let Some(reason) = running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let settings = install.root.join(SETTINGS.replace('/', "\\"));
    let text = read_text(&settings)?;
    let old_sha = hex_sha(text.as_bytes());
    if old_sha != expected_sha {
        return Err("RetroBat's settings changed since you reviewed them, so nothing was changed. Review the changes again.".into());
    }
    let current = values(&entries(&text)?)?;
    check_games(&install.root, changes)?;
    let wanted = validate(changes, &offer(&install.root)?, &current)?;
    write_settings(connection, &install, &settings, &text, &wanted, &current)
}

/// The shared safe write: journal, new file read back, current file moved to the
/// backup folder, new file renamed in. `wanted` must already be validated.
fn write_settings(connection: &Connection, install: &Install, settings: &Path, text: &str, wanted: &BTreeMap<String, Option<String>>, current: &BTreeMap<String, String>) -> Result<String, String> {
    let new_text = edit(text, wanted)?;
    write_file(connection, install, SETTINGS, settings, text, &new_text, rows(wanted, current)).map(|(m, _)| m)
}

/// Journaled, backed-up replacement of one RetroBat file (`relative` to the
/// installation) with `new_text`. Undo and crash repair work from the journal.
pub(crate) fn write_file(connection: &Connection, install: &Install, relative: &str, settings: &Path, text: &str, new_text: &str, changed: Vec<ChangeRow>) -> Result<(String, i64), String> {
    // A file that doesn't exist yet (e.g. RetroBat's favorites playlist) is created;
    // its "previous version" is no file, and undo moves it into the backup folder.
    let creating = fs::symlink_metadata(settings).is_err();
    let old_sha = if creating { NO_FILE.to_string() } else { hex_sha(text.as_bytes()) };
    let new_sha = hex_sha(new_text.as_bytes());

    let token = db::new_id()[..8].to_string();
    let marker = format!("{TEMP_MARKER}{token}");
    let temp = PathBuf::from(format!("{}{marker}", settings.display()));
    let backup_relative = format!("{BACKUP_DIR}/settings-{token}/{relative}");
    if !creating {
        copier::ensure_folders(&install.root, &backup_relative).map_err(|e| format!("Couldn't prepare the backup folder: {e} Nothing was changed."))?;
    }
    let backup = paths::join(&install.root_text, &backup_relative);
    if fs::symlink_metadata(&backup).is_ok() || fs::symlink_metadata(&temp).is_ok() {
        return Err("RetroDrive's working file names are already taken, so nothing was changed.".into());
    }

    // Journal first.
    connection
        .execute(
            "INSERT INTO settings_changes (installation_id, file_path, root_path, stable_id, backup_path, temp_path, old_sha, new_sha, changes_json, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'applying')",
            params![
                install.id,
                settings.display().to_string(),
                install.root_text,
                install.stable,
                (!creating).then(|| backup.display().to_string()),
                temp.display().to_string(),
                old_sha,
                new_sha,
                serde_json::to_string(&changed).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    let id = connection.last_insert_rowid();

    let written = (|| -> std::io::Result<()> {
        let mut out = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        out.write_all(new_text.as_bytes())?;
        out.sync_all()
    })();
    let fail = |message: String| -> Result<(String, i64), String> {
        copier::remove_own_temp(&temp, &marker);
        set_state(connection, id, "failed", Some(&message))?;
        Err(message)
    };
    if let Err(e) = written {
        return fail(format!("Couldn't write the new settings: {e}. Nothing was changed."));
    }
    match copier::sha256_of(&temp) {
        Ok(sha) if sha == new_sha => {}
        _ => return fail("The new settings didn't read back correctly, so they weren't used. Nothing was changed.".into()),
    }
    if !creating {
        if let Err(e) = copier::rename_no_replace(settings, &backup) {
            return fail(format!("Couldn't move the current settings to the backup folder: {e}. Nothing was changed."));
        }
    }
    if let Err(e) = copier::rename_no_replace(&temp, settings) {
        let restored = creating || copier::rename_no_replace(&backup, settings).is_ok();
        return fail(if restored {
            format!("Couldn't put the new settings in place ({e}), so the current ones were put back.")
        } else {
            format!("Couldn't put the new settings in place ({e}). Your settings are safe in {}; restart RetroDrive with the drive connected to put them back.", backup.display())
        });
    }
    connection.execute("UPDATE settings_changes SET temp_path = NULL WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    let message = if creating {
        format!("{} created. Undo moves it into RetroDrive's backup folder.", relative.rsplit('/').next().unwrap_or(relative))
    } else {
        format!("{} setting(s) changed. The previous settings file is in {backup_relative}.", changed.len())
    };
    set_state(connection, id, "applied", Some(&message))?;
    Ok((message, id))
}

/// Theme folders RetroBat can use (`emulationstation\.emulationstation\themes` and `emulationstation\themes`).
pub fn theme_folders(root: &Path) -> Vec<String> {
    folders(&[root.join("emulationstation/.emulationstation/themes"), root.join("emulationstation/themes")])
}

/// Settings path, current text, current values and the wanted entry for a theme switch.
type ThemeChange = (PathBuf, String, BTreeMap<String, String>, BTreeMap<String, Option<String>>);

fn theme_change(install: &Install, theme: &str) -> Result<ThemeChange, String> {
    if !theme_folders(&install.root).iter().any(|t| t == theme) {
        return Err(format!("There's no theme called {theme} in this RetroBat."));
    }
    let settings = install.root.join(SETTINGS.replace('/', "\\"));
    let text = read_text(&settings)?;
    let current = values(&entries(&text)?)?;
    if current.get("ThemeSet").map(String::as_str) == Some(theme) {
        return Err(format!("{theme} is already RetroBat's theme."));
    }
    let wanted: BTreeMap<String, Option<String>> = [("ThemeSet".to_string(), Some(theme.to_string()))].into_iter().collect();
    Ok((settings, text, current, wanted))
}

/// What switching RetroBat's theme would change (reads only).
pub fn preview_theme(connection: &Connection, installation_id: &str, theme: &str, presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let (_, text, current, wanted) = theme_change(&install, theme)?;
    edit(&text, &wanted)?;
    Ok(Preview { changes: rows(&wanted, &current), file_sha: hex_sha(text.as_bytes()) })
}

/// Make `theme` RetroBat's theme (`ThemeSet`), the same safe way as other settings; undoable.
pub fn apply_theme(connection: &Connection, installation_id: &str, theme: &str, expected_sha: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    if let Some(reason) = running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let (settings, text, current, wanted) = theme_change(&install, theme)?;
    if hex_sha(text.as_bytes()) != expected_sha {
        return Err("RetroBat's settings changed since you reviewed them, so nothing was changed. Review the change again.".into());
    }
    write_settings(connection, &install, &settings, &text, &wanted, &current)
}

/// RetroBat's current theme (`ThemeSet`), if set.
pub fn current_theme(root: &Path) -> Option<String> {
    let text = read_text(&root.join(SETTINGS)).ok()?;
    values(&entries(&text).ok()?).ok()?.get("ThemeSet").cloned()
}

// ---------- retrobat.ini: boot video and frontend options ----------

pub(crate) const INI: &str = "retrobat.ini";
pub(crate) const VIDEOS: &str = "emulationstation/.emulationstation/video";
pub(crate) const MUSIC: &str = "emulationstation/.emulationstation/music";
const MAX_VIDEO: u64 = 2 << 30;

/// One `retrobat.ini` option RetroDrive offers (checked against the comments in
/// RetroBat 8.3.1.1's own retrobat.ini, which document each key).
#[derive(Debug, Serialize, Clone)]
pub struct IniOption {
    pub section: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// `bool` (0/1), `choice` or `number`.
    pub kind: &'static str,
    pub choices: Vec<Choice>,
}

fn opt(section: &'static str, key: &'static str, label: &'static str, help: &'static str, kind: &'static str, choices: &[(&str, &str)]) -> IniOption {
    IniOption { section, key, label, help, kind, choices: choices.iter().map(|(v, l)| Choice { label: l.to_string(), value: v.to_string() }).collect() }
}

/// The options offered. Left out on purpose: Autostart (changes Windows startup),
/// ResetConfigMode (resets RetroBat's settings), AppLauncher, window sizes.
pub(crate) fn ini_options() -> Vec<IniOption> {
    vec![
        opt("SplashScreen", "EnableIntro", "Play a boot video", "Play a video before RetroBat's menus appear.", "bool", &[]),
        opt("SplashScreen", "RandomVideo", "Random video", "Pick a random video each time (otherwise the chosen video plays).", "bool", &[]),
        opt("SplashScreen", "FileName", "Boot video", "The video that plays when Random video is off.", "choice", &[]),
        opt("SplashScreen", "WaitForVideoEnd", "Wait for the video to end", "Start the menus only after the video finishes.", "bool", &[]),
        opt("SplashScreen", "GamepadVideoKill", "Skip with a gamepad", "A gamepad button press skips the video (XInput controllers).", "bool", &[]),
        opt("SplashScreen", "KillVideoWhenESReady", "Stop the video when the menus are ready", "", "bool", &[]),
        opt("SplashScreen", "VideoDelay", "Video delay (ms)", "Delay between the video and the menus starting; longer can help if the video isn't in front.", "number", &[]),
        opt("EmulationStation", "InterfaceMode", "Interface mode", "Kiosk and kid modes hide settings and limit what players can change.", "choice", &[("0", "Normal"), ("1", "Kiosk"), ("2", "Kid")]),
        opt("EmulationStation", "Fullscreen", "Fullscreen", "Start the menus in fullscreen (otherwise in a window).", "bool", &[]),
        opt("EmulationStation", "FullscreenBorderless", "Borderless fullscreen", "", "bool", &[]),
        opt("EmulationStation", "RandomTheme", "Random theme", "Pick a random installed theme at every start.", "bool", &[]),
        opt("EmulationStation", "NoExitMenu", "Hide the Quit option", "Hide the option to quit RetroBat from the menu.", "bool", &[]),
        opt("EmulationStation", "VSync", "VSync in menus", "", "bool", &[]),
        opt("EmulationStation", "DrawFramerate", "Show frame rate in menus", "", "bool", &[]),
        opt("RetroBat", "LanguageDetection", "Use Windows' language", "Set RetroBat's language from Windows at startup.", "bool", &[]),
        opt("RetroBat", "DisableWindowsInterface", "Hide Windows while playing", "Hide notifications (and optionally the taskbar) while RetroBat runs; restored when it exits.", "choice", &[("0", "No"), ("1", "Notifications"), ("2", "Notifications and desktop")]),
    ]
}

/// `[Section]` + `Key=value` lines: value with quotes removed, and the byte range of the raw value.
pub(crate) fn ini_entries(text: &str) -> BTreeMap<String, (String, usize, usize)> {
    let mut found = BTreeMap::new();
    let mut section = String::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_string();
        } else if !trimmed.starts_with(';') && !trimmed.starts_with('#') {
            if let Some(eq) = body.find('=') {
                let key = body[..eq].trim();
                if !key.is_empty() {
                    let raw_start = offset + eq + 1;
                    let raw_end = offset + body.len();
                    let value = body[eq + 1..].trim().trim_matches('"').to_string();
                    found.insert(format!("{section}.{key}"), (value, raw_start, raw_end));
                }
            }
        }
        offset += line.len();
    }
    found
}

/// Replace values of existing keys only (RetroBat writes every key it uses), checking the result.
pub(crate) fn ini_edit(text: &str, wanted: &BTreeMap<String, String>) -> Result<String, String> {
    let before = ini_entries(text);
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    for (key, value) in wanted {
        let (old, start, end) = before.get(key).ok_or(format!("retrobat.ini has no {key}, so RetroDrive won't add it."))?;
        let raw = &text[*start..*end];
        let quoted = raw.trim().starts_with('"') || key.ends_with(".FileName") || key.ends_with(".FilePath");
        let _ = old;
        spans.push((*start, *end, if quoted { format!("\"{value}\"") } else { value.clone() }));
    }
    spans.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut out = text.to_string();
    for (start, end, value) in spans {
        out.replace_range(start..end, &value);
    }
    let after = ini_entries(&out);
    for (key, (value, _, _)) in &before {
        let expected = wanted.get(key).unwrap_or(value);
        if after.get(key).map(|a| &a.0) != Some(expected) {
            return Err(format!("Editing retrobat.ini would have changed {key} unexpectedly; nothing was changed."));
        }
    }
    if after.len() != before.len() {
        return Err("Editing retrobat.ini would have added or lost settings; nothing was changed.".into());
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct LookOptions {
    pub options: Vec<IniOption>,
    /// `Section.Key` -> current value.
    pub values: BTreeMap<String, String>,
    /// Videos in RetroBat's video folder (for the boot video).
    pub videos: Vec<String>,
    pub videos_path: String,
    /// False when `FilePath` points somewhere else than the default video folder.
    pub default_video_folder: bool,
    /// Music files in RetroBat's music folder (played in the menus).
    pub music: Vec<String>,
    /// Every track incl. system subfolders (`snes/Song.ogg`).
    pub tracks: Vec<String>,
    /// Tracks in the favorites playlist (`None`: no playlist).
    pub favorites: Option<Vec<String>>,
    pub music_path: String,
    pub file_sha: String,
    pub blocked: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IniChange {
    pub section: String,
    pub key: String,
    pub value: String,
}

pub(crate) fn videos(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(VIDEOS))
        .map(|read| read.filter_map(Result::ok).filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false)).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.to_lowercase().ends_with(".mp4")).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Boot video and frontend options from retrobat.ini (reads only).
pub fn look_options(connection: &Connection, installation_id: &str, presence: PresenceFn, running: RunningFn) -> Result<LookOptions, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read_text(&install.root.join(INI))?;
    let entries = ini_entries(&text);
    let options: Vec<IniOption> = ini_options().into_iter().filter(|o| entries.contains_key(&format!("{}.{}", o.section, o.key))).collect();
    let values = options.iter().filter_map(|o| entries.get(&format!("{}.{}", o.section, o.key)).map(|v| (format!("{}.{}", o.section, o.key), v.0.clone()))).collect();
    let default_video_folder = entries.get("SplashScreen.FilePath").map(|v| v.0.eq_ignore_ascii_case("default")).unwrap_or(true);
    Ok(LookOptions {
        options,
        values,
        videos: videos(&install.root),
        videos_path: install.root.join(VIDEOS.replace('/', "\\")).display().to_string(),
        default_video_folder,
        music: music_files(&install.root),
        tracks: music_tracks(&install.root),
        favorites: favorite_tracks(&install.root, &install.root_text),
        music_path: install.root.join(MUSIC.replace('/', "\\")).display().to_string(),
        file_sha: hex_sha(text.as_bytes()),
        blocked: running_message(running),
    })
}

fn ini_wanted(install: &Install, text: &str, changes: &[IniChange]) -> Result<(BTreeMap<String, String>, Vec<ChangeRow>), String> {
    let (wanted, rows) = ini_validate(install, text, changes, &[])?;
    if wanted.is_empty() {
        return Err("Those are already the current settings.".into());
    }
    Ok((wanted, rows))
}

/// retrobat.ini changes checked against the offered options; `extra_videos` are
/// videos that will be added before writing. Only real changes are returned.
pub(crate) fn ini_validate(install: &Install, text: &str, changes: &[IniChange], extra_videos: &[String]) -> Result<(BTreeMap<String, String>, Vec<ChangeRow>), String> {
    let entries = ini_entries(text);
    let options = ini_options();
    let mut videos = videos(&install.root);
    videos.extend(extra_videos.iter().cloned());
    let mut wanted = BTreeMap::new();
    let mut rows = Vec::new();
    for c in changes {
        let o = options.iter().find(|o| o.section == c.section && o.key == c.key).ok_or(format!("RetroDrive doesn't change {}.{}.", c.section, c.key))?;
        let ok = match o.kind {
            "bool" => c.value == "0" || c.value == "1",
            "number" => c.value.len() <= 6 && c.value.parse::<u32>().is_ok_and(|n| n <= 60_000),
            _ if o.key == "FileName" => videos.contains(&c.value),
            _ => o.choices.iter().any(|ch| ch.value == c.value),
        };
        if !ok {
            return Err(format!("{} isn't a valid choice for {}.", c.value, o.label));
        }
        let key = format!("{}.{}", c.section, c.key);
        let before = entries.get(&key).map(|e| e.0.clone());
        if before.as_deref() != Some(c.value.as_str()) {
            rows.push(ChangeRow { system: c.section.clone(), key: c.key.clone(), before, after: Some(c.value.clone()) });
            wanted.insert(key, c.value.clone());
        }
    }
    Ok((wanted, rows))
}

/// What a retrobat.ini change would do (reads only).
pub fn preview_look(connection: &Connection, installation_id: &str, changes: &[IniChange], presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read_text(&install.root.join(INI))?;
    let (wanted, rows) = ini_wanted(&install, &text, changes)?;
    ini_edit(&text, &wanted)?;
    Ok(Preview { changes: rows, file_sha: hex_sha(text.as_bytes()) })
}

/// Write a reviewed retrobat.ini change (same safety as es_settings.cfg; undoable).
pub fn apply_look(connection: &Connection, installation_id: &str, changes: &[IniChange], expected_sha: &str, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    if let Some(reason) = running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let path = install.root.join(INI);
    let text = read_text(&path)?;
    if hex_sha(text.as_bytes()) != expected_sha {
        return Err("retrobat.ini changed since you reviewed it, so nothing was changed. Review the change again.".into());
    }
    let (wanted, rows) = ini_wanted(&install, &text, changes)?;
    let new_text = ini_edit(&text, &wanted)?;
    write_file(connection, &install, INI, &path, &text, &new_text, rows).map(|(m, _)| m)
}

/// Copy the user's own video into RetroBat's video folder as a new file (never over one).
pub fn add_boot_video(connection: &Connection, installation_id: &str, source: &str, presence: PresenceFn) -> Result<String, String> {
    add_file(connection, installation_id, source, presence, VIDEOS, &["mp4"], "Boot videos must be .mp4 files.")
}

/// Copy the user's own music (.ogg or .mp3, as RetroBat's menus play) into its music folder as a new file.
pub fn add_music(connection: &Connection, installation_id: &str, source: &str, presence: PresenceFn) -> Result<String, String> {
    add_file(connection, installation_id, source, presence, MUSIC, &["ogg", "mp3"], "Music must be .ogg or .mp3 files.")
}

/// RetroBat's favorites playlist (RetroBat 8.3.1.1 writes it here:
/// `music\favorites.m3u`, one `<full path with />;<name>` per line, CRLF).
pub(crate) const FAVORITES: &str = "emulationstation/.emulationstation/music/favorites.m3u";

/// Every music track, relative to the music folder (`Song.ogg`, `snes/Song.ogg`): the folder and its system subfolders.
pub fn music_tracks(root: &Path) -> Vec<String> {
    let music = root.join(MUSIC);
    let is_music = |n: &str| { let l = n.to_lowercase(); l.ends_with(".ogg") || l.ends_with(".mp3") };
    let mut found = Vec::new();
    if let Ok(read) = fs::read_dir(&music) {
        for e in read.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            match e.file_type() {
                Ok(t) if t.is_file() && is_music(&name) => found.push(name),
                Ok(t) if t.is_dir() => {
                    if let Ok(sub) = fs::read_dir(e.path()) {
                        for f in sub.filter_map(Result::ok).filter(|f| f.file_type().map(|t| t.is_file()).unwrap_or(false)) {
                            let n = f.file_name().to_string_lossy().into_owned();
                            if is_music(&n) {
                                found.push(format!("{name}/{n}"));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found.sort_by_key(|t| t.to_lowercase());
    found
}

/// The music folder as RetroBat writes it in favorites.m3u (`G:/RetroBat/.../music`).
pub(crate) fn music_prefix(root_text: &str) -> String {
    format!("{}/{MUSIC}", root_text.trim_end_matches(['\\', '/']).replace('\\', "/"))
}

/// Tracks in RetroBat's favorites playlist, relative to the music folder (`None`: no playlist).
pub fn favorite_tracks(root: &Path, root_text: &str) -> Option<Vec<String>> {
    let text = fs::read_to_string(root.join(FAVORITES)).ok()?;
    let prefix = format!("{}/", music_prefix(root_text)).to_lowercase();
    Some(
        text.lines()
            .filter_map(|l| l.split_once(';').map(|(p, _)| p.replace('\\', "/")))
            .filter_map(|p| p.to_lowercase().starts_with(&prefix).then(|| p[prefix.len()..].to_string()))
            .collect(),
    )
}

/// favorites.m3u text for the given tracks (relative to the music folder).
pub(crate) fn favorites_text(root_text: &str, tracks: &[String]) -> String {
    let prefix = music_prefix(root_text);
    tracks
        .iter()
        .map(|t| {
            let name = t.rsplit('/').next().unwrap_or(t);
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
            format!("{prefix}/{t};{stem}\r\n")
        })
        .collect()
}

/// Music files in RetroBat's music folder.
pub fn music_files(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(MUSIC))
        .map(|read| read.filter_map(Result::ok).filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false)).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| { let l = n.to_lowercase(); l.ends_with(".ogg") || l.ends_with(".mp3") }).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Copy a user file into an installation folder as a new file: temporary name,
/// checksum compared with the source, no-replace rename. Never replaces a file.
pub(crate) fn add_file(connection: &Connection, installation_id: &str, source: &str, presence: PresenceFn, folder: &str, exts: &[&str], wrong_type: &str) -> Result<String, String> {
    let install = install(connection, installation_id, presence)?;
    let source = PathBuf::from(source);
    if !source.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x))) {
        return Err(wrong_type.into());
    }
    let size = fs::metadata(&source).map_err(|e| format!("Couldn't open {}: {e}", source.display()))?.len();
    if size > MAX_VIDEO {
        return Err("That file is larger than 2 GB.".into());
    }
    let name: String = source.file_name().and_then(|n| n.to_str()).unwrap_or("video.mp4").chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let relative = format!("{folder}/{name}");
    copier::ensure_folders(&install.root, &relative)?;
    let target = paths::join(&install.root.display().to_string(), &relative);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(format!("There's already a file called {name} there; rename yours first."));
    }
    let token = db::new_id()[..8].to_string();
    let marker = format!("{TEMP_MARKER}{token}");
    let temp = PathBuf::from(format!("{}{marker}", target.display()));
    let copied = (|| -> std::io::Result<()> {
        let mut input = fs::File::open(&source)?;
        let mut out = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        std::io::copy(&mut input, &mut out)?;
        out.sync_all()
    })();
    let fail = |message: String| -> Result<String, String> {
        copier::remove_own_temp(&temp, &marker);
        Err(message)
    };
    if let Err(e) = copied {
        return fail(format!("Couldn't copy the file: {e}"));
    }
    match (copier::sha256_of(&temp), copier::sha256_of(&source)) {
        (Ok(a), Ok(b)) if a == b => {}
        _ => return fail("The copy didn't match the original, so it wasn't used.".into()),
    }
    if let Err(e) = copier::rename_no_replace(&temp, &target) {
        return fail(format!("Couldn't give the file its name: {e}"));
    }
    Ok(name)
}

struct Journal {
    id: i64,
    file: String,
    root: String,
    stable: Option<String>,
    backup: Option<String>,
    temp: Option<String>,
    undo: Option<String>,
    old_sha: String,
    new_sha: String,
    changes: String,
    state: String,
    message: Option<String>,
    when: String,
}

fn journals(connection: &Connection, where_clause: &str, value: &dyn rusqlite::ToSql) -> Result<Vec<Journal>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT id, file_path, root_path, stable_id, backup_path, temp_path, undo_path, old_sha, new_sha, changes_json, state, message, created_at
             FROM settings_changes WHERE {where_clause} ORDER BY id DESC"
        ))
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([value], |r| {
            Ok(Journal {
                id: r.get(0)?,
                file: r.get(1)?,
                root: r.get(2)?,
                stable: r.get(3)?,
                backup: r.get(4)?,
                temp: r.get(5)?,
                undo: r.get(6)?,
                old_sha: r.get(7)?,
                new_sha: r.get(8)?,
                changes: r.get(9)?,
                state: r.get(10)?,
                message: r.get(11)?,
                when: r.get(12)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn on_drive(j: &Journal, presence: PresenceFn) -> bool {
    paths::split_volume(Path::new(&j.root)).is_some_and(|(volume, _)| presence(&volume, j.stable.as_deref()) == Presence::Connected)
}

fn ours(j: &Journal, path: &str) -> bool {
    let backup_root = format!("{}\\{}", j.root.trim_end_matches(['\\', '/']), BACKUP_DIR.replace('/', "\\"));
    copier::is_inside(&path.replace('/', "\\"), &backup_root)
}

fn file_sha(path: &Path) -> Option<String> {
    fs::symlink_metadata(path).ok().filter(|m| m.is_file() && !m.file_type().is_symlink())?;
    copier::sha256_of(path).ok()
}

fn undo_block(j: &Journal, presence: PresenceFn) -> Option<String> {
    if j.state != "applied" {
        return Some(match j.state.as_str() {
            "undone" => "Already undone.".into(),
            "failed" => "This change wasn't applied.".into(),
            _ => "Interrupted; restart RetroDrive with the drive connected to finish it.".into(),
        });
    }
    if !on_drive(j, presence) {
        return Some("The drive isn't connected.".into());
    }
    if j.old_sha != NO_FILE {
        let Some(backup) = j.backup.as_deref().filter(|b| ours(j, b)) else { return Some("No backup is recorded.".into()) };
        if fs::symlink_metadata(backup).map(|m| !m.is_file()).unwrap_or(true) {
            return Some("The previous settings file isn't in the backup folder any more.".into());
        }
    }
    match file_sha(Path::new(&j.file)) {
        Some(sha) if sha == j.new_sha => None,
        Some(_) => Some("The settings file changed since then (in its own menus or by another change), so undoing would lose those changes.".into()),
        None => Some("The settings file is missing.".into()),
    }
}

/// Settings changes made on one installation, newest first.
pub fn history(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Vec<HistoryRow>, String> {
    Ok(journals(connection, "installation_id = ?1", &installation_id)?
        .into_iter()
        .map(|j| HistoryRow {
            id: j.id,
            when: j.when.clone(),
            state: j.state.clone(),
            changes: serde_json::from_str(&j.changes).unwrap_or_default(),
            message: j.message.clone(),
            blocked: undo_block(&j, presence),
        })
        .collect())
}

/// Put the previous settings back; the changed file moves into the backup folder.
pub fn undo(connection: &Connection, change_id: i64, presence: PresenceFn, running: RunningFn) -> Result<String, String> {
    let j = journals(connection, "id = ?1", &change_id)?.into_iter().next().ok_or("That change wasn't found.")?;
    if let Some(reason) = running_message(running) {
        return Err(format!("{reason} Nothing was changed."));
    }
    if let Some(reason) = undo_block(&j, presence) {
        return Err(format!("{reason} Nothing was changed."));
    }
    let backup = PathBuf::from(j.backup.as_deref().unwrap_or_default());
    let settings = PathBuf::from(&j.file);
    let file_relative = j.file.get(j.root.len()..).filter(|_| copier::is_inside(&j.file, &j.root)).unwrap_or(SETTINGS).trim_start_matches(['\\', '/']).replace('\\', "/");
    let undo_relative = format!("{BACKUP_DIR}/settings-undo-{}/{file_relative}", j.id);
    copier::ensure_folders(Path::new(&j.root), &undo_relative).map_err(|e| format!("Couldn't prepare the backup folder: {e} Nothing was changed."))?;
    let undo_path = paths::join(&j.root, &undo_relative);
    if fs::symlink_metadata(&undo_path).is_ok() {
        return Err(format!("{} already exists, so nothing was changed.", undo_path.display()));
    }
    connection
        .execute("UPDATE settings_changes SET state = 'undoing', undo_path = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?1", params![j.id, undo_path.display().to_string()])
        .map_err(|e| e.to_string())?;
    if let Err(e) = copier::rename_no_replace(&settings, &undo_path) {
        set_state(connection, j.id, "applied", None)?;
        return Err(format!("Couldn't move the current settings aside, so nothing was changed: {e}"));
    }
    if j.old_sha == NO_FILE {
        // The change created the file: moving it aside is the whole undo.
        let message = format!("Undone: the file RetroDrive created is now in {undo_relative}.");
        set_state(connection, j.id, "undone", Some(&message))?;
        return Ok(message);
    }
    if let Err(e) = copier::rename_no_replace(&backup, &settings) {
        if copier::rename_no_replace(&undo_path, &settings).is_ok() {
            set_state(connection, j.id, "applied", None)?;
            return Err(format!("Couldn't put the previous settings back, so everything was left as it was: {e}"));
        }
        return Err(format!("Couldn't put the previous settings back ({e}). Both files are safe: {} and {}. Restart RetroDrive with the drive connected to finish.", backup.display(), undo_path.display()));
    }
    let message = format!("Undone: the previous settings are back. The changed file is in {undo_relative}.");
    set_state(connection, j.id, "undone", Some(&message))?;
    Ok(message)
}

/// At startup: finish or roll back settings changes that were interrupted, from
/// what's on the drive. Only on the confirmed drive and only RetroDrive's own files.
pub fn recover(connection: &Connection, presence: PresenceFn) -> Result<usize, String> {
    let mut fixed = 0;
    for j in journals(connection, "state IN ('applying', 'undoing') AND ?1", &1)? {
        if !on_drive(&j, presence) || !copier::is_inside(&j.file, &j.root) {
            continue;
        }
        let settings = Path::new(&j.file);
        let backup = j.backup.as_deref().filter(|b| ours(&j, b)).map(PathBuf::from);
        let current = file_sha(settings);
        if j.state == "applying" {
            if current.as_deref() == Some(j.new_sha.as_str()) {
                set_state(connection, j.id, "applied", Some("Applied (finished before an interruption)."))?;
                fixed += 1;
            } else if current.is_some() {
                if let Some(temp) = j.temp.as_deref() {
                    let marker = format!("{TEMP_MARKER}{}", Path::new(temp).file_name().and_then(|n| n.to_str()).and_then(|n| n.rsplit(TEMP_MARKER).next()).unwrap_or(""));
                    copier::remove_own_temp(Path::new(temp), &marker);
                }
                set_state(connection, j.id, "failed", Some("Interrupted before the change; nothing was changed."))?;
                fixed += 1;
            } else if j.old_sha == NO_FILE {
                if let Some(temp) = j.temp.as_deref() {
                    let marker = format!("{TEMP_MARKER}{}", Path::new(temp).file_name().and_then(|n| n.to_str()).and_then(|n| n.rsplit(TEMP_MARKER).next()).unwrap_or(""));
                    copier::remove_own_temp(Path::new(temp), &marker);
                }
                set_state(connection, j.id, "failed", Some("Interrupted before the file was created; nothing was changed."))?;
                fixed += 1;
            } else if let Some(backup) = backup.filter(|b| b.is_file()) {
                if copier::rename_no_replace(&backup, settings).is_ok() {
                    set_state(connection, j.id, "failed", Some("Interrupted during the change; the previous settings were put back."))?;
                    fixed += 1;
                }
            }
        } else {
            let undo = j.undo.as_deref().filter(|u| ours(&j, u)).map(PathBuf::from);
            if j.old_sha == NO_FILE {
                // Undoing a created file is a single move.
                match (current.as_deref(), undo.as_ref().is_some_and(|u| u.is_file())) {
                    (None, true) => set_state(connection, j.id, "undone", Some("Undone (finished before an interruption)."))?,
                    (Some(sha), false) if sha == j.new_sha => set_state(connection, j.id, "applied", None)?,
                    _ => continue,
                }
                fixed += 1;
                continue;
            }
            match (current.as_deref(), undo.as_ref().is_some_and(|u| u.is_file())) {
                (Some(sha), _) if sha == j.old_sha => {
                    set_state(connection, j.id, "undone", Some("Undone (finished before an interruption)."))?;
                    fixed += 1;
                }
                (Some(sha), false) if sha == j.new_sha => {
                    set_state(connection, j.id, "applied", None)?;
                    fixed += 1;
                }
                (None, true) if undo.as_ref().is_some_and(|u| copier::rename_no_replace(u, settings).is_ok()) => {
                    set_state(connection, j.id, "applied", None)?;
                    fixed += 1;
                }
                _ => {}
            }
        }
    }
    Ok(fixed)
}

/// For installations whose frontend never runs on this PC (Batocera runs on its own machine).
pub fn never_running() -> bool {
    false
}

/// Whether RetroBat's interface or launcher is running on this computer.
pub fn retrobat_running() -> bool {
    any_running(&["retrobat.exe", "emulationstation.exe", "emulatorlauncher.exe"])
}

/// Whether ES-DE is running on this computer (it saves its game lists when it closes).
pub fn esde_running() -> bool {
    any_running(&["es-de.exe"])
}

/// Whether a process with one of these (lower-case) names is running.
#[cfg(windows)]
fn any_running(names: &[&str]) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    // SAFETY: standard ToolHelp snapshot walk; the entry's size is set before use and the handle is closed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
            if names.contains(&name.as_str()) {
                found = true;
                break;
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn any_running(_names: &[&str]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn unplugged(_: &str, _: Option<&str>) -> Presence {
        Presence::NotConnected
    }
    fn closed() -> bool {
        false
    }
    fn open() -> bool {
        true
    }

    const SETTINGS_FILE: &str = "<?xml version=\"1.0\"?>\r\n<config>\r\n\t<bool name=\"FavoritesFirst\" value=\"true\" />\r\n\t<string name=\"ScreenSaverGameInfo\" value=\"start &amp; end\" />\r\n\t<string name=\"snes.shaderset\" value=\"scanlines\" />\r\n\t<string name=\"xbox.bezel\" value=\"none\" />\r\n</config>\r\n";

    const SYSTEMS_FILE: &str = r#"<?xml version="1.0"?>
<systemList>
  <system>
    <name>snes</name>
    <fullname>Super Nintendo Entertainment System</fullname>
    <emulators>
      <emulator name="libretro"><cores><core>snes9x</core><core>bsnes</core></cores></emulator>
      <emulator name="snes9x"/>
    </emulators>
  </system>
  <system>
    <name>nes</name>
    <fullname>Nintendo Entertainment System</fullname>
    <emulators><emulator name="libretro"><cores><core>fceumm</core><core>mesen</core></cores></emulator></emulators>
  </system>
</systemList>"#;

    const FEATURES_FILE: &str = r#"<?xml version="1.0"?>
<features><sharedFeatures>
  <feature name="GAME ASPECT RATIO" value="ratio"><choice name="4/3" value="4/3"/><choice name="16/9" value="16/9"/></feature>
</sharedFeatures></features>"#;

    struct Setup {
        _dir: tempfile::TempDir,
        root: PathBuf,
        catalog: Connection,
        install: String,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        let es = root.join("emulationstation/.emulationstation");
        for d in [es.clone(), root.join("roms/snes"), root.join("bios"), root.join("system/shaders/configs/crt-royale"), root.join("system/shaders/configs/scanlines"), root.join("system/decorations/arcade_1980s"), root.join("decorations/my_bezels")] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("roms/snes/Mario.sfc"), b"m").unwrap();
        fs::write(es.join("es_settings.cfg"), SETTINGS_FILE).unwrap();
        fs::write(es.join("es_systems.cfg"), SYSTEMS_FILE).unwrap();
        fs::write(es.join("es_features.cfg"), FEATURES_FILE).unwrap();
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        let install = installations::add(&catalog, &root, "retrobat", &|_: &str| VolumeProbe::unknown()).unwrap();
        let root = paths::normalize(&root).unwrap();
        Setup { _dir: dir, root, catalog, install }
    }

    fn change(system: &str, key: &str, value: Option<&str>) -> Change {
        Change { system: system.into(), key: key.into(), value: value.map(str::to_string), game: None }
    }

    fn settings(s: &Setup) -> String {
        fs::read_to_string(s.root.join(SETTINGS)).unwrap()
    }

    /// Reads (never writes) a real RetroBat's settings files:
    /// `RETRODRIVE_REAL_RETROBAT=G:\RetroBat cargo test real_retrobat_files -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_retrobat_files_parse_and_edit_cleanly() {
        let Some(root) = std::env::var_os("RETRODRIVE_REAL_RETROBAT").map(PathBuf::from) else { return };
        let systems = read_systems(&root.join(SYSTEMS)).unwrap();
        let ratios = read_ratios(&root.join(FEATURES));
        let text = read_text(&root.join(SETTINGS)).unwrap();
        let list = entries(&text).unwrap();
        well_formed(&text).unwrap();
        let snes = systems.iter().find(|(s, _, _)| s == "snes").unwrap();
        eprintln!("systems: {}, snes emulators: {:?}", systems.len(), snes.2.iter().map(|e| format!("{} {:?}", e.emulator, e.cores)).collect::<Vec<_>>());
        eprintln!("ratios: {}, settings entries: {}", ratios.len(), list.len());
        // An edit in memory only: everything else must stay byte for byte.
        let wanted: BTreeMap<String, Option<String>> = [("snes.emulator".to_string(), Some("libretro".to_string())), ("snes.core".to_string(), Some("bsnes".to_string()))].into_iter().collect();
        let out = edit(&text, &wanted).unwrap();
        let close = text.rfind("</config>").unwrap();
        assert_eq!(&out[..close], &text[..close], "existing lines untouched");
        assert!(!ratios.is_empty() && systems.len() > 100);
        // retrobat.ini: every offered option is found, and an in-memory edit changes only its value.
        let ini = read_text(&root.join(INI)).unwrap();
        let found = ini_entries(&ini);
        let offered: Vec<String> = ini_options().iter().map(|o| format!("{}.{}", o.section, o.key)).filter(|k| found.contains_key(k)).collect();
        eprintln!("retrobat.ini: {} keys, offered {}/{}: {:?}", found.len(), offered.len(), ini_options().len(), offered);
        let wanted: BTreeMap<String, String> = [("SplashScreen.RandomVideo".to_string(), "0".to_string())].into_iter().collect();
        let edited = ini_edit(&ini, &wanted).unwrap();
        assert_eq!(edited.len(), ini.len());
        eprintln!("videos: {:?}, music: {}", videos(&root), music_files(&root).len());
    }

    #[test]
    fn options_come_from_the_install() {
        let s = setup();
        let o = options(&s.catalog, &s.install, &connected, &closed).unwrap();
        let snes = o.systems.iter().find(|x| x.system == "snes").unwrap();
        assert!(snes.has_games);
        assert_eq!(snes.emulators[0], EmulatorOption { emulator: "libretro".into(), cores: vec!["snes9x".into(), "bsnes".into()] });
        assert_eq!(snes.emulators[1].emulator, "snes9x");
        assert_eq!(snes.current.get("shaderset").map(String::as_str), Some("scanlines"));
        assert!(!o.systems.iter().find(|x| x.system == "nes").unwrap().has_games);
        assert_eq!(o.shaders.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(), ["none", "crt-royale", "scanlines"]);
        assert_eq!(o.bezels.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(), ["none", "arcade_1980s", "my_bezels"]);
        assert_eq!(o.ratios.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(), ["4/3", "16/9"]);
        assert!(o.blocked.is_none());
        assert!(options(&s.catalog, &s.install, &connected, &open).unwrap().blocked.unwrap().contains("running"));
    }

    #[test]
    fn edits_touch_only_the_changed_lines() {
        let wanted: BTreeMap<String, Option<String>> = [
            ("snes.emulator".to_string(), Some("libretro".to_string())),
            ("snes.core".to_string(), Some("bsnes".to_string())),
            ("snes.shaderset".to_string(), None),
            ("xbox.bezel".to_string(), Some("a & b".to_string())),
        ]
        .into_iter()
        .collect();
        let out = edit(SETTINGS_FILE, &wanted).unwrap();
        assert_eq!(
            out,
            "<?xml version=\"1.0\"?>\r\n<config>\r\n\t<bool name=\"FavoritesFirst\" value=\"true\" />\r\n\t<string name=\"ScreenSaverGameInfo\" value=\"start &amp; end\" />\r\n\t<string name=\"xbox.bezel\" value=\"a &amp; b\" />\r\n\t<string name=\"snes.core\" value=\"bsnes\" />\r\n\t<string name=\"snes.emulator\" value=\"libretro\" />\r\n</config>\r\n"
        );
        assert!(edit("<config><string name=\"a\" value=\"1\" /><string name=\"a\" value=\"2\" /></config>", &wanted).unwrap_err().contains("twice"));
        assert!(edit("<config><string name=\"a\" value=\"1\"></string></config>", &wanted).is_err(), "unusual layouts aren't edited");
    }

    #[test]
    fn only_offered_choices_are_accepted() {
        let s = setup();
        let try_one = |c: Change| preview(&s.catalog, &s.install, &[c], &connected);
        assert!(try_one(change("snes", "emulator", Some("dosbox"))).is_err());
        assert!(try_one(change("snes", "core", Some("fceumm"))).is_err(), "a core of another system");
        assert!(try_one(change("snes", "shaderset", Some("../../evil"))).is_err());
        assert!(try_one(change("snes", "rewind", Some("1"))).is_err(), "keys outside version 1");
        assert!(try_one(change("psx", "ratio", Some("4/3"))).is_err(), "systems this RetroBat doesn't have");
        assert!(try_one(change("snes", "shaderset", Some("scanlines"))).unwrap_err().contains("already"));
        // Emulator and core must go together.
        assert!(preview(&s.catalog, &s.install, &[change("snes", "emulator", Some("libretro"))], &connected).unwrap_err().contains("cores"));
        let ok = preview(&s.catalog, &s.install, &[change("snes", "emulator", Some("libretro")), change("snes", "core", Some("bsnes"))], &connected).unwrap();
        assert_eq!(ok.changes.len(), 2);
        assert_eq!(ok.changes[0], ChangeRow { system: "snes".into(), key: "core".into(), before: None, after: Some("bsnes".into()) });
        assert!(preview(&s.catalog, &s.install, &[change("snes", "emulator", Some("snes9x"))], &connected).is_ok(), "a standalone emulator needs no core");
    }

    #[test]
    fn apply_backs_up_then_changes_and_undo_puts_it_back() {
        let s = setup();
        let changes = [change("snes", "emulator", Some("libretro")), change("snes", "core", Some("bsnes")), change("snes", "ratio", Some("4/3"))];
        let sha = preview(&s.catalog, &s.install, &changes, &connected).unwrap().file_sha;
        assert!(apply(&s.catalog, &s.install, &changes, &sha, &connected, &open).unwrap_err().contains("running"));
        assert_eq!(settings(&s), SETTINGS_FILE, "nothing changed while RetroBat runs");

        apply(&s.catalog, &s.install, &changes, &sha, &connected, &closed).unwrap();
        let now = settings(&s);
        assert!(now.contains("<string name=\"snes.core\" value=\"bsnes\" />") && now.contains("snes.ratio"));
        let backups: Vec<PathBuf> = walkdir::WalkDir::new(s.root.join(".retrodrive/backup")).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file()).map(|e| e.into_path()).collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), SETTINGS_FILE, "the previous file is kept, unchanged");
        assert!(walkdir::WalkDir::new(&s.root).into_iter().filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().contains(TEMP_MARKER)));

        // A stale review is refused.
        assert!(apply(&s.catalog, &s.install, &[change("snes", "bezel", Some("none"))], &sha, &connected, &closed).unwrap_err().contains("changed since"));

        let h = history(&s.catalog, &s.install, &connected).unwrap();
        assert_eq!((h.len(), h[0].state.as_str(), h[0].blocked.as_deref()), (1, "applied", None));
        assert_eq!(h[0].changes.len(), 3);
        undo(&s.catalog, h[0].id, &connected, &closed).unwrap();
        assert_eq!(settings(&s), SETTINGS_FILE, "the previous settings are back");
        assert_eq!(history(&s.catalog, &s.install, &connected).unwrap()[0].state, "undone");
        assert!(undo(&s.catalog, h[0].id, &connected, &closed).is_err(), "only once");
    }

    #[test]
    fn one_game_gets_its_own_emulator_and_display_settings() {
        let s = setup();
        fs::write(s.root.join("roms/snes/F-Zero (USA).zip"), b"f").unwrap();
        let game = |key: &str, value: Option<&str>| Change { system: "snes".into(), key: key.into(), value: value.map(str::to_string), game: Some("F-Zero (USA).zip".into()) };
        let gs = game_settings(&s.catalog, &s.install, "snes", &connected).unwrap();
        assert_eq!(gs.games, ["F-Zero (USA).zip", "Mario.sfc"]);
        assert!(gs.current.is_empty());
        assert!(game_settings(&s.catalog, &s.install, "nope", &connected).is_err());

        // Refused: a game that isn't there, a name RetroBat couldn't use, a core without its emulator.
        let missing = Change { game: Some("Nope.zip".into()), ..game("ratio", Some("4/3")) };
        assert!(preview(&s.catalog, &s.install, &[missing], &connected).unwrap_err().contains("isn't in"));
        let quoted = Change { game: Some("a\"b.zip".into()), ..game("ratio", Some("4/3")) };
        assert!(preview(&s.catalog, &s.install, &[quoted], &connected).is_err());
        assert!(preview(&s.catalog, &s.install, &[game("core", Some("bsnes"))], &connected).unwrap_err().contains("needs an emulator"));

        let changes = [game("emulator", Some("libretro")), game("core", Some("bsnes")), game("ratio", Some("4/3"))];
        let p = preview(&s.catalog, &s.install, &changes, &connected).unwrap();
        assert!(p.changes.iter().all(|r| r.system == "snes: F-Zero (USA).zip"), "{:?}", p.changes);
        apply(&s.catalog, &s.install, &changes, &p.file_sha, &connected, &closed).unwrap();
        let now = settings(&s);
        assert!(now.contains("<string name=\"snes[&quot;F-Zero (USA).zip&quot;].core\" value=\"bsnes\" />"), "{now}");
        assert!(!now.contains("<string name=\"snes.core\""), "the system's own settings are untouched");
        let gs = game_settings(&s.catalog, &s.install, "snes", &connected).unwrap();
        assert_eq!(gs.current["F-Zero (USA).zip"].get("ratio").map(String::as_str), Some("4/3"), "read back by name");

        // "Same as the system" removes the game's line.
        let p = preview(&s.catalog, &s.install, &[game("ratio", None)], &connected).unwrap();
        apply(&s.catalog, &s.install, &[game("ratio", None)], &p.file_sha, &connected, &closed).unwrap();
        assert!(!settings(&s).contains("F-Zero (USA).zip&quot;].ratio"));
    }

    #[test]
    fn switching_the_theme_is_reviewed_backed_up_and_undoable() {
        let s = setup();
        let themes = s.root.join("emulationstation/.emulationstation/themes");
        fs::create_dir_all(themes.join("es-theme-carbon")).unwrap();
        fs::create_dir_all(themes.join("My Neon")).unwrap();
        fs::write(s.root.join(SETTINGS), SETTINGS_FILE.replace("</config>", "\t<string name=\"ThemeSet\" value=\"es-theme-carbon\" />\r\n</config>")).unwrap();
        let before = settings(&s);
        assert!(preview_theme(&s.catalog, &s.install, "Missing", &connected).is_err());
        assert!(preview_theme(&s.catalog, &s.install, "es-theme-carbon", &connected).unwrap_err().contains("already"));
        let p = preview_theme(&s.catalog, &s.install, "My Neon", &connected).unwrap();
        assert_eq!(p.changes, [ChangeRow { system: "".into(), key: "ThemeSet".into(), before: Some("es-theme-carbon".into()), after: Some("My Neon".into()) }]);
        assert!(apply_theme(&s.catalog, &s.install, "My Neon", &p.file_sha, &connected, &open).is_err(), "not while RetroBat runs");
        apply_theme(&s.catalog, &s.install, "My Neon", &p.file_sha, &connected, &closed).unwrap();
        assert_eq!(current_theme(&s.root).as_deref(), Some("My Neon"));
        let id = history(&s.catalog, &s.install, &connected).unwrap()[0].id;
        undo(&s.catalog, id, &connected, &closed).unwrap();
        assert_eq!(settings(&s), before);
    }

    const INI_FILE: &str = "; RETROBAT GLOBAL CONFIG FILE\r\n\r\n[RetroBat]\r\n\r\n; comment\r\nLanguageDetection=1\r\nAutostart=0\r\n\r\n[SplashScreen]\r\n\r\nEnableIntro=1\r\nFileName=\"retrobat-neon.mp4\"\r\nFilePath=\"default\"\r\nRandomVideo=1\r\nVideoDelay=1000\r\n\r\n[EmulationStation]\r\nInterfaceMode=0\r\n";

    #[test]
    fn boot_video_and_frontend_options_edit_retrobat_ini_safely() {
        let s = setup();
        fs::write(s.root.join(INI), INI_FILE).unwrap();
        let videos = s.root.join(VIDEOS);
        fs::create_dir_all(&videos).unwrap();
        fs::write(videos.join("retrobat-neon.mp4"), b"neon").unwrap();
        fs::write(videos.join("retrobat-space.mp4"), b"space").unwrap();
        let o = look_options(&s.catalog, &s.install, &connected, &closed).unwrap();
        assert_eq!(o.videos, ["retrobat-neon.mp4", "retrobat-space.mp4"]);
        assert_eq!(o.values.get("SplashScreen.RandomVideo").map(String::as_str), Some("1"));
        assert!(o.options.iter().all(|x| x.key != "Autostart"), "risky options aren't offered");
        assert!(o.options.iter().all(|x| x.key != "WaitForVideoEnd"), "only keys the file has");
        let ch = |section: &str, key: &str, value: &str| IniChange { section: section.into(), key: key.into(), value: value.into() };

        assert!(preview_look(&s.catalog, &s.install, &[ch("RetroBat", "Autostart", "1")], &connected).is_err());
        assert!(preview_look(&s.catalog, &s.install, &[ch("SplashScreen", "FileName", "..\\..\\evil.mp4")], &connected).is_err());
        assert!(preview_look(&s.catalog, &s.install, &[ch("EmulationStation", "InterfaceMode", "7")], &connected).is_err());
        let changes = [ch("SplashScreen", "FileName", "retrobat-space.mp4"), ch("SplashScreen", "RandomVideo", "0"), ch("EmulationStation", "InterfaceMode", "1")];
        let p = preview_look(&s.catalog, &s.install, &changes, &connected).unwrap();
        assert_eq!(p.changes.len(), 3);
        assert!(apply_look(&s.catalog, &s.install, &changes, &p.file_sha, &connected, &open).is_err());
        apply_look(&s.catalog, &s.install, &changes, &p.file_sha, &connected, &closed).unwrap();
        let now = fs::read_to_string(s.root.join(INI)).unwrap();
        assert_eq!(now, INI_FILE.replace("FileName=\"retrobat-neon.mp4\"", "FileName=\"retrobat-space.mp4\"").replace("RandomVideo=1", "RandomVideo=0").replace("InterfaceMode=0", "InterfaceMode=1"), "only those three values changed; comments and layout kept");
        let id = history(&s.catalog, &s.install, &connected).unwrap()[0].id;
        undo(&s.catalog, id, &connected, &closed).unwrap();
        assert_eq!(fs::read_to_string(s.root.join(INI)).unwrap(), INI_FILE);
        assert!(walkdir::WalkDir::new(s.root.join(".retrodrive/backup")).into_iter().filter_map(Result::ok).any(|e| e.file_name() == "retrobat.ini"), "backups keep the file name");
    }

    #[test]
    fn adding_a_boot_video_never_replaces_one() {
        let s = setup();
        let videos = s.root.join(VIDEOS);
        fs::create_dir_all(&videos).unwrap();
        fs::write(videos.join("intro.mp4"), b"theirs").unwrap();
        let mine = s.root.join("mine");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("intro.mp4"), b"mine").unwrap();
        fs::write(mine.join("Gollum Boot.mp4"), b"mine2").unwrap();
        fs::write(mine.join("x.exe"), b"no").unwrap();
        assert!(add_boot_video(&s.catalog, &s.install, &mine.join("intro.mp4").display().to_string(), &connected).unwrap_err().contains("already"));
        assert_eq!(fs::read(videos.join("intro.mp4")).unwrap(), b"theirs");
        assert!(add_boot_video(&s.catalog, &s.install, &mine.join("x.exe").display().to_string(), &connected).is_err());
        assert_eq!(add_boot_video(&s.catalog, &s.install, &mine.join("Gollum Boot.mp4").display().to_string(), &connected).unwrap(), "Gollum Boot.mp4");
        assert_eq!(fs::read(videos.join("Gollum Boot.mp4")).unwrap(), b"mine2");
        assert!(fs::read_dir(&videos).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(TEMP_MARKER)));
        fs::write(mine.join("Theme Song.mp3"), b"song").unwrap();
        assert_eq!(add_music(&s.catalog, &s.install, &mine.join("Theme Song.mp3").display().to_string(), &connected).unwrap(), "Theme Song.mp3");
        assert_eq!(music_files(&s.root), ["Theme Song.mp3"]);
        assert!(add_music(&s.catalog, &s.install, &mine.join("Gollum Boot.mp4").display().to_string(), &connected).is_err(), "only music files");
    }

    #[test]
    fn a_new_file_is_created_journaled_and_undone_by_moving_it_aside() {
        let s = setup();
        let install = install(&s.catalog, &s.install, &connected).unwrap();
        let path = s.root.join(FAVORITES);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = favorites_text(&install.root_text, &["Boss.ogg".to_string(), "snes/Tune.mp3".to_string()]);
        assert!(text.ends_with(";Tune\r\n") && text.contains("/emulationstation/.emulationstation/music/snes/Tune.mp3;Tune"));
        let (_, id) = write_file(&s.catalog, &install, FAVORITES, &path, "", &text, Vec::new()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert_eq!(favorite_tracks(&s.root, &install.root_text).unwrap(), ["Boss.ogg", "snes/Tune.mp3"]);
        undo(&s.catalog, id, &connected, &closed).unwrap();
        assert!(!path.exists(), "undo moves the created file away");
        let moved: Vec<PathBuf> = walkdir::WalkDir::new(s.root.join(".retrodrive/backup")).into_iter().filter_map(Result::ok).filter(|e| e.file_name() == "favorites.m3u").map(|e| e.into_path()).collect();
        assert_eq!(moved.len(), 1, "never deleted");
        assert_eq!(fs::read_to_string(&moved[0]).unwrap(), text);
    }

    #[test]
    fn undo_refuses_when_retrobat_changed_the_settings_since() {
        let s = setup();
        let changes = [change("nes", "ratio", Some("16/9"))];
        let sha = preview(&s.catalog, &s.install, &changes, &connected).unwrap().file_sha;
        apply(&s.catalog, &s.install, &changes, &sha, &connected, &closed).unwrap();
        let id = history(&s.catalog, &s.install, &connected).unwrap()[0].id;
        assert!(undo(&s.catalog, id, &unplugged, &closed).unwrap_err().contains("isn't connected"));
        assert!(undo(&s.catalog, id, &connected, &open).unwrap_err().contains("running"));
        let edited = settings(&s).replace("</config>", "\t<string name=\"LastSystem\" value=\"nes\" />\r\n</config>");
        fs::write(s.root.join(SETTINGS), &edited).unwrap();
        assert!(undo(&s.catalog, id, &connected, &closed).unwrap_err().contains("changed since"));
        assert_eq!(settings(&s), edited, "nothing was touched");
    }

    #[test]
    fn interrupted_changes_are_put_right_at_startup() {
        let s = setup();
        let changes = [change("snes", "bezel", Some("my_bezels"))];
        let sha = preview(&s.catalog, &s.install, &changes, &connected).unwrap().file_sha;
        apply(&s.catalog, &s.install, &changes, &sha, &connected, &closed).unwrap();
        let (id, backup, file): (i64, String, String) = s.catalog.query_row("SELECT id, backup_path, file_path FROM settings_changes", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        let changed = fs::read(&file).unwrap();
        let state = || s.catalog.query_row("SELECT state FROM settings_changes WHERE id = ?1", [id], |r| r.get::<_, String>(0)).unwrap();

        // Crash after the current file moved to the backup folder, before the new one arrived.
        fs::rename(&file, s.root.join("lost-new.cfg")).unwrap();
        fs::rename(&backup, &file).unwrap();
        fs::rename(&file, &backup).unwrap();
        s.catalog.execute("UPDATE settings_changes SET state = 'applying' WHERE id = ?1", [id]).unwrap();
        assert_eq!(recover(&s.catalog, &unplugged).unwrap(), 0, "nothing happens while the drive is away");
        assert_eq!(recover(&s.catalog, &connected).unwrap(), 1);
        assert_eq!((state().as_str(), fs::read_to_string(&file).unwrap().as_str()), ("failed", SETTINGS_FILE));

        // Crash after both moves of an apply: recorded as applied.
        fs::rename(&file, &backup).unwrap();
        fs::write(&file, &changed).unwrap();
        s.catalog.execute("UPDATE settings_changes SET state = 'applying' WHERE id = ?1", [id]).unwrap();
        recover(&s.catalog, &connected).unwrap();
        assert_eq!(state(), "applied");

        // Crash in the middle of an undo: the changed file goes back, still applied.
        let undo_path = s.root.join(format!(".retrodrive/backup/settings-undo-{id}/emulationstation/.emulationstation/es_settings.cfg"));
        fs::create_dir_all(undo_path.parent().unwrap()).unwrap();
        fs::rename(&file, &undo_path).unwrap();
        s.catalog.execute("UPDATE settings_changes SET state = 'undoing', undo_path = ?2 WHERE id = ?1", params![id, undo_path.display().to_string()]).unwrap();
        recover(&s.catalog, &connected).unwrap();
        assert_eq!((state().as_str(), fs::read(&file).unwrap()), ("applied", changed));
    }
}
