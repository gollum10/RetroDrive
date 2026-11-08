//! Lakka playlists and box art (ADR-086; docs/18 §54). Lakka (RetroArch) shows games
//! through playlists, so after a copy RetroDrive adds the copied games to the playlist
//! of their system on Lakka's `Playlists` share, and copies the user's own PNG box art
//! to its `Thumbnails` share.
//!
//! Facts (RetroArch, checked 2026-11-09):
//! - A playlist is `<playlist_directory>/<database name>.lpl`, JSON (`playlist_use_old_format
//!   = false` on Lakka): top-level `version`, `default_core_path`, `default_core_name`,
//!   `label_display_mode`, `right_thumbnail_mode`, `left_thumbnail_mode`, `sort_mode`,
//!   `items`; each item `path`, `label`, `core_path`, `core_name`, `crc32`, `db_name`.
//!   `DETECT` for the core means RetroArch asks which core to use when the game starts.
//! - Box art: `<thumbnails_directory>/<database name>/Named_Boxarts/<label>.png`, where
//!   `&*/:`<>?\|` in the label are replaced by `_`. PNG only.
//!
//! Safety: a playlist is changed only by **adding** entries for games RetroDrive copied
//! (never removing or editing others), through `customizer::write_file` (journaled,
//! backed up on the Playlists share, undoable). A playlist that isn't JSON is left
//! alone. Box art files are new files only (temporary name, checked, renamed without
//! replacing); a picture already there is kept. Nothing is deleted.

use crate::adapters::lakka;
use crate::customizer::{self, ChangeRow, Install, PresenceFn};
use crate::gamelists;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Where Lakka's ROMs share is on the device.
const DEVICE_ROMS: &str = "/storage/roms";

/// One playlist's change, for review.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PlaylistChange {
    pub playlist: String,
    pub add: usize,
    pub already: usize,
    /// The playlist exists but isn't RetroArch's JSON format, so it's left alone.
    pub skipped: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct LakkaPreview {
    pub playlists: Vec<PlaylistChange>,
    pub art_to_add: usize,
    /// Games whose only art isn't PNG (RetroArch shows PNG thumbnails only).
    pub art_not_png: usize,
    /// Systems RetroDrive has no RetroArch playlist name for (their games aren't added).
    pub unknown_systems: Vec<String>,
    /// Checksums of the playlists as reviewed (`-` = none yet).
    pub shas: BTreeMap<String, String>,
}

struct Game {
    system: String,
    rel: String,
    label: String,
    art: Option<PathBuf>,
    has_other_art: bool,
}

/// RetroArch's file name for a thumbnail of `label`.
pub fn thumbnail_name(label: &str) -> String {
    let safe: String = label.chars().map(|c| if "&*/:`<>?\\|".contains(c) { '_' } else { c }).collect();
    format!("{safe}.png")
}

fn stem(rel: &str) -> String {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file).to_string()
}

/// Games RetroDrive copied to this Lakka (still on the share), with label and art.
fn copied(connection: &Connection, install: &Install, deployment: Option<&str>, presence: PresenceFn) -> Result<Vec<Game>, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.destination_path, i.game_file_id FROM deployment_items i
             JOIN deployments d ON d.id = i.deployment_id JOIN game_files gf ON gf.id = i.game_file_id
             WHERE d.installation_id = ?1 AND (?2 IS NULL OR d.id = ?2) AND gf.file_role = 'primary'
               AND i.state IN ('verified', 'identical', 'replaced') ORDER BY i.id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = statement.query_map(params![install.id, deployment], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    let roms = format!("{}\\", install.root.display().to_string().trim_end_matches(['\\', '/']));
    let mut online = HashMap::new();
    let mut seen: BTreeMap<String, Game> = BTreeMap::new();
    for (destination, file_id) in rows {
        let d = destination.replace('/', "\\");
        if d.len() <= roms.len() || !d[..roms.len()].eq_ignore_ascii_case(&roms) || !Path::new(&d).is_file() {
            continue;
        }
        let inside = d[roms.len()..].replace('\\', "/");
        let Some((system, rel)) = inside.split_once('/') else { continue };
        let (media, any) = gamelists::media_for(connection, &file_id, presence, &mut online)?;
        let art = media.iter().filter(|(rank, _, _)| *rank <= 2).map(|(_, _, p)| p).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) && p.is_file()).cloned();
        let title = gamelists::details_for(connection, &file_id)?.title;
        seen.insert(format!("{system}/{}", rel.to_lowercase()), Game { system: system.to_string(), rel: rel.to_string(), label: title.unwrap_or_else(|| stem(rel)), art, has_other_art: any });
    }
    Ok(seen.into_values().collect())
}

/// The installation and its Playlists and Thumbnails shares.
fn shares(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<(Install, Install, PathBuf), String> {
    let games = customizer::install_for(connection, installation_id, presence, &[lakka::ID], "Playlists are for Lakka installations.")?;
    let playlists = lakka::share(&games.root, "Playlists").filter(|p| p.is_dir()).ok_or("Lakka's Playlists share can't be reached. Check that Lakka is on and Samba is switched on (Settings → Services).")?;
    let thumbnails = lakka::share(&games.root, "Thumbnails").unwrap_or_else(|| playlists.clone());
    let root_text = playlists.display().to_string();
    let stable = Some(crate::volumes::share_stable_id(&root_text, None));
    let list_install = Install { id: games.id.clone(), root: playlists, root_text, stable };
    Ok((games, list_install, thumbnails))
}

struct PlannedList {
    file: String,
    path: PathBuf,
    old: String,
    new: Option<String>,
    change: PlaylistChange,
    added: Vec<String>,
}

/// The shares, the copied games, each playlist's change, and systems without a playlist name.
type Planned = (Install, Install, PathBuf, Vec<Game>, Vec<PlannedList>, Vec<String>);

fn new_playlist() -> Value {
    json!({ "version": "1.5", "default_core_path": "", "default_core_name": "", "label_display_mode": 0, "right_thumbnail_mode": 0, "left_thumbnail_mode": 0, "sort_mode": 0, "items": [] })
}

/// RetroArch's top-level order for a new playlist (serde_json would sort the keys).
fn render(doc: &Value, fresh: bool) -> Result<String, String> {
    if !fresh {
        return serde_json::to_string_pretty(doc).map_err(|e| e.to_string());
    }
    let mut out = String::from("{\n");
    let keys = ["version", "default_core_path", "default_core_name", "label_display_mode", "right_thumbnail_mode", "left_thumbnail_mode", "sort_mode"];
    for k in keys {
        out.push_str(&format!("  \"{k}\": {},\n", serde_json::to_string(&doc[k]).map_err(|e| e.to_string())?));
    }
    let items = serde_json::to_string_pretty(&doc["items"]).map_err(|e| e.to_string())?;
    out.push_str(&format!("  \"items\": {}\n}}\n", items.replace('\n', "\n  ")));
    Ok(out)
}

fn plan(connection: &Connection, installation_id: &str, deployment: Option<&str>, presence: PresenceFn) -> Result<Planned, String> {
    let (games_install, list_install, thumbnails) = shares(connection, installation_id, presence)?;
    let games = copied(connection, &games_install, deployment, presence)?;
    let mut by_list: BTreeMap<&'static str, Vec<&Game>> = BTreeMap::new();
    let mut unknown: Vec<String> = Vec::new();
    for g in &games {
        match lakka::playlist_name(&g.system) {
            Some(name) => by_list.entry(name).or_default().push(g),
            None if !unknown.contains(&g.system) => unknown.push(g.system.clone()),
            None => {}
        }
    }
    let mut lists = Vec::new();
    for (name, members) in by_list {
        let file = format!("{name}.lpl");
        let path = list_install.root.join(&file);
        let (old, exists) = match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => (customizer::read_text(&path)?, true),
            Ok(_) => return Err(format!("{} isn't a file, so RetroDrive won't use it.", path.display())),
            Err(_) => (String::new(), false),
        };
        let parsed = if exists { serde_json::from_str::<Value>(&old).ok().filter(|v| v.get("items").is_some_and(Value::is_array)) } else { Some(new_playlist()) };
        let Some(mut doc) = parsed else {
            lists.push(PlannedList { file, path, old, new: None, change: PlaylistChange { playlist: name.into(), add: 0, already: 0, skipped: true }, added: Vec::new() });
            continue;
        };
        let have: Vec<String> = doc["items"].as_array().map(|a| a.iter().filter_map(|i| i["path"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        let (mut add, mut already, mut added) = (0, 0, Vec::new());
        for g in members {
            let device = format!("{DEVICE_ROMS}/{}/{}", g.system, g.rel);
            if have.iter().any(|p| p == &device) {
                already += 1;
                continue;
            }
            add += 1;
            added.push(g.label.clone());
            doc["items"].as_array_mut().expect("items is an array").push(json!({ "path": device, "label": g.label, "core_path": "DETECT", "core_name": "DETECT", "crc32": "DETECT", "db_name": file }));
        }
        let new = (add > 0).then(|| render(&doc, !exists)).transpose()?;
        lists.push(PlannedList { file, path, old, new, change: PlaylistChange { playlist: name.into(), add, already, skipped: false }, added });
    }
    Ok((games_install, list_install, thumbnails, games, lists, unknown))
}

fn art_targets<'a>(games: &'a [Game], thumbnails: &Path) -> Vec<(&'a Path, PathBuf)> {
    games
        .iter()
        .filter_map(|g| {
            let name = lakka::playlist_name(&g.system)?;
            let art = g.art.as_deref()?;
            let target = thumbnails.join(name).join("Named_Boxarts").join(thumbnail_name(&g.label));
            (std::fs::symlink_metadata(&target).is_err()).then_some((art, target))
        })
        .collect()
}

fn sha(list: &PlannedList) -> String {
    if list.old.is_empty() && std::fs::symlink_metadata(&list.path).is_err() {
        "-".into()
    } else {
        customizer::hex_sha(list.old.as_bytes())
    }
}

/// What adding the copied games to Lakka's playlists (and box art) would do (reads only).
pub fn preview(connection: &Connection, installation_id: &str, deployment: Option<&str>, presence: PresenceFn) -> Result<LakkaPreview, String> {
    let (_, _, thumbnails, games, lists, unknown) = plan(connection, installation_id, deployment, presence)?;
    let art_to_add = art_targets(&games, &thumbnails).len();
    let art_not_png = games.iter().filter(|g| g.art.is_none() && g.has_other_art).count();
    Ok(LakkaPreview { shas: lists.iter().map(|l| (l.file.clone(), sha(l))).collect(), playlists: lists.into_iter().map(|l| l.change).collect(), art_to_add, art_not_png, unknown_systems: unknown })
}

/// Add the copied games to Lakka's playlists and copy their box art. `expected`: the
/// playlist checksums from the review (`None` right after a copy).
pub fn apply(connection: &Connection, installation_id: &str, deployment: Option<&str>, expected: Option<&BTreeMap<String, String>>, presence: PresenceFn) -> Result<String, String> {
    let (_, list_install, thumbnails, games, lists, unknown) = plan(connection, installation_id, deployment, presence)?;
    if let Some(expected) = expected {
        for l in &lists {
            if expected.get(&l.file).is_some_and(|s| s != &sha(l)) {
                return Err(format!("{} changed since you reviewed it (perhaps Lakka scanned games), so nothing was changed. Review again.", l.file));
            }
        }
    }
    let mut added = 0;
    let mut skipped = Vec::new();
    for l in &lists {
        if l.change.skipped {
            skipped.push(l.change.playlist.clone());
        }
        let Some(new) = &l.new else { continue };
        let rows = l.added.iter().map(|label| ChangeRow { system: l.change.playlist.clone(), key: "game".into(), before: None, after: Some(label.clone()) }).collect();
        customizer::write_file(connection, &list_install, &l.file, &l.path, &l.old, new, rows)?;
        added += l.change.add;
    }
    let mut art = 0;
    let mut art_failed = 0;
    for (source, target) in art_targets(&games, &thumbnails) {
        match gamelists::copy_image(source, &target) {
            Ok(()) => art += 1,
            Err(_) => art_failed += 1,
        }
    }
    if added == 0 && art == 0 && art_failed == 0 && skipped.is_empty() && unknown.is_empty() {
        return Err("Nothing new to add: these games are already in Lakka's playlists.".into());
    }
    let mut message = format!("{added} game{} added to Lakka's playlists", if added == 1 { "" } else { "s" });
    if art > 0 {
        message.push_str(&format!(", {art} box art picture{} added", if art == 1 { "" } else { "s" }));
    }
    message.push('.');
    if art_failed > 0 {
        message.push_str(&format!(" {art_failed} picture(s) couldn't be copied (nothing was replaced)."));
    }
    if !skipped.is_empty() {
        message.push_str(&format!(" Left alone (not RetroArch's current playlist format): {}.", skipped.join(", ")));
    }
    if !unknown.is_empty() {
        message.push_str(&format!(" No RetroArch playlist known for: {} (scan those folders on Lakka).", unknown.join(", ")));
    }
    message.push_str(" On Lakka, the games are under their system's playlist; the first time a game starts, Lakka asks which core to use. Undo for playlists is under \"Show every settings change\".");
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copier::{deploy, DeployRequest};
    use crate::volumes::{Presence, VolumeProbe, VolumeSpace};
    use crate::{db, deployment, installations, scanner};
    use std::fs;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn unknown(_: &str) -> VolumeProbe {
        VolumeProbe::unknown()
    }
    fn roomy(_: &Path) -> Option<VolumeSpace> {
        Some(VolumeSpace { free_bytes: 500 << 30, total_bytes: 1 << 40, filesystem: Some("NTFS".into()), cluster_bytes: 4096 })
    }
    struct Quiet;
    impl crate::copier::DeployObserver for Quiet {}

    #[test]
    fn copied_games_are_added_to_lakkas_playlists_with_their_box_art() {
        assert_eq!(thumbnail_name("Zelda: A Link? <1/2>"), "Zelda_ A Link_ _1_2_.png");
        let dir = tempfile::tempdir().unwrap();
        for d in ["ROMs", "Playlists", "Thumbnails", "System", "Configfiles/retroarch"] {
            fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        fs::write(dir.path().join("Configfiles/retroarch/retroarch.cfg"), "").unwrap();
        let roms = dir.path().join("ROMs");
        let library = dir.path().join("library");
        for (rel, bytes) in [("snes/Mario.sfc", &b"mario"[..]), ("snes/Zelda.sfc", b"zelda"), ("nes/Contra.nes", b"contra"), ("snes/images/Mario.png", b"\x89PNGmario"), ("snes/images/Zelda.jpg", b"jpg")] {
            fs::create_dir_all(library.join(rel).parent().unwrap()).unwrap();
            fs::write(library.join(rel), bytes).unwrap();
        }
        // An existing NES playlist (as a Lakka scan writes it) and a SNES thumbnail already there.
        let nes_list = "{\n  \"version\": \"1.5\",\n  \"items\": [\n    {\n      \"path\": \"/storage/roms/nes/Other.nes\",\n      \"label\": \"Other\"\n    }\n  ]\n}\n";
        fs::write(dir.path().join("Playlists/Nintendo - Nintendo Entertainment System.lpl"), nes_list).unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &library, &unknown).unwrap();
        let id = installations::add(&c, &roms, "lakka", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let plan = deployment::plan_with(&c, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert!(plan.copy_supported && plan.systems_prefix.is_empty());
        let request = DeployRequest { installation_id: &id, systems: None, fingerprint: &plan.fingerprint, choices: &choices, collection: None, sync: false };
        let summary = deploy(&c, None, &request, &connected, &roomy, &mut Quiet).unwrap();
        assert_eq!(summary.games_copied, 3, "{:?}", summary.issues);
        assert_eq!(fs::read(roms.join("snes/Mario.sfc")).unwrap(), b"mario");

        let p = preview(&c, &id, None, &connected).unwrap();
        let snes = p.playlists.iter().find(|l| l.playlist == "Nintendo - Super Nintendo Entertainment System").unwrap();
        assert_eq!((snes.add, snes.already, snes.skipped), (2, 0, false));
        assert_eq!((p.art_to_add, p.art_not_png), (1, 1), "Mario's PNG; Zelda's art is a JPG");
        assert_eq!(p.shas["Nintendo - Super Nintendo Entertainment System.lpl"], "-", "no SNES playlist yet");

        // A changed playlist since the review is refused.
        let mut stale = p.shas.clone();
        stale.insert("Nintendo - Nintendo Entertainment System.lpl".into(), "x".into());
        assert!(apply(&c, &id, None, Some(&stale), &connected).unwrap_err().contains("changed since"));

        let message = apply(&c, &id, None, Some(&p.shas), &connected).unwrap();
        assert!(message.contains("3 games added") && message.contains("1 box art"), "{message}");
        let snes_text = fs::read_to_string(dir.path().join("Playlists/Nintendo - Super Nintendo Entertainment System.lpl")).unwrap();
        assert!(snes_text.starts_with("{\n  \"version\": \"1.5\",\n  \"default_core_path\": \"\","), "RetroArch's order: {snes_text}");
        let doc: Value = serde_json::from_str(&snes_text).unwrap();
        assert_eq!(doc["items"][0]["path"], "/storage/roms/snes/Mario.sfc");
        assert_eq!(doc["items"][0]["core_path"], "DETECT");
        assert_eq!(doc["items"][0]["db_name"], "Nintendo - Super Nintendo Entertainment System.lpl");
        let nes: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("Playlists/Nintendo - Nintendo Entertainment System.lpl")).unwrap()).unwrap();
        assert_eq!(nes["items"].as_array().unwrap().len(), 2, "added to, never removed from");
        assert_eq!(nes["items"][0]["path"], "/storage/roms/nes/Other.nes");
        let thumb = dir.path().join("Thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts").join(thumbnail_name(doc["items"][0]["label"].as_str().unwrap()));
        assert_eq!(fs::read(&thumb).unwrap(), b"\x89PNGmario");

        // Again: nothing new.
        assert!(apply(&c, &id, None, None, &connected).unwrap_err().contains("Nothing new"));
        // Undo puts the NES playlist back exactly.
        let change: i64 = c.query_row("SELECT id FROM settings_changes WHERE file_path LIKE '%Entertainment System.lpl' AND file_path NOT LIKE '%Super%' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        customizer::undo(&c, change, &connected, &customizer::never_running).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("Playlists/Nintendo - Nintendo Entertainment System.lpl")).unwrap(), nes_list);

        // A playlist in RetroArch's old line format is left alone.
        fs::write(dir.path().join("Playlists/Nintendo - Nintendo Entertainment System.lpl"), "/storage/roms/nes/Other.nes\nOther\nDETECT\nDETECT\nDETECT\nx.lpl\n").unwrap();
        let p = preview(&c, &id, None, &connected).unwrap();
        assert!(p.playlists.iter().any(|l| l.playlist == "Nintendo - Nintendo Entertainment System" && l.skipped));
    }
}
