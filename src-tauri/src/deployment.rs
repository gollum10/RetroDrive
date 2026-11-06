//! Deployment plans (RDS-009, docs/08). Read-only.
//!
//! Turns an adapter's placement into a plan for one installation: what would
//! happen to every file (copy, already there, already in place, duplicate,
//! conflict, blocked), whether it fits on the destination drive, and which
//! folders would be created. Nothing is created, copied or changed here. The
//! copier (RDS-010) must recheck everything just before it writes, because the
//! drive can change after the plan is made.

use crate::adapters::{self, Placement, PlacementFile, PlacementRequest};
use crate::installations::{self, PresenceFn};
use crate::paths;
use crate::volumes::{self, Presence, VolumeSpace};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub type SpaceFn<'a> = &'a dyn Fn(&Path) -> Option<VolumeSpace>;

/// Free space always left on the destination: the larger of 1 GiB and 2% of the drive.
pub const MIN_RESERVE_BYTES: u64 = 1 << 30;
pub const RESERVE_PERCENT: u64 = 2;
/// Longer full paths still work in RetroDrive, but some emulators can't open them.
const LONG_PATH: usize = 259;
const SAMPLE_LIMIT: usize = 50;
const ATTENTION_LIMIT: usize = 500;

/// What would happen to a file or game. Ordered from least to most serious; a
/// game takes the most serious action of its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The library file already *is* the destination (the RetroBat folder was scanned).
    InPlace,
    /// A file with the same name and size is already there; skipped.
    AlreadyThere,
    /// Another copy of the same file in the library is already planned; copied once.
    Duplicate,
    Copy,
    /// A different file with this name is already there; the existing file is kept.
    Conflict,
    /// The file can't be placed (filesystem limit, unsafe path, name clash...).
    Blocked,
}

#[derive(Debug, Serialize)]
pub struct PlanGame {
    pub title: String,
    pub platform: String,
    pub system_folder: String,
    pub action: Action,
    /// Why, for anything other than a plain copy.
    pub reason: Option<String>,
    pub source: String,
    /// Destination of the primary file.
    pub destination: String,
    pub files: usize,
    pub bytes: i64,
    pub bytes_to_copy: i64,
}

#[derive(Debug, Serialize)]
pub struct SystemChoice {
    pub system_folder: String,
    pub platforms: Vec<String>,
    pub games: i64,
    pub bytes: i64,
    pub selected: bool,
}

#[derive(Debug, Serialize)]
pub struct UnsupportedGroup {
    pub platform: String,
    pub reason: String,
    pub games: i64,
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct ActionCounts {
    pub copy: i64,
    pub already_there: i64,
    pub in_place: i64,
    pub duplicate: i64,
    pub conflict: i64,
    pub blocked: i64,
}

impl ActionCounts {
    fn add(&mut self, action: Action) {
        match action {
            Action::Copy => self.copy += 1,
            Action::AlreadyThere => self.already_there += 1,
            Action::InPlace => self.in_place += 1,
            Action::Duplicate => self.duplicate += 1,
            Action::Conflict => self.conflict += 1,
            Action::Blocked => self.blocked += 1,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Capacity {
    /// False when Windows couldn't measure the drive; `fits` is then unknown.
    pub measured: bool,
    pub filesystem: Option<String>,
    pub free_bytes: i64,
    pub total_bytes: i64,
    pub reserve_bytes: i64,
    /// Room for one partly written file if a copy has to be retried.
    pub staging_bytes: i64,
    /// Space the copy would use: files rounded up to whole clusters, new folders and staging.
    pub required_bytes: i64,
    pub fits: Option<bool>,
    pub max_file_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DeploymentPlan {
    pub installation_id: String,
    pub adapter_id: String,
    pub adapter_name: String,
    pub adapter_contract: u32,
    pub installation_path: String,
    /// Every system folder with placeable games, and whether it's in this plan.
    pub systems: Vec<SystemChoice>,
    pub unsupported: Vec<UnsupportedGroup>,
    /// Games left out because their drive isn't connected or a file is missing.
    pub unavailable_games: i64,
    /// Games in the selected systems, by action.
    pub games: ActionCounts,
    pub selected_games: i64,
    pub files_to_copy: i64,
    pub bytes_to_copy: i64,
    pub folders_to_create: i64,
    pub long_paths: i64,
    pub capacity: Capacity,
    /// Conflicts and blocked games (the first 500).
    pub attention: Vec<PlanGame>,
    /// The first games that would be copied.
    pub sample: Vec<PlanGame>,
    /// Stable hash of every source, destination, size and action. RDS-010 rebuilds
    /// the plan before copying and refuses to start if this changed.
    pub fingerprint: String,
}

/// One planned file. Shared with the copier (RDS-010), which works from exactly
/// what the plan contains.
pub(crate) struct ItemFile {
    pub game_file_id: String,
    pub source: PathBuf,
    pub relative: String,
    pub destination: PathBuf,
    pub size: i64,
    pub action: Action,
    pub reason: Option<String>,
}

pub(crate) struct Item {
    pub title: String,
    pub platform: String,
    pub system_folder: String,
    pub files: Vec<ItemFile>,
}

impl Item {
    /// The game's action: the most serious action of its files.
    pub fn action(&self) -> Action {
        self.files.iter().map(|f| f.action).max().unwrap_or(Action::Copy)
    }
}

/// Build the plan for an installation. `systems` limits it to those system folders
/// (`None` = all). Reads the catalog and the destination; writes nothing.
pub fn plan(connection: &Connection, installation_id: &str, systems: Option<&[String]>, presence: PresenceFn, space: SpaceFn) -> Result<DeploymentPlan, String> {
    build(connection, installation_id, systems, presence, space).map(|(plan, _)| plan)
}

/// The plan plus every selected item, for the copier.
pub(crate) fn build(connection: &Connection, installation_id: &str, systems: Option<&[String]>, presence: PresenceFn, space: SpaceFn) -> Result<(DeploymentPlan, Vec<Item>), String> {
    let install = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if install.presence != Presence::Connected {
        return Err("The installation's drive isn't connected, so a plan can't be made.".into());
    }
    let adapter = adapters::find(&install.adapter_id).ok_or_else(|| format!("Unknown platform adapter: {}", install.adapter_id))?;
    let install_root = install.root.display().to_string();
    let gathered = gather(connection, adapter.as_ref(), &install_root, install.options, presence)?;

    // System folders on offer, and the selection.
    let mut choices: BTreeMap<String, SystemChoice> = BTreeMap::new();
    for item in &gathered.items {
        let choice = choices.entry(item.system_folder.clone()).or_insert_with(|| SystemChoice {
            system_folder: item.system_folder.clone(),
            platforms: Vec::new(),
            games: 0,
            bytes: 0,
            selected: systems.is_none_or(|s| s.iter().any(|x| x == &item.system_folder)),
        });
        if !choice.platforms.contains(&item.platform) {
            choice.platforms.push(item.platform.clone());
        }
        choice.games += 1;
        choice.bytes += item.files.iter().map(|f| f.size).sum::<i64>();
    }
    let mut items: Vec<Item> = gathered.items.into_iter().filter(|i| choices[&i.system_folder].selected).collect();

    let measured = space(&install.root);
    let max_file = measured.as_ref().and_then(|s| volumes::max_file_bytes(s.filesystem.as_deref()));
    check_files(&mut items, &install.root, max_file);

    // Totals.
    let cluster = measured.as_ref().map(|s| s.cluster_bytes.max(1)).unwrap_or(4096);
    let mut games = ActionCounts::default();
    let (mut files_to_copy, mut bytes_to_copy, mut allocated, mut largest, mut long_paths) = (0i64, 0i64, 0u64, 0u64, 0i64);
    let mut new_folders: BTreeSet<String> = BTreeSet::new();
    let mut attention = Vec::new();
    let mut sample = Vec::new();
    let mut fingerprint = Fnv::new();
    fingerprint.write(&install.adapter_id);
    fingerprint.write(&adapters::CONTRACT_VERSION.to_string());
    fingerprint.write(&install_root);
    for item in &items {
        let action = item.action();
        games.add(action);
        let mut game_copy_bytes = 0i64;
        for file in &item.files {
            fingerprint.write(&file.source.display().to_string());
            fingerprint.write(&file.destination.display().to_string());
            fingerprint.write(&file.size.to_string());
            fingerprint.write(&format!("{:?}", if action == Action::Copy { file.action } else { action }));
            if file.destination.as_os_str().len() > LONG_PATH {
                long_paths += 1;
            }
            if action == Action::Copy && file.action == Action::Copy {
                let size = u64::try_from(file.size).unwrap_or(0);
                files_to_copy += 1;
                game_copy_bytes += file.size;
                allocated += size.div_ceil(cluster) * cluster;
                largest = largest.max(size.div_ceil(cluster) * cluster);
                let mut parent = file.destination.parent();
                while let Some(dir) = parent {
                    if dir == install.root || dir.is_dir() {
                        break;
                    }
                    new_folders.insert(dir.display().to_string().to_lowercase());
                    parent = dir.parent();
                }
            }
        }
        bytes_to_copy += game_copy_bytes;
        let primary = &item.files[0];
        let reason = item.files.iter().filter(|f| f.action == action).find_map(|f| f.reason.clone());
        let game = PlanGame {
            title: item.title.clone(),
            platform: item.platform.clone(),
            system_folder: item.system_folder.clone(),
            action,
            reason,
            source: primary.source.display().to_string(),
            destination: primary.destination.display().to_string(),
            files: item.files.len(),
            bytes: item.files.iter().map(|f| f.size).sum(),
            bytes_to_copy: game_copy_bytes,
        };
        if matches!(action, Action::Conflict | Action::Blocked) {
            if attention.len() < ATTENTION_LIMIT {
                attention.push(game);
            }
        } else if action == Action::Copy && sample.len() < SAMPLE_LIMIT {
            sample.push(game);
        }
    }

    let folders = new_folders.len() as u64;
    let required = allocated + folders * cluster + largest;
    let capacity = match &measured {
        Some(s) => {
            let reserve = MIN_RESERVE_BYTES.max(s.total_bytes / 100 * RESERVE_PERCENT);
            Capacity {
                measured: true,
                filesystem: s.filesystem.clone(),
                free_bytes: to_i64(s.free_bytes),
                total_bytes: to_i64(s.total_bytes),
                reserve_bytes: to_i64(reserve),
                staging_bytes: to_i64(largest),
                required_bytes: to_i64(required),
                fits: Some(files_to_copy == 0 || required <= s.free_bytes.saturating_sub(reserve)),
                max_file_bytes: max_file.map(to_i64),
            }
        }
        None => Capacity {
            measured: false,
            filesystem: None,
            free_bytes: 0,
            total_bytes: 0,
            reserve_bytes: 0,
            staging_bytes: to_i64(largest),
            required_bytes: to_i64(required),
            fits: None,
            max_file_bytes: None,
        },
    };

    let plan = DeploymentPlan {
        installation_id: install.id,
        adapter_id: install.adapter_id,
        adapter_name: adapter.display_name().to_string(),
        adapter_contract: adapters::CONTRACT_VERSION,
        installation_path: install_root,
        selected_games: items.len() as i64,
        systems: choices.into_values().collect(),
        unsupported: gathered.unsupported.into_iter().map(|((platform, reason), games)| UnsupportedGroup { platform, reason, games }).collect(),
        unavailable_games: gathered.unavailable,
        games,
        files_to_copy,
        bytes_to_copy,
        folders_to_create: folders as i64,
        long_paths,
        capacity,
        attention,
        sample,
        fingerprint: format!("{:016x}", fingerprint.finish()),
    };
    Ok((plan, items))
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// A destination relative path is only allowed to name files inside the installation.
fn safe_relative(relative: &str) -> bool {
    !relative.is_empty()
        && !relative.starts_with(['/', '\\'])
        && relative.split(['/', '\\']).all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(':'))
}

/// Windows compares names without regard to case.
fn path_key(path: &Path) -> String {
    path.display().to_string().replace('/', "\\").to_lowercase()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ancestor {
    Folder,
    Missing,
    Link,
    File,
}

/// Classify each planned file against the destination. Only reads metadata.
fn check_files(items: &mut [Item], install_root: &Path, max_file: Option<u64>) {
    // Several library files aiming at one destination name: identical sizes are
    // duplicates (copy one); different sizes are a clash nobody can resolve
    // automatically. A source that already *is* the destination wins.
    let mut targets: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        for (j, file) in item.files.iter().enumerate() {
            targets.entry(path_key(&file.destination)).or_default().push((i, j));
        }
    }
    for (key, mut users) in targets {
        let distinct: BTreeSet<String> = users.iter().map(|&(i, j)| path_key(&items[i].files[j].source)).collect();
        if distinct.len() < 2 {
            continue;
        }
        let sizes: BTreeSet<i64> = users.iter().map(|&(i, j)| items[i].files[j].size).collect();
        if sizes.len() > 1 {
            for (i, j) in users {
                let file = &mut items[i].files[j];
                file.action = Action::Blocked;
                file.reason = Some("Two different files in your library would be copied to the same name.".into());
            }
            continue;
        }
        users.sort_by_key(|&(i, j)| (path_key(&items[i].files[j].source) != key, path_key(&items[i].files[j].source)));
        for &(i, j) in &users[1..] {
            let file = &mut items[i].files[j];
            file.action = Action::Duplicate;
            file.reason = Some("Another copy of this file in your library is already in the plan.".into());
        }
    }

    let mut folders: HashMap<PathBuf, Ancestor> = HashMap::new();
    for item in items.iter_mut() {
        for file in item.files.iter_mut() {
            if file.action != Action::Copy {
                continue;
            }
            let (action, reason) = check_file(file, install_root, max_file, &mut folders);
            file.action = action;
            file.reason = reason;
        }
    }
}

fn check_file(file: &ItemFile, install_root: &Path, max_file: Option<u64>, folders: &mut HashMap<PathBuf, Ancestor>) -> (Action, Option<String>) {
    if !safe_relative(&file.relative) {
        return (Action::Blocked, Some("The destination would be outside the installation folder.".into()));
    }
    if path_key(&file.source) == path_key(&file.destination) {
        return (Action::InPlace, Some("This file is already in the installation (that folder was scanned).".into()));
    }
    if let Some(limit) = max_file {
        if u64::try_from(file.size).unwrap_or(0) > limit {
            return (Action::Blocked, Some(format!("The file is larger than this drive's filesystem allows ({} bytes max).", limit)));
        }
    }
    // Every folder between the installation and the file must be a real folder (or missing).
    let mut current = install_root.to_path_buf();
    let parts: Vec<&str> = file.relative.split(['/', '\\']).collect();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        let state = *folders.entry(current.clone()).or_insert_with(|| match std::fs::symlink_metadata(&current) {
            Err(_) => Ancestor::Missing,
            Ok(m) if m.file_type().is_symlink() => Ancestor::Link,
            Ok(m) if m.is_dir() => Ancestor::Folder,
            Ok(_) => Ancestor::File,
        });
        match state {
            Ancestor::Missing => break,
            Ancestor::Folder => {}
            Ancestor::Link => return (Action::Blocked, Some(format!("{} is a link to another location; RetroDrive won't write through it.", current.display()))),
            Ancestor::File => return (Action::Blocked, Some(format!("{} is a file where a folder is needed.", current.display()))),
        }
    }
    match std::fs::symlink_metadata(&file.destination) {
        Err(_) => (Action::Copy, None),
        Ok(m) if m.file_type().is_symlink() => (Action::Blocked, Some("A link with this name is already there; RetroDrive won't write through it.".into())),
        Ok(m) if m.is_dir() => (Action::Blocked, Some("A folder with this name is already there.".into())),
        Ok(m) if m.len() == u64::try_from(file.size).unwrap_or(u64::MAX) => (Action::AlreadyThere, Some("A file with the same name and size is already there.".into())),
        Ok(m) => (
            Action::Conflict,
            Some(format!("A different file with this name is already there ({} bytes; yours is {} bytes). It will be kept.", m.len(), file.size)),
        ),
    }
}

/// (release, storage location) -> [(relative path, size, present status, game file id)].
type Members = HashMap<(String, String), Vec<(String, i64, String, String)>>;

struct Gathered {
    items: Vec<Item>,
    unsupported: BTreeMap<(String, String), i64>,
    unavailable: i64,
}

/// Every available catalog game, placed by the adapter.
fn gather(connection: &Connection, adapter: &dyn adapters::PlatformAdapter, install_root: &str, options: adapters::PlacementOptions, presence: PresenceFn) -> Result<Gathered, String> {
    // Discs and tracks per release, on the same storage location as the primary.
    let mut members: Members = HashMap::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT gf.release_id, fl.storage_location_id, fl.relative_path, gf.size_bytes, fl.present_status, gf.id
                 FROM game_files gf JOIN file_locations fl ON fl.game_file_id = gf.id
                 WHERE gf.file_role <> 'primary' ORDER BY fl.relative_path",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (release, storage, relative, size, status, id) = row.map_err(|e| e.to_string())?;
            members.entry((release, storage)).or_default().push((relative, size, status, id));
        }
    }

    let mut statement = connection
        .prepare(
            "SELECT g.canonical_title, g.platform, gf.extension, gf.release_id, gf.size_bytes, fl.relative_path, fl.present_status,
                    fl.storage_location_id, s.root_path, s.stable_id, gf.id
             FROM game_files gf
             JOIN game_releases r ON r.id = gf.release_id
             JOIN games g ON g.id = r.game_id
             JOIN file_locations fl ON fl.game_file_id = gf.id
             JOIN storage_locations s ON s.id = fl.storage_location_id
             WHERE gf.file_role = 'primary' AND gf.excluded_at IS NULL
             ORDER BY g.canonical_title COLLATE NOCASE, fl.relative_path",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, String>(10)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut connected: HashMap<String, bool> = HashMap::new();
    let mut gathered = Gathered { items: Vec::new(), unsupported: BTreeMap::new(), unavailable: 0 };
    for row in rows {
        let (title, platform, extension, release, size, relative, status, storage, root, stable, file_id) = row.map_err(|e| e.to_string())?;
        let online = *connected.entry(storage.clone()).or_insert_with(|| presence(&root, stable.as_deref()) == Presence::Connected);
        let parts = release.map(|r| members.get(&(r, storage)).cloned().unwrap_or_default()).unwrap_or_default();
        if !online || status != "present" || parts.iter().any(|m| m.2 != "present") {
            gathered.unavailable += 1;
            continue;
        }
        let primary_dir = relative.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
        let primary_name = relative.rsplit('/').next().unwrap_or(&relative).to_string();
        let mut sources = vec![(relative.clone(), file_id)];
        let mut files = vec![PlacementFile { relative_to_primary: primary_name, size }];
        for (member_relative, member_size, _, member_id) in &parts {
            let relative_to_primary = member_relative.strip_prefix(&primary_dir).map(str::to_string).unwrap_or_else(|| format!("../{member_relative}"));
            files.push(PlacementFile { relative_to_primary, size: *member_size });
            sources.push((member_relative.clone(), member_id.clone()));
        }
        let request = PlacementRequest { title: title.clone(), platform: platform.clone(), extension, files, options };
        match adapter.plan_placement(&request) {
            Placement::Planned { system_folder, destinations } => {
                let files = sources
                    .into_iter()
                    .zip(&request.files)
                    .zip(destinations)
                    .map(|(((source, game_file_id), planned), destination)| ItemFile {
                        game_file_id,
                        source: paths::join(&root, &source),
                        destination: paths::join(install_root, &destination),
                        relative: destination,
                        size: planned.size,
                        action: Action::Copy,
                        reason: None,
                    })
                    .collect();
                gathered.items.push(Item { title, platform, system_folder, files });
            }
            Placement::Unsupported { reason } => *gathered.unsupported.entry((platform, reason)).or_default() += 1,
        }
    }
    Ok(gathered)
}

/// 64-bit FNV-1a: small, dependency-free and stable across Rust versions.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, text: &str) {
        for byte in text.bytes().chain(std::iter::once(0)) {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::VolumeProbe;
    use crate::{db, scanner};
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

    fn write(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn retrobat(dir: &Path) -> PathBuf {
        let root = dir.join("RetroBat");
        for d in ["emulationstation", "emulators", "roms", "bios"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        fs::write(root.join("retrobat.ini"), b"").unwrap();
        root
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, u64)> {
        walkdir::WalkDir::new(root).sort_by_file_name().into_iter().map(|e| e.unwrap()).map(|e| (e.path().to_path_buf(), e.metadata().unwrap().len())).collect()
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        library: PathBuf,
        catalog: Connection,
        id: String,
    }

    fn fixture(files: &[(&str, &[u8])]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = retrobat(dir.path());
        let library = dir.path().join("library");
        for (path, contents) in files {
            write(&library, path, contents);
        }
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &root, "retrobat", &unknown).unwrap();
        Fixture { _dir: dir, root, library, catalog, id }
    }

    fn game<'a>(plan: &'a DeploymentPlan, title: &str) -> &'a PlanGame {
        plan.attention.iter().chain(&plan.sample).find(|g| g.title == title).unwrap_or_else(|| panic!("{title} not listed"))
    }

    #[test]
    fn plans_copies_by_system_and_never_writes() {
        let f = fixture(&[
            ("nes/Kid.nes", b"nes"),
            ("nes/Disk.fds", b"fds"),
            ("snes/Quest.sfc", b"snes!"),
            ("arcade/pacman.zip", b"zip"),
            ("psx/Saga/Saga.m3u", b"Saga (Disc 1).cue\nSaga (Disc 2).cue\n"),
            ("psx/Saga/Saga (Disc 1).cue", b"FILE \"d1.bin\" BINARY\n"),
            ("psx/Saga/Saga (Disc 2).cue", b"FILE \"d2.bin\" BINARY\n"),
            ("psx/Saga/d1.bin", &[0u8; 100]),
            ("psx/Saga/d2.bin", &[0u8; 200]),
        ]);
        let before = snapshot(&f.root);
        let plan = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        let folders: Vec<(&str, i64, bool)> = plan.systems.iter().map(|s| (s.system_folder.as_str(), s.games, s.selected)).collect();
        assert_eq!(folders, [("fds", 1, true), ("nes", 1, true), ("psx", 1, true), ("snes", 1, true)]);
        assert_eq!(plan.games, ActionCounts { copy: 4, ..ActionCounts::default() });
        assert_eq!(plan.files_to_copy, 8, "three single files plus a playlist, two cue sheets and two tracks");
        assert_eq!(plan.unsupported[0].platform, "Arcade");
        assert_eq!(plan.folders_to_create, 5, "roms/fds, nes, snes, psx and psx/Saga");
        let saga = game(&plan, "Saga");
        assert!(saga.destination.replace('\\', "/").ends_with("RetroBat/roms/psx/Saga/Saga.m3u"), "{}", saga.destination);
        assert_eq!(saga.files, 5);
        let capacity = &plan.capacity;
        assert_eq!(capacity.fits, Some(true));
        assert_eq!(capacity.reserve_bytes, (1i64 << 40) / 100 * 2, "2% of a 1 TiB drive is more than 1 GiB");
        assert_eq!(capacity.required_bytes, 8 * 4096 + 5 * 4096 + 4096, "files and folders round up to clusters, plus one staging file");
        assert_eq!(plan.fingerprint.len(), 16);
        assert_eq!(snapshot(&f.root), before, "planning must not create or change anything in the installation");

        let again = super::plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(again.fingerprint, plan.fingerprint, "same state, same fingerprint");
    }

    #[test]
    fn arcade_games_are_planned_once_an_emulator_is_chosen() {
        let f = fixture(&[("arcade/sf2.zip", b"sf2"), ("arcade/kinst.chd", b"chd")]);
        let before = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(before.selected_games, 0);
        assert!(before.unsupported.iter().any(|u| u.platform == "Arcade" && u.reason.contains("Choose MAME")));

        installations::set_arcade_system(&f.catalog, &f.id, Some("fbneo")).unwrap();
        let after = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(after.games.copy, 1);
        assert!(game(&after, "sf2").destination.replace('\\', "/").ends_with("RetroBat/roms/fbneo/sf2.zip"));
        assert!(after.unsupported.iter().any(|u| u.reason.contains("CHD")), "CHDs are not placed yet");
        assert_ne!(after.fingerprint, before.fingerprint, "changing the choice changes the plan");
    }

    #[test]
    fn a_subset_of_systems_can_be_chosen() {
        let f = fixture(&[("nes/Kid.nes", b"nes"), ("snes/Quest.sfc", b"snes!"), ("snes/Other.sfc", b"snes2")]);
        let plan = plan(&f.catalog, &f.id, Some(&["snes".to_string()]), &connected, &roomy).unwrap();
        let chosen: Vec<(&str, bool)> = plan.systems.iter().map(|s| (s.system_folder.as_str(), s.selected)).collect();
        assert_eq!(chosen, [("nes", false), ("snes", true)], "unselected systems are still offered");
        assert_eq!(plan.selected_games, 2);
        assert_eq!(plan.games.copy, 2);
        assert!(plan.sample.iter().all(|g| g.system_folder == "snes"));

        let none = super::plan(&f.catalog, &f.id, Some(&[]), &connected, &roomy).unwrap();
        assert_eq!(none.selected_games, 0);
        assert_eq!(none.capacity.fits, Some(true), "an empty plan always fits");
    }

    #[test]
    fn existing_files_are_kept_or_skipped() {
        let f = fixture(&[("nes/Same.nes", b"same"), ("nes/Diff.nes", b"mine"), ("nes/New.nes", b"new")]);
        write(&f.root, "roms/nes/Same.nes", b"SAME");
        write(&f.root, "roms/nes/Diff.nes", b"theirs, longer");
        let before = snapshot(&f.root);
        let plan = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.games, ActionCounts { copy: 1, already_there: 1, conflict: 1, ..ActionCounts::default() });
        let diff = game(&plan, "Diff");
        assert_eq!(diff.action, Action::Conflict);
        assert!(diff.reason.as_deref().unwrap().contains("will be kept"));
        assert_eq!(diff.bytes_to_copy, 0, "a conflicting game isn't copied at all");
        assert_eq!(plan.files_to_copy, 1);
        assert_eq!(snapshot(&f.root), before);
    }

    #[test]
    fn a_scanned_installation_is_already_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let root = retrobat(dir.path());
        write(&root, "roms/nes/Kid.nes", b"nes");
        write(dir.path(), "library/nes/Kid.nes", b"nes");
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root.join("roms"), &unknown).unwrap();
        scanner::scan(&mut c, &dir.path().join("library"), &unknown).unwrap();
        let id = installations::add(&c, &root, "retrobat", &unknown).unwrap();
        let plan = plan(&c, &id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.games, ActionCounts { in_place: 1, duplicate: 1, ..ActionCounts::default() }, "the copy in roms wins; the library copy is a duplicate");
        assert_eq!(plan.files_to_copy, 0);
    }

    #[test]
    fn duplicates_are_copied_once_and_clashes_are_blocked() {
        let f = fixture(&[
            ("a/nes/Twin.nes", b"twin"),
            ("b/nes/Twin.nes", b"twin"),
            ("a/nes/Clash.nes", b"one"),
            ("b/nes/CLASH.nes", b"other one"),
        ]);
        let plan = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.games.copy, 1);
        assert_eq!(plan.games.duplicate, 1);
        assert_eq!(plan.games.blocked, 2, "names that differ only in case, with different contents");
        assert!(plan.attention.iter().all(|g| g.reason.as_deref().unwrap().contains("same name")));
        assert_eq!(plan.files_to_copy, 1);
    }

    #[test]
    fn fat32_rejects_files_over_4_gib_and_small_drives_dont_fit() {
        let f = fixture(&[("nes/Kid.nes", &[0u8; 5000])]);
        let fat32 = |_: &Path| Some(VolumeSpace { free_bytes: 1 << 40, total_bytes: 1 << 40, filesystem: Some("FAT32".into()), cluster_bytes: 4096 });
        // Pretend the game is 5 GB without writing 5 GB.
        f.catalog.execute("UPDATE game_files SET size_bytes = 5000000000", []).unwrap();
        let plan = plan(&f.catalog, &f.id, None, &connected, &fat32).unwrap();
        assert_eq!(plan.games.blocked, 1);
        assert!(plan.attention[0].reason.as_deref().unwrap().contains("larger than this drive's filesystem allows"));
        assert_eq!(plan.capacity.max_file_bytes, Some(4_294_967_295));
        f.catalog.execute("UPDATE game_files SET size_bytes = 5000", []).unwrap();

        // 1.5 GiB free on a 10 GiB drive: the 1 GiB reserve leaves too little for 600 MB.
        f.catalog.execute("UPDATE game_files SET size_bytes = 600000000", []).unwrap();
        let tight = |_: &Path| Some(VolumeSpace { free_bytes: 3 << 29, total_bytes: 10 << 30, filesystem: Some("exFAT".into()), cluster_bytes: 32768 });
        let plan = super::plan(&f.catalog, &f.id, None, &connected, &tight).unwrap();
        assert_eq!(plan.capacity.reserve_bytes, 1 << 30);
        assert_eq!(plan.capacity.fits, Some(false));
        let unmeasured = super::plan(&f.catalog, &f.id, None, &connected, &|_: &Path| None).unwrap();
        assert_eq!(unmeasured.capacity.fits, None, "unknown free space is never reported as fitting");
        drop(f.library);
    }

    #[test]
    fn links_and_folders_in_the_way_are_blocked() {
        let f = fixture(&[("nes/Kid.nes", b"nes"), ("snes/Quest.sfc", b"snes")]);
        fs::create_dir_all(f.root.join("roms/nes/Kid.nes")).unwrap();
        write(&f.root, "roms/snes", b"a file, not a folder");
        let plan = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.games.blocked, 2);
        assert!(game(&plan, "Kid").reason.as_deref().unwrap().contains("folder with this name"));
        assert!(game(&plan, "Quest").reason.as_deref().unwrap().contains("a file where a folder is needed"));
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_never_written_through() {
        let f = fixture(&[("nes/Kid.nes", b"nes")]);
        let elsewhere = f.root.parent().unwrap().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(f.root.join("roms").join("nes"))
            .arg(&elsewhere)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let plan = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.games.blocked, 1);
        assert!(game(&plan, "Kid").reason.as_deref().unwrap().contains("link to another location"));
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        assert!(safe_relative("roms/nes/Kid.nes"));
        for bad in ["", "/roms/x", "\\roms\\x", "roms/../x", "roms/./x", "C:/x", "roms//x", "roms/nes/"] {
            assert!(!safe_relative(bad), "{bad}");
        }
    }

    #[test]
    fn missing_files_and_unplugged_drives_are_left_out() {
        let f = fixture(&[("nes/Kid.nes", b"nes"), ("snes/Quest.sfc", b"snes")]);
        let mut c = f.catalog;
        fs::remove_file(f.library.join("snes/Quest.sfc")).unwrap();
        scanner::scan(&mut c, &f.library, &unknown).unwrap();
        let plan = plan(&c, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(plan.unavailable_games, 1);
        assert_eq!(plan.games.copy, 1);
        let offline = super::plan(&c, &f.id, None, &|_, _| Presence::NotConnected, &roomy);
        assert!(offline.unwrap_err().contains("isn't connected"));
    }
}
