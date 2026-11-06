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

const SETTINGS: &str = "emulationstation/.emulationstation/es_settings.cfg";
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
struct Entry {
    name: String,
    value: String,
    /// Byte range of the element (`<string … />`).
    start: usize,
    end: usize,
}

fn unescape(text: &str) -> String {
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
fn entries(text: &str) -> Result<Vec<Entry>, String> {
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
fn well_formed(text: &str) -> Result<(), String> {
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => return Ok(()),
            Err(e) => return Err(format!("not valid XML: {e}")),
            _ => {}
        }
    }
}

fn values(list: &[Entry]) -> Result<BTreeMap<String, String>, String> {
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
    let list = entries(text)?;
    let before = values(&list)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    // Replace or remove existing entries, last first so positions stay valid.
    let mut out = text.to_string();
    let mut existing: Vec<&Entry> = list.iter().filter(|e| wanted.contains_key(&e.name)).collect();
    existing.sort_by_key(|e| std::cmp::Reverse(e.start));
    for e in existing {
        match &wanted[&e.name] {
            Some(value) => out.replace_range(e.start..e.end, &format!("<string name=\"{}\" value=\"{}\" />", escape(&e.name), escape(value))),
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
    // Add new entries before </config>, indented like the others.
    let additions: Vec<(&String, &String)> = wanted.iter().filter(|(k, _)| !before.contains_key(*k)).filter_map(|(k, v)| v.as_ref().map(|v| (k, v))).collect();
    if !additions.is_empty() {
        let close = out.rfind("</config>").ok_or("es_settings.cfg has no </config>, so RetroDrive won't edit it.")?;
        let line_start = out[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let indent = list.first().map(|e| {
            let s = text[..e.start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            text[s..e.start].to_string()
        });
        let indent = indent.filter(|i| i.trim().is_empty()).unwrap_or_else(|| "\t".to_string());
        let mut block = String::new();
        for (name, value) in additions {
            block.push_str(&format!("{indent}<string name=\"{}\" value=\"{}\" />{newline}", escape(name), escape(value)));
        }
        if out[line_start..close].trim().is_empty() {
            out.insert_str(line_start, &block);
        } else {
            out.insert_str(close, &format!("{newline}{block}"));
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

fn read_text(path: &Path) -> Result<String, String> {
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

struct Install {
    id: String,
    root: PathBuf,
    root_text: String,
    stable: Option<String>,
}

fn install(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<Install, String> {
    let located = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if located.adapter_id != "retrobat" {
        return Err("Emulator settings are only available for RetroBat so far.".into());
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

fn running_message(running: RunningFn) -> Option<String> {
    running().then(|| "RetroBat is running. Close it first: it saves its own settings when it closes, which would undo these changes.".to_string())
}

struct Offer {
    systems: Vec<(String, String, Vec<EmulatorOption>)>,
    shaders: Vec<Choice>,
    bezels: Vec<Choice>,
    ratios: Vec<Choice>,
    integer: Vec<Choice>,
}

fn offer(root: &Path) -> Result<Offer, String> {
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

fn hex_sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Check every change against what the install offers; returns the entries to write.
fn validate(changes: &[Change], offer: &Offer, current: &BTreeMap<String, String>) -> Result<BTreeMap<String, Option<String>>, String> {
    if changes.is_empty() {
        return Err("There are no changes to apply.".into());
    }
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
        wanted.insert(format!("{}.{}", c.system, c.key), c.value.clone());
    }
    // The emulator and core must belong together after the change.
    for (system, _, emulators) in &offer.systems {
        let get = |key: &str| -> Option<String> {
            let k = format!("{system}.{key}");
            match wanted.get(&k) {
                Some(v) => v.clone(),
                None => current.get(&k).cloned(),
            }
        };
        if !changes.iter().any(|c| &c.system == system && (c.key == "emulator" || c.key == "core")) {
            continue;
        }
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
    if wanted.is_empty() {
        return Err("Those are already the current settings.".into());
    }
    Ok(wanted)
}

fn rows(wanted: &BTreeMap<String, Option<String>>, current: &BTreeMap<String, String>) -> Vec<ChangeRow> {
    wanted
        .iter()
        .map(|(k, v)| {
            let (system, key) = k.rsplit_once('.').unwrap_or((k, ""));
            ChangeRow { system: system.to_string(), key: key.to_string(), before: current.get(k).cloned(), after: v.clone() }
        })
        .collect()
}

/// The exact before → after values, without writing anything.
pub fn preview(connection: &Connection, installation_id: &str, changes: &[Change], presence: PresenceFn) -> Result<Preview, String> {
    let install = install(connection, installation_id, presence)?;
    let text = read_text(&install.root.join(SETTINGS))?;
    let current = values(&entries(&text)?)?;
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
    let wanted = validate(changes, &offer(&install.root)?, &current)?;
    let new_text = edit(&text, &wanted)?;
    let new_sha = hex_sha(new_text.as_bytes());
    let changed = rows(&wanted, &current);

    let token = db::new_id()[..8].to_string();
    let marker = format!("{TEMP_MARKER}{token}");
    let temp = PathBuf::from(format!("{}{marker}", settings.display()));
    let backup_relative = format!("{BACKUP_DIR}/settings-{token}/{SETTINGS}");
    copier::ensure_folders(&install.root, &backup_relative).map_err(|e| format!("Couldn't prepare the backup folder: {e} Nothing was changed."))?;
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
                backup.display().to_string(),
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
    let fail = |message: String| -> Result<String, String> {
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
    if let Err(e) = copier::rename_no_replace(&settings, &backup) {
        return fail(format!("Couldn't move the current settings to the backup folder: {e}. Nothing was changed."));
    }
    if let Err(e) = copier::rename_no_replace(&temp, &settings) {
        let restored = copier::rename_no_replace(&backup, &settings).is_ok();
        return fail(if restored {
            format!("Couldn't put the new settings in place ({e}), so the current ones were put back.")
        } else {
            format!("Couldn't put the new settings in place ({e}). Your settings are safe in {}; restart RetroDrive with the drive connected to put them back.", backup.display())
        });
    }
    connection.execute("UPDATE settings_changes SET temp_path = NULL WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    let message = format!("{} setting(s) changed. The previous settings file is in {backup_relative}.", changed.len());
    set_state(connection, id, "applied", Some(&message))?;
    Ok(message)
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
    let Some(backup) = j.backup.as_deref().filter(|b| ours(j, b)) else { return Some("No backup is recorded.".into()) };
    if fs::symlink_metadata(backup).map(|m| !m.is_file()).unwrap_or(true) {
        return Some("The previous settings file isn't in the backup folder any more.".into());
    }
    match file_sha(Path::new(&j.file)) {
        Some(sha) if sha == j.new_sha => None,
        Some(_) => Some("RetroBat's settings changed since then (in RetroBat or by another change), so undoing would lose those changes.".into()),
        None => Some("RetroBat's settings file is missing.".into()),
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
    let undo_relative = format!("{BACKUP_DIR}/settings-undo-{}/{SETTINGS}", j.id);
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
            } else if let Some(backup) = backup.filter(|b| b.is_file()) {
                if copier::rename_no_replace(&backup, settings).is_ok() {
                    set_state(connection, j.id, "failed", Some("Interrupted during the change; the previous settings were put back."))?;
                    fixed += 1;
                }
            }
        } else {
            let undo = j.undo.as_deref().filter(|u| ours(&j, u)).map(PathBuf::from);
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

/// Whether RetroBat's interface or launcher is running on this computer.
#[cfg(windows)]
pub fn retrobat_running() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    const NAMES: [&str; 3] = ["retrobat.exe", "emulationstation.exe", "emulatorlauncher.exe"];
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
            if NAMES.contains(&name.as_str()) {
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
pub fn retrobat_running() -> bool {
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
        Change { system: system.into(), key: key.into(), value: value.map(str::to_string) }
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
