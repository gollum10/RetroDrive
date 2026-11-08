//! Controllers workspace, step 1 (RDS-028; owner request 2026-11-07, "I like all
//! three"; docs/19): recognize the game controllers plugged into this PC, name them,
//! say what kind of controller they are and which systems they suit, and look any
//! controller up. Reads only: nothing is written anywhere.
//!
//! Data: the owner's controller lookup (`resources/controllers.tsv`, compiled from SDL
//! GameControllerDB and RetroArch joypad autoconfig at pinned commits;
//! `resources/controller_sources.tsv`), the owner's 12 controller types
//! (`resources/controller_types.tsv`) and the sources' licences
//! (`resources/THIRD_PARTY_CONTROLLERS.txt`, shown in the workspace as credit).
//!
//! Detection: Windows Raw Input lists HID devices with their USB vendor and product
//! IDs; game controllers are HID usage page 1 with usage 4 (joystick), 5 (gamepad) or
//! 8 (multi-axis). A device's own product name (`HidD_GetProductString`) is shown for
//! controllers the list doesn't know.

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const CONTROLLERS: &str = include_str!("../resources/controllers.tsv");
const TYPES: &str = include_str!("../resources/controller_types.tsv");
const SOURCES: &str = include_str!("../resources/controller_sources.tsv");
const NOTICES: &str = include_str!("../resources/THIRD_PARTY_CONTROLLERS.txt");

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ControllerType {
    pub id: String,
    pub name: String,
    pub layout: String,
    pub systems: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ControllerInfo {
    pub vid: String,
    pub pid: String,
    pub name: String,
    /// Other names the sources use for the same USB IDs.
    pub also_known_as: Vec<String>,
    pub controller_type: Option<ControllerType>,
    /// The word in the name the type was taken from (e.g. `xbox`).
    pub type_basis: Option<String>,
    pub sources: String,
    /// SDL has a mapping for it on Windows (RetroBat, ES-DE and others can use it as is).
    pub sdl_windows: bool,
    /// RetroArch autoconfig files for it (`dinput/…`, `sdl2/…`, `xinput/…`).
    pub retroarch_files: Vec<String>,
    /// Other entries in the list with the same USB IDs (generic or reused IDs).
    pub shares_usb_id: usize,
    /// The entry lists different products under one USB ID, so its type isn't trusted.
    pub mixed_names: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Connected {
    pub vid: String,
    pub pid: String,
    /// The device's own product name, when Windows reports one.
    pub product: Option<String>,
    pub info: Option<ControllerInfo>,
    /// Windows uses it as an Xbox (XInput) controller.
    pub xinput: bool,
    /// The style RetroDrive thinks it is; the user confirms it or picks another.
    pub suggested: Option<ControllerType>,
    /// Where the suggestion came from: `remembered` (the user confirmed it before),
    /// `list`, `name` (words in its name) or `xinput`.
    pub suggested_by: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct Reference {
    pub types: Vec<ControllerType>,
    pub sources: Vec<(String, String, String, String)>,
    pub controllers: usize,
    pub notices: &'static str,
}

struct Row {
    vid: String,
    pid: String,
    name: String,
    sources: String,
    sdl_platform: String,
    type_id: String,
    type_basis: String,
    retroarch_files: String,
}

fn rows() -> &'static [Row] {
    static ROWS: OnceLock<Vec<Row>> = OnceLock::new();
    ROWS.get_or_init(|| {
        CONTROLLERS
            .lines()
            .skip(1)
            .filter_map(|l| {
                let c: Vec<&str> = l.split('\t').collect();
                (c.len() >= 10 && !c[2].is_empty()).then(|| Row {
                    vid: c[0].to_lowercase(),
                    pid: c[1].to_lowercase(),
                    name: c[2].to_string(),
                    sources: c[3].to_string(),
                    sdl_platform: c[4].to_string(),
                    type_id: c[6].to_string(),
                    type_basis: c[7].to_string(),
                    retroarch_files: c[9].to_string(),
                })
            })
            .collect()
    })
}

pub fn types() -> Vec<ControllerType> {
    TYPES
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            (c.len() >= 4).then(|| ControllerType { id: c[0].into(), name: c[1].into(), layout: c[2].into(), systems: c[3].into() })
        })
        .collect()
}

fn type_by_id(id: &str) -> Option<ControllerType> {
    types().into_iter().find(|t| t.id == id)
}

/// The names in a row (sources join several with ` | `).
fn names(row: &Row) -> Vec<String> {
    row.name.split('|').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect()
}

/// One source entry as shown: its own names and facts only (unrelated devices can
/// share a USB ID, so entries are never merged). `shared`: other entries with the same ID.
fn info_from(row: &Row, shared: usize) -> ControllerInfo {
    let all = names(row);
    let mut files: Vec<String> = row.retroarch_files.split(';').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
    files.sort();
    files.dedup();
    // Some rows list several different products under one generic USB ID (e.g. 05ac:0004
    // "8BitDo Micro | … | PlayStation VR2 …"). Trust the type only when its keyword is
    // in at least half of the row's names.
    let basis = row.type_basis.to_lowercase();
    let with_basis = all.iter().filter(|n| !basis.is_empty() && n.to_lowercase().replace([' ', '-'], "").contains(&basis.replace([' ', '-'], ""))).count();
    let type_certain = !row.type_id.is_empty() && with_basis * 2 >= all.len();
    ControllerInfo {
        vid: row.vid.clone(),
        pid: row.pid.clone(),
        name: all.first().cloned().unwrap_or_default(),
        also_known_as: all.iter().skip(1).take(6).cloned().collect(),
        controller_type: type_certain.then(|| type_by_id(&row.type_id)).flatten(),
        mixed_names: !row.type_id.is_empty() && !type_certain,
        type_basis: Some(row.type_basis.clone()).filter(|b| !b.is_empty()),
        sources: row.sources.clone(),
        sdl_windows: row.sdl_platform.contains("Windows"),
        retroarch_files: files,
        shares_usb_id: shared,
    }
}

/// What the list knows about a controller by its USB IDs (4 hex digits each): the best
/// entry (one with a type, then one both sources know), noting other entries sharing the ID.
pub fn lookup(vid: &str, pid: &str) -> Option<ControllerInfo> {
    let (vid, pid) = (vid.to_lowercase(), pid.to_lowercase());
    let group: Vec<&Row> = rows().iter().filter(|r| r.vid == vid && r.pid == pid && !vid.is_empty()).collect();
    let best = group.iter().max_by_key(|r| (!r.type_id.is_empty(), r.sources.contains('+')))?;
    Some(info_from(best, group.len() - 1))
}

/// Controllers whose name contains `query`, or `vid:pid` (up to 50).
pub fn search(query: &str) -> Vec<ControllerInfo> {
    let q = query.trim().to_lowercase();
    if q.len() < 2 {
        return Vec::new();
    }
    if let Some((v, p)) = q.split_once(':').filter(|(v, p)| v.len() == 4 && p.len() == 4) {
        return lookup(v, p).into_iter().collect();
    }
    let mut per_id: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in rows().iter().filter(|r| !r.vid.is_empty()) {
        *per_id.entry((r.vid.clone(), r.pid.clone())).or_default() += 1;
    }
    let mut out: Vec<ControllerInfo> = rows()
        .iter()
        .filter(|r| r.name.to_lowercase().contains(&q))
        .map(|r| info_from(r, per_id.get(&(r.vid.clone(), r.pid.clone())).map(|n| n.saturating_sub(1)).unwrap_or(0)))
        .collect();
    out.sort_by_key(|i| i.name.to_lowercase());
    out.truncate(50);
    out
}

pub fn reference() -> Reference {
    let sources = SOURCES
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            (c.len() >= 4 && !c[0].eq_ignore_ascii_case("note")).then(|| (c[0].into(), c[1].into(), c[2].into(), c[3].into()))
        })
        .collect();
    Reference { types: types(), sources, controllers: rows().len(), notices: NOTICES }
}

/// A HID device seen by Windows: (vid, pid, product name, de-duplication key, XInput).
pub(crate) type Seen = (String, String, Option<String>, String, bool);

/// Game controllers plugged into this PC (reads only). `remembered`: styles the user
/// confirmed before, by `vid:pid`.
pub fn connected(remembered: &BTreeMap<String, String>) -> Vec<Connected> {
    devices().into_iter().map(|(vid, pid, product, _, xinput)| describe(vid, pid, product, xinput, remembered)).collect()
}

fn describe(vid: String, pid: String, product: Option<String>, xinput: bool, remembered: &BTreeMap<String, String>) -> Connected {
    let info = lookup(&vid, &pid);
    let (suggested, suggested_by) = match remembered.get(&format!("{vid}:{pid}")).and_then(|id| type_by_id(id)) {
        Some(t) => (Some(t), Some("remembered")),
        None => suggest(info.as_ref(), product.as_deref(), xinput),
    };
    Connected { vid, pid, product, info, xinput, suggested, suggested_by }
}

/// The style to suggest: the list's type when it is certain, else words in the
/// controller's name, else Xbox-style for controllers Windows uses as Xbox controllers.
fn suggest(info: Option<&ControllerInfo>, product: Option<&str>, xinput: bool) -> (Option<ControllerType>, Option<&'static str>) {
    if let Some(t) = info.and_then(|i| i.controller_type.clone()) {
        return (Some(t), Some("list"));
    }
    let named = product.and_then(guess_from_name).or_else(|| info.filter(|i| !i.mixed_names).and_then(|i| guess_from_name(&i.name)));
    if let Some(t) = named.and_then(type_by_id) {
        return (Some(t), Some("name"));
    }
    if xinput {
        return (type_by_id("CT07"), Some("xinput"));
    }
    (None, None)
}

/// A controller type from words in a name (whole words; phrases as written).
pub fn guess_from_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |list: &[&str]| list.iter().any(|w| if w.contains([' ', '-']) { lower.contains(w) } else { words.contains(w) });
    const RULES: [(&str, &[&str]); 9] = [
        ("CT11", &["gun", "lightgun", "sinden", "gun4ir", "aimtrak", "zapper", "phaser", "guncon", "light gun"]),
        ("CT12", &["wheel", "racing", "flight", "throttle", "hotas", "dance", "guitar", "drum", "drums", "bongos"]),
        ("CT09", &["arcade", "fightstick", "encoder", "ipac", "xinmo", "fight stick", "zero delay", "i-pac", "xin-mo"]),
        ("CT10", &["spinner", "trackball", "paddle", "paddles"]),
        ("CT07", &["xbox", "xinput", "gamecube", "dreamcast"]),
        ("CT06", &["playstation", "dualshock", "dualsense", "sixaxis", "ps2", "ps3", "ps4", "ps5"]),
        ("CT05", &["n64", "nintendo 64"]),
        ("CT04", &["snes", "genesis", "saturn", "megadrive", "super nintendo", "mega drive", "super famicom"]),
        ("CT03", &["nes", "famicom", "master system"]),
    ];
    RULES.iter().find(|(_, list)| has(list)).map(|(id, _)| *id)
}

/// Styles the user confirmed, by `vid:pid` (ADR-063).
pub fn remembered(connection: &rusqlite::Connection) -> Result<BTreeMap<String, String>, String> {
    let mut statement = connection.prepare("SELECT usb_id, type_id FROM controller_choices").map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Controllers the user confirmed: (vid, pid, XInput), oldest first.
pub fn confirmed(connection: &rusqlite::Connection) -> Result<Vec<(String, String, bool)>, String> {
    let mut statement = connection.prepare("SELECT usb_id, xinput FROM controller_choices ORDER BY decided_at, usb_id").map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (usb, xinput) = row.map_err(|e| e.to_string())?;
        if let Some((vid, pid)) = usb.split_once(':') {
            out.push((vid.to_string(), pid.to_string(), xinput));
        }
    }
    Ok(out)
}

/// A controller's name for messages: the list's name, else its USB ID.
pub fn display_name(vid: &str, pid: &str) -> String {
    lookup(vid, pid).filter(|i| !i.mixed_names).map(|i| i.name).unwrap_or_else(|| format!("controller {vid}:{pid}"))
}

/// Remember the style the user confirmed for a controller (`None` forgets it).
pub fn remember(connection: &rusqlite::Connection, vid: &str, pid: &str, type_id: Option<&str>, xinput: bool) -> Result<(), String> {
    let hex4 = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit());
    if !hex4(vid) || !hex4(pid) {
        return Err("Not a USB ID".into());
    }
    let usb_id = format!("{}:{}", vid.to_lowercase(), pid.to_lowercase());
    match type_id {
        Some(id) => {
            if type_by_id(id).is_none() {
                return Err(format!("Unknown controller type {id}"));
            }
            connection
                .execute(
                    "INSERT INTO controller_choices (usb_id, type_id, xinput) VALUES (?1, ?2, ?3) ON CONFLICT(usb_id) DO UPDATE SET type_id = excluded.type_id, xinput = excluded.xinput, decided_at = CURRENT_TIMESTAMP",
                    rusqlite::params![usb_id, id, xinput],
                )
                .map_err(|e| e.to_string())?;
        }
        None => {
            connection.execute("DELETE FROM controller_choices WHERE usb_id = ?1", [usb_id]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// HID game controllers: usage page 1, usage 4 (joystick), 5 (gamepad) or 8 (multi-axis).
fn devices() -> Vec<Seen> {
    hid_scan(&|page, usage, _, _| page == 1 && matches!(usage, 4 | 5 | 8))
}

/// HID mice (usage page 1, usage 2), where light guns show up, plus GunCons by USB ID
/// (light guns, ADR-066).
pub(crate) fn pointing_devices() -> Vec<Seen> {
    hid_scan(&|page, usage, vid, _| (page == 1 && usage == 2) || vid == "0b9a")
}

#[cfg(windows)]
fn hid_scan(want: &dyn Fn(u16, u16, &str, &str) -> bool) -> Vec<Seen> {
    use windows_sys::Win32::UI::Input::{GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICEINFO, RIDI_DEVICENAME, RID_DEVICE_INFO, RIM_TYPEHID};
    let mut count: u32 = 0;
    let item = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    // SAFETY: first call only reports the count; buffers below are sized from it.
    unsafe {
        if GetRawInputDeviceList(std::ptr::null_mut(), &mut count, item) == u32::MAX || count == 0 {
            return Vec::new();
        }
        let mut list: Vec<RAWINPUTDEVICELIST> = vec![std::mem::zeroed(); count as usize];
        let got = GetRawInputDeviceList(list.as_mut_ptr(), &mut count, item);
        if got == u32::MAX {
            return Vec::new();
        }
        list.truncate(got as usize);
        let mut out: Vec<Seen> = Vec::new();
        for d in list.iter().filter(|d| d.dwType == RIM_TYPEHID) {
            let mut info: RID_DEVICE_INFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
            let mut size = info.cbSize;
            if GetRawInputDeviceInfoW(d.hDevice, RIDI_DEVICEINFO, &mut info as *mut _ as *mut _, &mut size) == u32::MAX {
                continue;
            }
            let hid = info.Anonymous.hid;
            let (vid, pid) = (format!("{:04x}", hid.dwVendorId & 0xffff), format!("{:04x}", hid.dwProductId & 0xffff));
            if !want(hid.usUsagePage, hid.usUsage, &vid, &pid) {
                continue;
            }
            let mut len: u32 = 0;
            GetRawInputDeviceInfoW(d.hDevice, RIDI_DEVICENAME, std::ptr::null_mut(), &mut len);
            let mut name = vec![0u16; len as usize + 1];
            let path = if GetRawInputDeviceInfoW(d.hDevice, RIDI_DEVICENAME, name.as_mut_ptr() as *mut _, &mut len) != u32::MAX {
                String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())])
            } else {
                String::new()
            };
            let key = dedupe_key(&path);
            if out.iter().any(|s| s.3 == key && !key.is_empty()) {
                continue;
            }
            // XInput devices have `IG_` in their device path.
            let xinput = path.to_lowercase().contains("ig_");
            out.push((vid, pid, product_name(&name), key, xinput));
        }
        out
    }
}

/// One entry per physical device: a device with several HID collections lists each
/// (`…&col01`, `…&col02`); identical controllers keep separate instance paths.
fn dedupe_key(path: &str) -> String {
    let lower = path.to_lowercase();
    let without_col = match lower.find("&col") {
        Some(at) => {
            let rest = &lower[at + 4..];
            let skip = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
            format!("{}{}", &lower[..at], &rest[skip..])
        }
        None => lower,
    };
    // `…#vid_…&pid_…#7&1a2b&0&0000#{guid}`: the instance's last number differs per collection.
    let mut parts: Vec<String> = without_col.split('#').map(str::to_string).collect();
    if parts.len() >= 3 {
        if let Some(at) = parts[2].rfind('&') {
            parts[2].truncate(at);
        }
    }
    parts.join("#")
}

/// The device's own product name (opens the device without read or write access).
#[cfg(windows)]
fn product_name(path: &[u16]) -> Option<String> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetProductString;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    // SAFETY: NUL-terminated path; zero access rights (query only); handle closed below.
    unsafe {
        let handle = CreateFileW(path.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut());
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut buffer = [0u16; 128];
        let ok = HidD_GetProductString(handle, buffer.as_mut_ptr() as *mut _, (buffer.len() * 2) as u32);
        CloseHandle(handle);
        if !ok {
            return None;
        }
        let text = String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())]);
        Some(text.trim().to_string()).filter(|t| !t.is_empty())
    }
}

#[cfg(not(windows))]
fn hid_scan(_want: &dyn Fn(u16, u16, &str, &str) -> bool) -> Vec<Seen> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_suggested_and_the_users_answer_is_remembered() {
        assert_eq!(guess_from_name("PowerA Xbox Series Controller"), Some("CT07"));
        assert_eq!(guess_from_name("Sony DualSense Wireless Controller"), Some("CT06"));
        assert_eq!(guess_from_name("8BitDo SN30 Pro for SNES"), Some("CT04"), "SNES isn't NES");
        assert_eq!(guess_from_name("Sinden Lightgun"), Some("CT11"));
        assert_eq!(guess_from_name("Ultimarc I-PAC 2"), Some("CT09"));
        assert_eq!(guess_from_name("Generic USB Joystick"), None);
        assert_eq!(guess_from_name("Business Pad"), None, "whole words only");
        let none = BTreeMap::new();
        // Unknown to the list, but Windows uses it as an Xbox controller.
        let pad = describe("abcd".into(), "dcba".into(), Some("Wireless Controller".into()), true, &none);
        assert_eq!((pad.suggested.as_ref().map(|t| t.id.as_str()), pad.suggested_by), (Some("CT07"), Some("xinput")));
        // Unknown and nothing to go on: no suggestion, the user picks.
        let unknown = describe("abcd".into(), "dcba".into(), None, false, &none);
        assert!(unknown.suggested.is_none());
        // The list's certain type wins over the name.
        let n64 = describe("2dc8".into(), "9002".into(), Some("Xbox thing".into()), true, &none);
        assert_eq!((n64.suggested.as_ref().map(|t| t.id.as_str()), n64.suggested_by), (Some("CT05"), Some("list")));
        // The user's answer is remembered, wins, and can be forgotten.
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        remember(&c, "ABCD", "dcba", Some("CT09"), false).unwrap();
        remember(&c, "abcd", "dcba", Some("CT06"), true).unwrap();
        assert_eq!(confirmed(&c).unwrap(), vec![("abcd".to_string(), "dcba".to_string(), true)]);
        let mine = remembered(&c).unwrap();
        assert_eq!(mine.get("abcd:dcba").map(String::as_str), Some("CT06"));
        let pad = describe("abcd".into(), "dcba".into(), None, true, &mine);
        assert_eq!((pad.suggested.as_ref().map(|t| t.id.as_str()), pad.suggested_by), (Some("CT06"), Some("remembered")));
        assert!(remember(&c, "abcd", "dcba", Some("CT99"), false).is_err());
        assert!(remember(&c, "xyz", "dcba", Some("CT06"), false).is_err());
        remember(&c, "abcd", "dcba", None, false).unwrap();
        assert!(remembered(&c).unwrap().is_empty());
    }

    #[test]
    fn the_owners_list_is_loaded_and_looked_up() {
        let r = reference();
        assert_eq!(r.controllers, 1405);
        assert_eq!(r.types.len(), 12);
        assert_eq!(r.types[0].name, "Classic joystick");
        assert_eq!(r.sources.len(), 4, "the note row isn't a source");
        assert!(r.notices.contains("gamesdb.xml"));
        assert!(r.notices.contains("zlib") || r.notices.contains("Permission is granted"));
        assert!(r.notices.contains("MIT License"));
        // 8BitDo N64 (2dc8:9002): a three-prong controller, with its own name.
        let n64 = lookup("2DC8", "9002").expect("known");
        assert!(n64.name.contains("8BitDo N64"), "{}", n64.name);
        assert_eq!(n64.controller_type.as_ref().map(|t| t.id.as_str()), Some("CT05"));
        assert!(lookup("abcd", "dcba").is_none(), "an ID the list doesn't have");
        // Search by name and by vid:pid.
        assert!(search("afterglow ps3").iter().all(|c| c.controller_type.as_ref().is_some_and(|t| t.id == "CT06")));
        assert_eq!(search("2dc8:9002").len(), 1);
        assert!(search("x").is_empty(), "too short");
        // Unrelated devices sharing a USB ID aren't merged: the 8BitDo Micro doesn't become a PlayStation pad.
        let micro = search("8bitdo micro");
        assert!(!micro.is_empty() && micro.iter().all(|c| c.controller_type.as_ref().map(|t| t.id.as_str()) != Some("CT06")), "{micro:?}");
        let generic = micro.iter().find(|c| c.vid == "05ac" && c.pid == "0004").expect("the generic-ID row");
        assert!(generic.mixed_names && generic.controller_type.is_none(), "its type is shown as uncertain");
        assert!(lookup("0e6f", "0413").is_some_and(|c| c.controller_type.is_some_and(|t| t.id == "CT07")), "a row of name variants keeps its type");
    }

    #[test]
    fn several_collections_of_one_device_count_once() {
        let a = r"\\?\HID#VID_045E&PID_028E&IG_00&Col01#7&1a2b&0&0000#{4d1e55b2}";
        let b = r"\\?\HID#VID_045E&PID_028E&IG_00&Col02#7&1a2b&0&0001#{4d1e55b2}";
        assert_eq!(dedupe_key(a), dedupe_key(b));
        let other = r"\\?\HID#VID_045E&PID_028E&IG_01&Col01#7&9f8e&0&0000#{4d1e55b2}";
        assert_ne!(dedupe_key(a), other.to_lowercase());
        assert_ne!(dedupe_key(a), dedupe_key(other), "two identical controllers stay two");
    }
}

