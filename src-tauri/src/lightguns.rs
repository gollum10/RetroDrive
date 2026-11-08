//! Light guns (RDS-028 step 3; owner 2026-11-07 "build A, B, and C, then do D";
//! ADR-066; research in docs/19).
//!
//! A. Recognize a light gun plugged into this PC. Guns show up in Windows as a mouse
//!    (plus keyboard), so they're found by their product name, using the names
//!    Batocera's per-gun udev rules match; GunCons also by USB ID. The user confirms,
//!    or picks a gun from the list (also without a gun plugged in); remembered.
//! B. A short guide for the user's gun on RetroBat and Batocera, from their wikis
//!    (docs/19 §1), and how far to sit (Batocera's table).
//! C. Light gun games in the library, from Batocera EmulationStation's
//!    `resources/gamesdb.xml` (MIT; `resources/gun_games.tsv`), matched the way
//!    EmulationStation matches them (`MameNames::isLightgun`).
//!
//! Reads only, except one catalog row per confirmed gun.

use crate::adapters;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

const GUN_GAMES: &str = include_str!("../resources/gun_games.tsv");

/// One kind of light gun and what to know about it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Gun {
    pub id: &'static str,
    pub name: &'static str,
    /// `ir` (aims at an IR light bar), `border` (sees a white border on screen),
    /// `wiimote` (IR bar with a Wiimote) or `crt` (needs a CRT TV).
    pub tracking: &'static str,
    /// How far to sit, as multiples of the screen's size (Batocera's table).
    pub distance: Option<(f32, f32)>,
    /// Steps for any system.
    pub setup: &'static [&'static str],
    pub retrobat: &'static [&'static str],
    pub batocera: &'static [&'static str],
    /// Whether RetroDrive can tell it apart by name (generic Arduino guns can't).
    pub detectable: bool,
}

/// Name words (lower case) that identify each gun; GunCons also by USB ID.
const NAME_WORDS: &[(&str, &[&str])] = &[
    ("sinden", &["sinden"]),
    ("gun4ir", &["gun4ir"]),
    ("aimtrak", &["aimtrak", "atrak"]),
    ("blamcon", &["blamcon"]),
    ("retroshooter", &["retro shooter"]),
    ("fusion", &["fusion lightgun", "fusion mini", "fusion piggie"]),
    ("openfire", &["openfire", "firecon"]),
    ("xgunner", &["xgunner"]),
    ("onehit", &["onehit"]),
    ("ae", &["ae lightgun"]),
    ("wiimote", &["wiimote pc adapter", "wiimote"]),
    ("guncon2", &["guncon2"]),
    ("guncon3", &["guncon3"]),
];
const USB_IDS: &[(&str, &str, &str)] = &[("0b9a", "016a", "guncon2"), ("0b9a", "0800", "guncon3")];

const RETROBAT_AUTO: &str = "RetroBat finds the gun by itself; keep it in its mouse mode.";
const BATOCERA_AUTO: &str = "Batocera finds the gun by itself. Calibrate it once; the calibration is kept in the gun.";
const IR_BAR: &str = "Mount the IR light bar (or LEDs) as the gun's guide shows: it's what the gun aims at, so where it sits decides how accurate the gun is.";

pub fn guns() -> &'static [Gun] {
    static GUNS: OnceLock<Vec<Gun>> = OnceLock::new();
    GUNS.get_or_init(|| {
        vec![
            Gun {
                id: "sinden",
                name: "Sinden Lightgun",
                tracking: "border",
                distance: Some((1.5, 1.5)),
                setup: &[
                    "Install the Sinden software (Lightgun.exe) from the Sinden website. Run it once, calibrate the gun and turn on \"Start automatically\".",
                    "The gun aims by seeing a white border around the picture. Turn off overscan or zoom on your TV so the border isn't cut off.",
                    "Plug the gun straight into the computer, not into a USB hub. Windows display scaling above 100% can throw the aim off.",
                    "Recoil models need their own power supply (not included with the gun).",
                ],
                retrobat: &[
                    "RetroBat needs the Sinden software running: put it in RetroBat's system\\tools\\sinden folder, or set SINDEN SOFTWARE PATH under GAME SETTINGS > GUNS > SINDEN.",
                    "RetroBat adds the white border by itself. If a standalone emulator shows no border, choose the sindenborder shader in RetroBat's menu.",
                ],
                batocera: &[
                    "Sinden support is built in. Border size, colour and recoil are under CONTROLLER & BLUETOOTH SETTINGS > SINDEN GUN SETTINGS.",
                    "On Batocera v40 or newer, the gun needs firmware 1.09 or newer.",
                ],
                detectable: true,
            },
            Gun {
                id: "gun4ir",
                name: "GUN4IR",
                tracking: "ir",
                distance: Some((1.2, 2.0)),
                setup: &["Place the 4 IR LEDs around your screen as the GUN4IR guide shows (the light bar).", "Calibrate it and change its settings with the GUN4IR tool on Windows; they're saved in the gun."],
                retrobat: &[RETROBAT_AUTO],
                batocera: &["Batocera (v36 or newer) finds the gun by itself. Settings are made with the GUN4IR tool on Windows."],
                detectable: true,
            },
            Gun {
                id: "aimtrak",
                name: "Ultimarc AimTrak",
                tracking: "ir",
                distance: Some((2.0, 2.5)),
                setup: &[IR_BAR, "Calibration can take a few tries."],
                retrobat: &[RETROBAT_AUTO],
                batocera: &["Batocera (v35 or newer) finds the gun by itself. Calibrate it once; the calibration is kept in the gun."],
                detectable: true,
            },
            Gun {
                id: "blamcon",
                name: "Blamcon",
                tracking: "ir",
                distance: Some((1.0, 2.0)),
                setup: &[IR_BAR],
                retrobat: &[RETROBAT_AUTO],
                batocera: &["Batocera (v42 or newer) finds the gun by itself. If the aim is off, adjust the offset when calibrating."],
                detectable: true,
            },
            Gun {
                id: "retroshooter",
                name: "Retro Shooter",
                tracking: "ir",
                distance: Some((2.0, 3.0)),
                setup: &[IR_BAR, "With the 4-LED bar the gun is more accurate, and you can sit closer, than with 2 LEDs."],
                retrobat: &[RETROBAT_AUTO],
                batocera: &["Batocera finds the gun by itself (RS1/RS2 from v39, RS3 Reaper from v42). Calibrate by holding the rear left button for 3 seconds."],
                detectable: true,
            },
            Gun { id: "fusion", name: "Fusion Lightgun", tracking: "ir", distance: Some((2.5, 2.5)), setup: &[IR_BAR], retrobat: &[RETROBAT_AUTO], batocera: &[BATOCERA_AUTO], detectable: true },
            Gun { id: "openfire", name: "OpenFIRE", tracking: "ir", distance: None, setup: &[IR_BAR], retrobat: &[RETROBAT_AUTO], batocera: &["Batocera (v42 or newer) finds the gun by itself."], detectable: true },
            Gun {
                id: "xgunner",
                name: "XGunner",
                tracking: "ir",
                distance: Some((2.0, 2.0)),
                setup: &[IR_BAR, "Each gun needs its own wireless dongle and a battery pack."],
                retrobat: &[RETROBAT_AUTO],
                batocera: &["Batocera (v42 or newer) finds the gun by itself."],
                detectable: true,
            },
            Gun { id: "onehit", name: "OneHit", tracking: "ir", distance: None, setup: &[IR_BAR], retrobat: &[RETROBAT_AUTO], batocera: &["Batocera (v42 or newer) finds the gun by itself."], detectable: true },
            Gun {
                id: "ae",
                name: "AE Lightgun",
                tracking: "ir",
                distance: Some((3.0, 3.5)),
                setup: &[IR_BAR, "It needs about 3 metres of space, and calibration can take a few tries."],
                retrobat: &[RETROBAT_AUTO],
                batocera: &[BATOCERA_AUTO],
                detectable: true,
            },
            Gun {
                id: "wiimote",
                name: "Wiimote with an IR bar",
                tracking: "wiimote",
                distance: Some((1.5, 2.5)),
                setup: &["Use a powered IR (sensor) bar, with one LED visible at the top or bottom centre of the screen."],
                retrobat: &[
                    "With a Mayflash DolphinBar in mode 1 or 2 it works by itself (one DolphinBar per Wiimote).",
                    "For up to 4 Wiimotes, install Wiimote4Guns and put the DolphinBar in mode 4.",
                ],
                batocera: &["Connect the Wiimote straight over Bluetooth with a powered IR bar: much more accurate than the DolphinBar, which Batocera no longer supports. Third-party Wiimotes need the wiimote3rdparty service (v39+)."],
                detectable: true,
            },
            Gun {
                id: "guncon2",
                name: "Namco GunCon 2",
                tracking: "crt",
                distance: None,
                setup: &["It only works on an old CRT TV: it uses the CRT's picture timing to aim."],
                retrobat: &["Check RetroBat's light gun guide for CRT setups."],
                batocera: &["Batocera supports it on an analogue CRT; calibration runs through a CRT script."],
                detectable: true,
            },
            Gun {
                id: "guncon3",
                name: "Namco GunCon 3",
                tracking: "ir",
                distance: Some((1.5, 2.0)),
                setup: &[IR_BAR],
                retrobat: &["Check RetroBat's light gun guide for the GunCon 3."],
                batocera: &["Batocera (v38 or newer) supports it. Calibrate with button B2 (v39) or C2 (v40 and newer)."],
                detectable: true,
            },
            Gun {
                id: "samco",
                name: "SAMCO (DIY)",
                tracking: "ir",
                distance: Some((1.5, 1.5)),
                setup: &[IR_BAR],
                retrobat: &["Check RetroBat's light gun guide: DIY Arduino guns may need extra setup."],
                batocera: &[BATOCERA_AUTO],
                detectable: false,
            },
        ]
    })
}

pub fn gun(id: &str) -> Option<&'static Gun> {
    guns().iter().find(|g| g.id == id)
}

/// The gun a device is, from its product name or USB ID.
pub fn identify(vid: &str, pid: &str, product: Option<&str>) -> Option<&'static Gun> {
    if let Some((_, _, id)) = USB_IDS.iter().find(|(v, p, _)| v.eq_ignore_ascii_case(vid) && p.eq_ignore_ascii_case(pid)) {
        return gun(id);
    }
    let name = product?.to_lowercase();
    NAME_WORDS.iter().find(|(_, words)| words.iter().any(|w| name.contains(w))).and_then(|(id, _)| gun(id))
}

/// A light gun plugged into this PC.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectedGun {
    pub vid: String,
    pub pid: String,
    pub product: Option<String>,
    pub gun: Option<Gun>,
    /// The user confirmed this gun before.
    pub remembered: bool,
}

/// Light guns plugged into this PC (reads only). Mice that aren't guns are left out.
pub fn connected(remembered: &BTreeMap<String, String>) -> Vec<ConnectedGun> {
    crate::controllers::pointing_devices()
        .into_iter()
        .filter_map(|(vid, pid, product, _, _)| {
            let mine = remembered.get(&format!("{vid}:{pid}")).and_then(|id| gun(id));
            let found = mine.or_else(|| identify(&vid, &pid, product.as_deref()))?;
            Some(ConnectedGun { remembered: mine.is_some(), gun: Some(found.clone()), vid, pid, product })
        })
        .collect()
}

/// Guns the user confirmed: `vid:pid` (or `manual` for one chosen without a gun plugged in) → gun id.
pub fn remembered(connection: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut statement = connection.prepare("SELECT usb_id, gun_id FROM gun_choices").map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Remember (or with `None` forget) the gun for `vid:pid`, or `manual`.
pub fn remember(connection: &Connection, usb_id: &str, gun_id: Option<&str>) -> Result<(), String> {
    let hex4 = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit());
    let key = usb_id.to_lowercase();
    if key != "manual" && !key.split_once(':').is_some_and(|(v, p)| hex4(v) && hex4(p)) {
        return Err("Not a USB ID".into());
    }
    match gun_id {
        Some(id) if gun(id).is_some() => connection
            .execute("INSERT INTO gun_choices (usb_id, gun_id) VALUES (?1, ?2) ON CONFLICT(usb_id) DO UPDATE SET gun_id = excluded.gun_id, decided_at = CURRENT_TIMESTAMP", rusqlite::params![key, id])
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Some(id) => Err(format!("Unknown light gun {id}")),
        None => connection.execute("DELETE FROM gun_choices WHERE usb_id = ?1", [key]).map(|_| ()).map_err(|e| e.to_string()),
    }
}

// ---------- C. light gun games ----------

struct GunGames {
    arcade: HashSet<String>,
    by_system: HashMap<String, Vec<String>>,
}

fn gun_games() -> &'static GunGames {
    static DB: OnceLock<GunGames> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = GunGames { arcade: HashSet::new(), by_system: HashMap::new() };
        for line in GUN_GAMES.lines().filter(|l| !l.starts_with('#')).skip(1) {
            let mut cols = line.split('\t');
            let (Some(system), Some(id)) = (cols.next(), cols.next()) else { continue };
            if system == "arcade" {
                db.arcade.insert(id.to_lowercase());
            } else {
                db.by_system.entry(system.to_string()).or_default().push(id.to_lowercase());
            }
        }
        db
    })
}

/// Number of light gun games in the list.
pub fn gun_game_count() -> usize {
    gun_games().arcade.len() + gun_games().by_system.values().map(Vec::len).sum::<usize>()
}

/// EmulationStation's `getIndexedName`: lower-case letters and digits outside () and [].
fn indexed_name(name: &str) -> String {
    let mut out = String::new();
    let (mut paren, mut block) = (0i32, 0i32);
    for c in name.to_lowercase().chars() {
        match c {
            '(' => paren += 1,
            ')' => paren = (paren - 1).max(0),
            '[' => block += 1,
            ']' => block = (block - 1).max(0),
            c if paren == 0 && block == 0 && c.is_ascii_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// Arcade folders (MAME short names).
const ARCADE_FOLDERS: &[&str] = &["mame", "fbneo", "cps1", "cps2", "cps3", "naomi", "atomiswave", "neogeo"];

/// Whether a game file is a light gun game, the way EmulationStation decides it:
/// arcade by its exact romset name; other systems by the cleaned name equal to, or
/// containing, a listed name.
pub fn is_gun_game(folder: &str, stem: &str) -> bool {
    let db = gun_games();
    if ARCADE_FOLDERS.contains(&folder) {
        return db.arcade.contains(&stem.to_lowercase());
    }
    let system = match folder {
        "msx1" | "msx2" => "msx",
        other => other,
    };
    let Some(ids) = db.by_system.get(system) else { return false };
    let name = indexed_name(stem);
    !name.is_empty() && ids.iter().any(|id| name == *id || name.contains(id.as_str()))
}

type Cache = Option<((i64, i64, i64, String), Arc<HashSet<String>>)>;

/// Ids of the library's light gun games (cached until the library changes).
pub fn game_ids(connection: &Connection) -> Result<Arc<HashSet<String>>, String> {
    static CACHE: Mutex<Cache> = Mutex::new(None);
    // Sizes plus the largest game id (random ids, so it also tells libraries apart).
    let key: (i64, i64, i64, String) = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM games), (SELECT COALESCE(MAX(rowid), 0) FROM game_files), (SELECT COUNT(*) FROM game_files), (SELECT COALESCE(MAX(id), '') FROM games)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| e.to_string())?;
    if let Ok(cache) = CACHE.lock() {
        if let Some((k, ids)) = cache.as_ref().filter(|(k, _)| *k == key) {
            let _ = k;
            return Ok(ids.clone());
        }
    }
    let mut statement = connection
        .prepare(
            "SELECT g.id, g.platform, gf.filename FROM games g JOIN game_releases r ON r.game_id = g.id
             JOIN game_files gf ON gf.release_id = r.id WHERE gf.file_role = 'primary'",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    let mut ids = HashSet::new();
    for row in rows {
        let (id, platform, filename) = row.map_err(|e| e.to_string())?;
        let (stem, extension) = filename.rsplit_once('.').unwrap_or((&filename, ""));
        if let Some(folder) = adapters::batocera_folder(&platform, &extension.to_lowercase()) {
            if is_gun_game(folder, stem) {
                ids.insert(id);
            }
        }
    }
    let ids = Arc::new(ids);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((key, ids.clone()));
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guns_are_recognized_by_name_and_remembered() {
        assert_eq!(identify("2341", "8042", Some("GUN4IR Pro Micro P1")).map(|g| g.id), Some("gun4ir"));
        assert_eq!(identify("16c0", "0f38", Some("SindenLightgun")).map(|g| g.id), Some("sinden"));
        assert_eq!(identify("d209", "1601", Some("Ultimarc ATRAK Device #1")).map(|g| g.id), Some("aimtrak"));
        assert_eq!(identify("0b9a", "016A", None).map(|g| g.id), Some("guncon2"));
        assert_eq!(identify("046d", "c077", Some("USB Optical Mouse")), None, "an ordinary mouse isn't a gun");
        assert_eq!(identify("1234", "5678", Some("Fusion Keyboard")), None);
        assert!(guns().iter().all(|g| !g.setup.is_empty() && !g.retrobat.is_empty() && !g.batocera.is_empty()));
        assert!(NAME_WORDS.iter().all(|(id, _)| gun(id).is_some()));
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        remember(&c, "manual", Some("sinden")).unwrap();
        remember(&c, "2341:8042", Some("gun4ir")).unwrap();
        remember(&c, "2341:8042", Some("samco")).unwrap();
        let mine = remembered(&c).unwrap();
        assert_eq!((mine.get("manual").map(String::as_str), mine.get("2341:8042").map(String::as_str)), (Some("sinden"), Some("samco")));
        assert!(remember(&c, "manual", Some("lasertag")).is_err());
        assert!(remember(&c, "nope", Some("sinden")).is_err());
        remember(&c, "manual", None).unwrap();
        assert_eq!(remembered(&c).unwrap().len(), 1);
    }

    #[test]
    fn light_gun_games_are_matched_like_emulationstation() {
        assert!(gun_game_count() > 700, "{}", gun_game_count());
        assert_eq!(indexed_name("Duck Hunt (World) [!]"), "duckhunt");
        assert!(is_gun_game("nes", "Duck Hunt (World)"));
        assert!(is_gun_game("nes", "Duck Hunt + Super Mario Bros (USA)"), "contains a listed name, as EmulationStation does");
        assert!(!is_gun_game("nes", "Super Mario Bros (World)"));
        assert!(!is_gun_game("snes", "Duck Hunt (World)"), "listed for another system");
        assert!(is_gun_game("mame", "2spicy") && is_gun_game("fbneo", "38ambushalley"));
        assert!(!is_gun_game("mame", "pacman"));
        assert!(is_gun_game("psx", "Area 51 (USA)"));
    }

    #[test]
    fn the_librarys_light_gun_games_are_found() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        for (folder, name) in [("nes", "Duck Hunt (World).nes"), ("nes", "Tetris (USA).nes"), ("snes", "Super Scope 6 (USA).sfc")] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
            fs::write(dir.path().join(folder).join(name), vec![0u8; 64]).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut c, None).unwrap();
        crate::scanner::scan(&mut c, dir.path(), &|_: &str| crate::volumes::VolumeProbe::unknown()).unwrap();
        let ids = game_ids(&c).unwrap();
        let titles: Vec<String> = ids.iter().map(|id| c.query_row("SELECT canonical_title FROM games WHERE id = ?1", [id], |r| r.get(0)).unwrap()).collect();
        assert!(titles.iter().any(|t: &String| t.contains("Duck Hunt")), "{titles:?}");
        assert!(!titles.iter().any(|t| t.contains("Tetris")), "{titles:?}");
        assert!(Arc::ptr_eq(&ids, &game_ids(&c).unwrap()), "cached while the library is unchanged");
    }
}

