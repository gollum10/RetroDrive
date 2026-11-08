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
use serde::{Deserialize, Serialize};
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
    /// Not copied because the user chose another version or chose to skip it.
    Skipped,
    /// The user removed this game from the drive and chose to keep it off (Sync, ADR-043).
    KeptOff,
    /// RetroDrive copied this game before but it's gone from the drive: not copied until
    /// the user chooses "copy it again" or "keep it off" (ADR-043).
    Removed,
    /// The same game is already on the drive, or another version is being copied, under
    /// another name (e.g. `Game(J).zip` next to `Game (Japan).zip`); not copied unless
    /// the user chooses "copy anyway" (ADR-042).
    SameGame,
    Copy,
    /// The existing file is moved to RetroDrive's backup folder, then replaced (step 5b).
    Replace,
    /// A different file with this name is already there; the existing file is kept.
    Conflict,
    /// The file can't be placed (filesystem limit, unsafe path, name clash...).
    Blocked,
}

/// What the user chose for a conflict or a clash (step 5b, ADR-028). Keyed in
/// `Choices` by the destination's path inside the installation, lower-case.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Choice {
    /// Default: keep the existing file / copy neither clashing version.
    Keep,
    /// For a clash: copy this library file; skip the other versions.
    Use { file_id: String },
    /// Copy under the next free name, e.g. `Game (2).zip`.
    KeepBoth,
    /// For a conflict: move the existing file to the backup folder, then copy.
    Replace,
}

pub type Choices = HashMap<String, Choice>;

/// Systems whose file names are romset names and must never be renamed.
const EXACT_NAME_PLATFORMS: &[&str] = &["Arcade", "Neo Geo", "CPS1", "CPS2", "CPS3", "Naomi", "Atomiswave"];

/// The choice offered for an item in the plan, and which options apply.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct ChoiceInfo {
    pub key: String,
    /// `conflict` (a different file is on the drive), `clash` (library versions collide) or
    /// `same_game` (the game is on the drive or in the plan under another name),
    /// `removed` (copied before, gone from the drive) or `kept_off` (the user keeps it off).
    pub situation: &'static str,
    /// Any of `keep`, `use`, `keep_both`, `replace`.
    pub options: Vec<&'static str>,
    /// The choice currently applied.
    pub current: &'static str,
    /// For clashes: the library file this row is (for "use this version").
    pub file_id: String,
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
    /// Present when the user can choose what happens (conflicts and clashes).
    pub choice: Option<ChoiceInfo>,
}

#[derive(Debug, Serialize)]
pub struct SystemChoice {
    pub system_folder: String,
    pub platforms: Vec<String>,
    pub games: i64,
    pub bytes: i64,
    pub selected: bool,
}

/// Games of one file type a system doesn't list (so its menu won't show them).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct FormatWarning {
    pub system_folder: String,
    pub extension: String,
    pub games: i64,
    /// The file types the system lists, e.g. `.nes .zip .7z`.
    pub accepted: String,
}

/// RDS-018: games whose file type the system doesn't list (RetroBat's es_systems.cfg).
fn format_warnings(adapter_id: &str, root: &Path, items: &[Item]) -> Vec<FormatWarning> {
    if adapter_id != "retrobat" {
        return Vec::new();
    }
    let Some(lists) = crate::customizer::system_extensions(root) else { return Vec::new() };
    let mut counts: BTreeMap<(String, String), i64> = BTreeMap::new();
    for item in items {
        let Some(list) = lists.get(&item.system_folder) else { continue };
        let Some(first) = item.files.first() else { continue };
        let file = first.relative.rsplit('/').next().unwrap_or(&first.relative);
        let Some((_, ext)) = file.rsplit_once('.') else { continue };
        let ext = format!(".{}", ext.to_lowercase());
        if !list.contains(&ext) {
            *counts.entry((item.system_folder.clone(), ext)).or_default() += 1;
        }
    }
    counts.into_iter().map(|((system_folder, extension), games)| FormatWarning { accepted: lists[&system_folder].join(" "), system_folder, extension, games }).collect()
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
    pub replace: i64,
    pub already_there: i64,
    pub in_place: i64,
    pub duplicate: i64,
    /// Already on the drive (or in the plan) under another name; not copied.
    pub same_game: i64,
    /// Copied before, now gone from the drive; waiting for the user's choice.
    pub removed: i64,
    /// Removed from the drive and kept off it (the user's choice).
    pub kept_off: i64,
    pub conflict: i64,
    pub blocked: i64,
}

impl ActionCounts {
    fn add(&mut self, action: Action) {
        match action {
            Action::Copy => self.copy += 1,
            Action::Replace => self.replace += 1,
            Action::AlreadyThere => self.already_there += 1,
            Action::InPlace => self.in_place += 1,
            Action::Duplicate | Action::Skipped => self.duplicate += 1,
            Action::SameGame => self.same_game += 1,
            Action::Removed => self.removed += 1,
            Action::KeptOff => self.kept_off += 1,
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
    /// False for preview-only adapters (Batocera): the plan is shown, but the copier refuses.
    pub copy_supported: bool,
    /// Where system folders sit inside the installation, as shown: `roms\\` (RetroBat,
    /// Batocera, a RetroPie USB folder) or empty (RetroPie's roms share).
    pub systems_prefix: String,
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
    /// Games not copied because they're on the drive (or in the plan) under another name; each can be copied anyway.
    pub same_game: Vec<PlanGame>,
    /// Games RetroDrive copied before that are no longer on the drive (asked about), and games kept off it.
    pub removed: Vec<PlanGame>,
    pub sample: Vec<PlanGame>,
    /// Games the system won't show because it doesn't list their file type (RDS-018).
    pub format_warnings: Vec<FormatWarning>,
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
    pub choice: Option<ChoiceInfo>,
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
#[cfg(test)]
pub fn plan(connection: &Connection, installation_id: &str, systems: Option<&[String]>, presence: PresenceFn, space: SpaceFn) -> Result<DeploymentPlan, String> {
    plan_with(connection, installation_id, systems, None, &Choices::new(), presence, space)
}

/// The plan with the user's conflict and clash choices applied.
pub fn plan_with(connection: &Connection, installation_id: &str, systems: Option<&[String]>, collection: Option<&str>, choices: &Choices, presence: PresenceFn, space: SpaceFn) -> Result<DeploymentPlan, String> {
    build(connection, installation_id, systems, None, collection, choices, presence, space).map(|(plan, _)| plan)
}

/// The Sync plan (ADR-043): every platform except the ones the user left out of Sync,
/// with the user's choices for games removed from the drive. Reads only.
pub fn plan_sync(connection: &Connection, installation_id: &str, choices: &Choices, presence: PresenceFn, space: SpaceFn) -> Result<DeploymentPlan, String> {
    let scope = sync_scope(connection, installation_id, presence)?;
    if scope.is_empty() {
        return Err(NOTHING_SYNCED.into());
    }
    build(connection, installation_id, None, Some(&scope), None, choices, presence, space).map(|(plan, _)| plan)
}

pub const NOTHING_SYNCED: &str = "Nothing is synced to this drive yet. Use Plan deployment to choose what goes on it; once copied, Sync keeps those platforms up to date.";

/// What Sync keeps up to date (ADR-045): the saved scope, or, before one is saved, the
/// platforms RetroDrive copied to this installation that still have a copied game on
/// the drive (so a platform whose files were all moved off isn't brought back).
pub fn sync_scope(connection: &Connection, installation_id: &str, presence: PresenceFn) -> Result<installations::SyncScope, String> {
    if let Some(scope) = installations::saved_sync_scope(connection, installation_id)? {
        return Ok(scope);
    }
    let install = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT i.destination_path FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
             WHERE d.installation_id = ?1 AND i.state IN ('verified', 'identical', 'replaced')",
        )
        .map_err(|e| e.to_string())?;
    let paths: Vec<String> = statement.query_map([installation_id], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    let mut systems: BTreeSet<String> = BTreeSet::new();
    let connected = install.presence == Presence::Connected;
    for path in paths {
        let Some(folder) = system_folder_of(&path) else { continue };
        if !systems.contains(&folder) && connected && Path::new(&path).exists() {
            systems.insert(folder);
        }
    }
    Ok(installations::SyncScope { systems: systems.into_iter().collect(), collections: Vec::new() })
}

/// The system folder of a destination path: the folder after `roms` (with something inside it).
fn system_folder_of(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split(['\\', '/']).collect();
    let at = parts.iter().position(|p| p.eq_ignore_ascii_case("roms"))?;
    parts.get(at + 2).and(parts.get(at + 1)).map(|s| s.to_string())
}

/// After a copy from a plan: the platforms (or collection) it covered are kept up to
/// date by Sync from now on (ADR-045). Catalog only.
pub fn add_to_sync_scope(connection: &Connection, installation_id: &str, systems: &[String], collection: Option<&str>, presence: PresenceFn) -> Result<(), String> {
    let mut scope = sync_scope(connection, installation_id, presence)?;
    match collection.filter(|c| !c.is_empty()) {
        Some(c) => scope.collections.push(c.to_string()),
        None => scope.systems.extend(systems.iter().cloned()),
    }
    installations::set_sync_scope(connection, installation_id, &scope)
}

/// The plan plus every selected item, for the copier.
/// `collection` limits the plan to the games in that collection (step 7); `scope` (Sync)
/// limits it to the synced platforms plus the games in the synced collections.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    connection: &Connection,
    installation_id: &str,
    systems: Option<&[String]>,
    scope: Option<&installations::SyncScope>,
    collection: Option<&str>,
    user_choices: &Choices,
    presence: PresenceFn,
    space: SpaceFn,
) -> Result<(DeploymentPlan, Vec<Item>), String> {
    let install = installations::located(connection, presence, Some(installation_id))?.into_iter().next().ok_or("Installation not found")?;
    if install.presence != Presence::Connected {
        return Err("The installation's drive isn't connected, so a plan can't be made.".into());
    }
    let adapter = adapters::find(&install.adapter_id).ok_or_else(|| format!("Unknown platform adapter: {}", install.adapter_id))?;
    let install_root = install.root.display().to_string();
    let mut gathered = gather(connection, adapter.as_ref(), &install_root, install.options, presence)?;
    if let Some(collection) = collection.filter(|c| !c.is_empty()) {
        let members = crate::collections::member_file_ids(connection, collection, presence)?;
        gathered.items.retain(|item| item.files.first().is_some_and(|f| members.contains(&f.game_file_id)));
    }
    if let Some(scope) = scope {
        let mut members: std::collections::HashSet<String> = std::collections::HashSet::new();
        for c in &scope.collections {
            members.extend(crate::collections::member_file_ids(connection, c, presence)?);
        }
        gathered.items.retain(|item| scope.systems.contains(&item.system_folder) || item.files.first().is_some_and(|f| members.contains(&f.game_file_id)));
    }

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
    check_files(&mut items, &install.root, max_file, user_choices);
    mark_removed(connection, &install.id, &mut items, user_choices)?;
    same_games(&mut items, user_choices);

    // Totals.
    let cluster = measured.as_ref().map(|s| s.cluster_bytes.max(1)).unwrap_or(4096);
    let mut games = ActionCounts::default();
    let (mut files_to_copy, mut bytes_to_copy, mut allocated, mut largest, mut long_paths) = (0i64, 0i64, 0u64, 0u64, 0i64);
    let mut new_folders: BTreeSet<String> = BTreeSet::new();
    let mut attention = Vec::new();
    let mut same_game = Vec::new();
    let mut removed = Vec::new();
    let mut sample = Vec::new();
    let mut fingerprint = Fnv::new();
    fingerprint.write(&install.adapter_id);
    fingerprint.write(&adapters::CONTRACT_VERSION.to_string());
    fingerprint.write(&install_root);
    fingerprint.write(collection.unwrap_or(""));
    if let Some(scope) = scope {
        fingerprint.write(&format!("sync:{}|{}", scope.systems.join(","), scope.collections.join(",")));
    }
    for item in &items {
        let action = item.action();
        games.add(action);
        let mut game_copy_bytes = 0i64;
        for file in &item.files {
            fingerprint.write(&file.source.display().to_string());
            fingerprint.write(&file.destination.display().to_string());
            fingerprint.write(&file.size.to_string());
            fingerprint.write(&format!("{:?}", if matches!(action, Action::Copy | Action::Replace) { file.action } else { action }));
            if file.destination.as_os_str().len() > LONG_PATH {
                long_paths += 1;
            }
            if matches!(action, Action::Copy | Action::Replace) && matches!(file.action, Action::Copy | Action::Replace) {
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
            choice: item.files.iter().find_map(|f| f.choice.clone()),
        };
        // Items with a choice stay listed, so the choice can be seen and changed.
        if matches!(action, Action::Removed | Action::KeptOff) {
            if removed.len() < ATTENTION_LIMIT {
                removed.push(game);
            }
        } else if action == Action::SameGame {
            if same_game.len() < ATTENTION_LIMIT {
                same_game.push(game);
            }
        } else if matches!(action, Action::Conflict | Action::Blocked) || game.choice.as_ref().is_some_and(|c| c.current != "keep") {
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

    let format_warnings = format_warnings(&install.adapter_id, &install.root, &items);
    let plan = DeploymentPlan {
        installation_id: install.id,
        adapter_id: install.adapter_id,
        adapter_name: adapter.display_name().to_string(),
        adapter_contract: adapters::CONTRACT_VERSION,
        copy_supported: adapter.capabilities().copy_games,
        systems_prefix: if ((adapter.id() == "retropie" || adapter.id() == "rocknix") && !install.options.has_roms_folder) || adapter.id() == "esde" || adapter.id() == "lakka" { String::new() } else { "roms\\".into() },
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
        same_game,
        removed,
        sample,
        format_warnings,
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
/// `Game.zip` -> `Game (2).zip` (the number goes before the extension).
fn numbered(relative: &str, n: u32) -> String {
    let (folder, name) = relative.rsplit_once('/').map(|(d, f)| (format!("{d}/"), f)).unwrap_or((String::new(), relative));
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{folder}{stem} ({n}).{ext}"),
        _ => format!("{folder}{name} ({n})"),
    }
}

fn rename_to(file: &mut ItemFile, install_root: &Path, relative: String) {
    file.destination = paths::join(&install_root.display().to_string(), &relative);
    file.relative = relative;
}

fn check_files(items: &mut [Item], install_root: &Path, max_file: Option<u64>, choices: &Choices) {
    // Several library files aiming at one destination name: identical sizes are
    // duplicates (copy one); different sizes are a clash the user resolves (default:
    // copy neither). A source that already *is* the destination wins.
    let mut targets: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        for (j, file) in item.files.iter().enumerate() {
            targets.entry(path_key(&file.destination)).or_default().push((i, j));
        }
    }
    let mut renamed: Vec<(usize, usize)> = Vec::new();
    let mut targets: Vec<(String, Vec<(usize, usize)>)> = targets.into_iter().collect();
    targets.sort();
    for (key, mut users) in targets {
        let distinct: BTreeSet<String> = users.iter().map(|&(i, j)| path_key(&items[i].files[j].source)).collect();
        if distinct.len() < 2 {
            continue;
        }
        let sizes: BTreeSet<i64> = users.iter().map(|&(i, j)| items[i].files[j].size).collect();
        users.sort_by_key(|&(i, j)| (path_key(&items[i].files[j].source) != key, path_key(&items[i].files[j].source)));
        if sizes.len() == 1 {
            for &(i, j) in &users[1..] {
                let file = &mut items[i].files[j];
                file.action = Action::Duplicate;
                file.reason = Some("Another copy of this file in your library is already in the plan.".into());
            }
            continue;
        }
        let choice_key = items[users[0].0].files[users[0].1].relative.to_lowercase();
        let renamable = users.iter().all(|&(i, _)| items[i].files.len() == 1 && !EXACT_NAME_PLATFORMS.contains(&items[i].platform.as_str()));
        let mut options = vec!["keep", "use"];
        if renamable {
            options.push("keep_both");
        }
        let chosen = choices.get(&choice_key).cloned().unwrap_or(Choice::Keep);
        let chosen = match chosen {
            Choice::KeepBoth if !renamable => Choice::Keep,
            Choice::Replace => Choice::Keep,
            Choice::Use { ref file_id } if !users.iter().any(|&(i, j)| &items[i].files[j].game_file_id == file_id) => Choice::Keep,
            other => other,
        };
        let current = match chosen {
            Choice::Keep | Choice::Replace => "keep",
            Choice::Use { .. } => "use",
            Choice::KeepBoth => "keep_both",
        };
        for (n, &(i, j)) in users.iter().enumerate() {
            let file = &mut items[i].files[j];
            file.choice = Some(ChoiceInfo { key: choice_key.clone(), situation: "clash", options: options.clone(), current, file_id: file.game_file_id.clone() });
            match &chosen {
                Choice::Keep | Choice::Replace => {
                    file.action = Action::Blocked;
                    file.reason = Some("Two different files in your library would be copied to the same name. Choose which to copy, or keep both.".into());
                }
                Choice::Use { file_id } if &file.game_file_id == file_id => {
                    file.reason = Some("You chose this version.".into());
                }
                Choice::Use { .. } => {
                    file.action = Action::Skipped;
                    file.reason = Some("You chose another version of this game.".into());
                }
                Choice::KeepBoth if n == 0 => file.reason = Some("Kept under its own name (you chose to keep both).".into()),
                Choice::KeepBoth => renamed.push((i, j)),
            }
        }
    }

    // Destinations planned so far (renamed copies must not collide with any of them).
    let mut planned: BTreeSet<String> = items.iter().flat_map(|it| it.files.iter().map(|f| path_key(&f.destination))).collect();
    let free_name = |relative: &str, planned: &BTreeSet<String>| -> String {
        (2..1000)
            .map(|n| numbered(relative, n))
            .find(|candidate| {
                let path = paths::join(&install_root.display().to_string(), candidate);
                !planned.contains(&path_key(&path)) && std::fs::symlink_metadata(&path).is_err()
            })
            .unwrap_or_else(|| numbered(relative, 1000))
    };
    for (i, j) in renamed {
        let relative = items[i].files[j].relative.clone();
        let new_relative = free_name(&relative, &planned);
        let file = &mut items[i].files[j];
        rename_to(file, install_root, new_relative);
        planned.insert(path_key(&file.destination));
        file.reason = Some(format!("Copied as \"{}\" (you chose to keep both).", file.destination.file_name().and_then(|n| n.to_str()).unwrap_or_default()));
    }

    let mut folders: HashMap<PathBuf, Ancestor> = HashMap::new();
    for item in items.iter_mut() {
        let single = item.files.len() == 1;
        let exact_names = EXACT_NAME_PLATFORMS.contains(&item.platform.as_str());
        for file in item.files.iter_mut() {
            if file.action != Action::Copy {
                continue;
            }
            let (action, reason) = check_file(file, install_root, max_file, &mut folders);
            if action != Action::Conflict {
                file.action = action;
                if reason.is_some() || file.reason.is_none() {
                    file.reason = reason;
                }
                continue;
            }
            // A different file is already there: keep it (default), keep both, or replace.
            let key = file.relative.to_lowercase();
            let mut options = vec!["keep"];
            if single && !exact_names {
                options.push("keep_both");
            }
            if single {
                options.push("replace");
            }
            let chosen = match choices.get(&key) {
                Some(Choice::KeepBoth) if options.contains(&"keep_both") => "keep_both",
                Some(Choice::Replace) if options.contains(&"replace") => "replace",
                _ => "keep",
            };
            file.choice = Some(ChoiceInfo { key, situation: "conflict", options, current: chosen, file_id: file.game_file_id.clone() });
            match chosen {
                "keep_both" => {
                    let new_relative = free_name(&file.relative, &planned);
                    rename_to(file, install_root, new_relative);
                    planned.insert(path_key(&file.destination));
                    let (action, reason) = check_file(file, install_root, max_file, &mut folders);
                    file.action = action;
                    file.reason = reason.or_else(|| Some(format!("Copied as \"{}\"; the existing file is kept (you chose to keep both).", file.destination.file_name().and_then(|n| n.to_str()).unwrap_or_default())));
                }
                "replace" => {
                    file.action = Action::Replace;
                    file.reason = Some("The existing file will be moved to RetroDrive's backup folder on this drive, then replaced (you chose replace).".into());
                }
                _ => {
                    file.action = Action::Conflict;
                    file.reason = reason;
                }
            }
        }
    }
}

/// Games RetroDrive copied to this installation before (verified, identical or replaced)
/// that are no longer on the drive: the user is asked instead of copying them back
/// silently, and can keep them off for good (ADR-043). Reads the catalog only.
fn mark_removed(connection: &Connection, installation_id: &str, items: &mut [Item], choices: &Choices) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT i.destination_path FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
             WHERE d.installation_id = ?1 AND i.state IN ('verified', 'identical', 'replaced')",
        )
        .map_err(|e| e.to_string())?;
    let copied: BTreeSet<String> = statement
        .query_map([installation_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|r| r.map(|p| path_key(Path::new(&p))))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if copied.is_empty() {
        return Ok(());
    }
    let kept_off = installations::kept_off(connection, installation_id)?;
    for item in items.iter_mut() {
        // A game RetroDrive copied whose file on the drive now differs: "changed on the
        // drive"; left as it is (default) or restored from the library (ADR-045).
        if let Some(file) = item.files.iter_mut().find(|f| f.choice.as_ref().is_some_and(|c| c.situation == "conflict") && copied.contains(&path_key(&f.destination))) {
            if let Some(choice) = file.choice.as_mut() {
                choice.situation = "changed";
                choice.options.retain(|o| *o != "keep_both");
            }
            if file.action == Action::Conflict {
                file.reason = Some("This file changed on the drive since RetroDrive copied it. It's left as it is unless you choose to restore your library's version (the drive's file is backed up first).".into());
            }
            continue;
        }
        let primary = &item.files[0];
        if primary.action != Action::Copy || primary.choice.is_some() {
            continue;
        }
        let key = path_key(&primary.destination);
        if !copied.contains(&key) {
            continue;
        }
        let choice_key = primary.relative.to_lowercase();
        let file_id = primary.game_file_id.clone();
        let (action, situation, reason, current) = if kept_off.contains(&key) {
            (Action::KeptOff, "kept_off", "You removed this game from the drive and chose to keep it off.", "keep")
        } else if matches!(choices.get(&choice_key), Some(Choice::KeepBoth)) {
            (Action::Copy, "removed", "You removed this game from the drive; it's copied again (you chose).", "keep_both")
        } else {
            (Action::Removed, "removed", "RetroDrive copied this game before, but it's no longer on the drive. Choose whether to copy it again or keep it off.", "keep")
        };
        let options = if situation == "removed" { vec!["keep", "keep_both", "keep_off"] } else { vec!["keep", "keep_both"] };
        for (n, file) in item.files.iter_mut().enumerate() {
            if action != Action::Copy {
                file.action = action;
            }
            if n == 0 {
                file.reason = Some(reason.to_string());
                file.choice = Some(ChoiceInfo { key: choice_key.clone(), situation, options: options.clone(), current, file_id: file_id.clone() });
            }
        }
    }
    Ok(())
}

/// A game's identity from its file name, ignoring tags and punctuation, so
/// `10-Yard_Fight(J).zip` and `10-Yard Fight (Japan).zip` match. Disc, disk, side, tape
/// and part tags are kept (they are different files of one game). `None` when nothing is left.
fn game_key(file_name: &str) -> Option<String> {
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name);
    let (mut base, mut kept, mut group, mut depth) = (String::new(), String::new(), String::new(), 0);
    for c in stem.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    group.clear();
                }
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                let g = group.trim().to_lowercase();
                if depth == 0 && ["disc", "disk", "side", "cd", "tape", "part"].iter().any(|w| g.starts_with(w)) {
                    kept.push_str(&g);
                    kept.push(' ');
                }
            }
            _ if depth > 0 => group.push(c),
            _ => base.push(c),
        }
    }
    // Dots and underscores often stand for spaces (`Addams.Family.The(U)`).
    let mut base = base.replace(['_', '.'], " ").trim().trim_end_matches(',').trim().to_lowercase();
    // "Legend of Zelda, The" and "Addams Family The" are "The Legend of Zelda", "The Addams Family".
    for article in ["the", "a", "an"] {
        if let Some(rest) = base.strip_suffix(article).filter(|r| r.ends_with([' ', ','])) {
            base = format!("{article} {}", rest.trim_end_matches([' ', ',']));
            break;
        }
    }
    let key: String = format!("{base} {kept}").chars().filter(|c| c.is_alphanumeric()).collect();
    (!key.is_empty()).then_some(key)
}

/// Which version of a game to copy when the library has several: good dumps over hacks,
/// betas and bad dumps; then USA/World, Europe, Japan; then the shorter name.
fn preference(file_name: &str) -> (u8, u8, usize, String) {
    let lower = file_name.to_lowercase();
    let mut tags = String::new();
    let mut depth = 0;
    for c in lower.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => {
                depth -= 1;
                tags.push(' ');
            }
            _ if depth > 0 => tags.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = tags.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let bad = lower.contains("[b") || lower.contains("[h") || lower.contains("[t") || lower.contains("[o") || lower.contains("[p") || lower.contains("[f") || has(&["hack", "beta", "proto", "prototype", "pirate", "sample", "demo", "bootleg"]);
    let region = if has(&["usa", "u", "world", "w", "ju", "ue"]) {
        0
    } else if has(&["europe", "e", "eu"]) {
        1
    } else if has(&["japan", "j"]) {
        2
    } else {
        3
    };
    (bad as u8, region, file_name.len(), lower)
}

/// Marks a file as "same game" (not copied) unless the user chose to copy it anyway.
/// Returns true when it stays a copy.
fn mark_same_game(file: &mut ItemFile, choices: &Choices, reason: String) -> bool {
    let key = file.relative.to_lowercase();
    let copy_anyway = matches!(choices.get(&key), Some(Choice::KeepBoth));
    file.choice = Some(ChoiceInfo { key, situation: "same_game", options: vec!["keep", "keep_both"], current: if copy_anyway { "keep_both" } else { "keep" }, file_id: file.game_file_id.clone() });
    if copy_anyway {
        file.reason = Some(format!("{reason} Copied anyway (you chose)."));
    } else {
        file.action = Action::SameGame;
        file.reason = Some(reason);
    }
    copy_anyway
}

/// Games already on the drive, or being copied in another version, under a different
/// name are not copied by default (owner report 2026-11-07: a second naming style of
/// the same NES games was being added). Only reads folder listings.
fn same_games(items: &mut [Item], choices: &Choices) {
    let eligible = |item: &Item| item.files.len() == 1 && item.files[0].action == Action::Copy && item.files[0].choice.is_none() && !EXACT_NAME_PLATFORMS.contains(&item.platform.as_str()) && item.platform != crate::classify::BIOS;
    let name_of = |file: &ItemFile| file.destination.file_name().and_then(|n| n.to_str()).map(str::to_string);
    // 1. On the drive under another name.
    let mut on_drive: HashMap<PathBuf, HashMap<String, String>> = HashMap::new();
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    for (i, item) in items.iter_mut().enumerate() {
        if !eligible(item) {
            continue;
        }
        let file = &mut item.files[0];
        let (Some(name), Some(folder)) = (name_of(file), file.destination.parent().map(Path::to_path_buf)) else { continue };
        let Some(key) = game_key(&name) else { continue };
        let existing = on_drive.entry(folder.clone()).or_insert_with(|| {
            let mut keys = HashMap::new();
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                if entry.file_type().is_ok_and(|t| t.is_file()) {
                    if let Some(n) = entry.file_name().to_str() {
                        if let Some(k) = game_key(n) {
                            keys.entry(k).or_insert_with(|| n.to_string());
                        }
                    }
                }
            }
            keys
        });
        if let Some(other) = existing.get(&key).filter(|o| !o.eq_ignore_ascii_case(&name)).cloned() {
            if mark_same_game(file, choices, format!("This game is already on the drive as \"{other}\".")) {
                chosen.insert(i);
            }
        }
    }
    // 2. Several versions in the plan, none on the drive: copy the preferred one.
    let mut groups: BTreeMap<(String, String), Vec<(usize, String)>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        if chosen.contains(&i) || !eligible(item) {
            continue;
        }
        let file = &item.files[0];
        let (Some(name), Some(folder)) = (name_of(file), file.destination.parent()) else { continue };
        if let Some(key) = game_key(&name) {
            groups.entry((path_key(folder), key)).or_default().push((i, name));
        }
    }
    for (_, mut members) in groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by_key(|(_, name)| preference(name));
        let best = members[0].1.clone();
        for (i, _) in &members[1..] {
            mark_same_game(&mut items[*i].files[0], choices, format!("Another version of this game, \"{best}\", is being copied."));
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
        // BIOS files in a library `bios` folder keep their place below it (e.g. `dc/dc_boot.bin`).
        let bios_path = (platform == crate::classify::BIOS).then(|| {
            let parts: Vec<&str> = relative.split('/').collect();
            parts.iter().position(|p| p.eq_ignore_ascii_case("bios")).map(|at| parts[at + 1..].join("/")).filter(|p| !p.is_empty())
        }).flatten();
        let request = PlacementRequest { title: title.clone(), platform: platform.clone(), extension, files, options, bios_path };
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
                        choice: None,
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
    fn games_retrobat_wont_show_are_flagged_by_file_type() {
        let f = fixture(&[("nes/Kid.7z", b"kid"), ("nes/Zelda.nes", b"zelda"), ("nes/Metroid.7z", b"metroid"), ("snes/Mario.sfc", b"mario")]);
        let none = plan_with(&f.catalog, &f.id, None, None, &Choices::new(), &connected, &roomy).unwrap();
        assert!(none.format_warnings.is_empty(), "no es_systems.cfg: nothing to check against");
        write(&f.root, "emulationstation/.emulationstation/es_systems.cfg", b"<?xml version=\"1.0\"?>
<systemList>
  <system>
    <name>nes</name>
    <extension>.nes .NES .zip .ZIP</extension>
  </system>
  <system>
    <name>snes</name>
    <extension>.sfc .smc .zip .7z</extension>
  </system>
</systemList>
");
        let plan = plan_with(&f.catalog, &f.id, None, None, &Choices::new(), &connected, &roomy).unwrap();
        assert_eq!(plan.format_warnings, vec![FormatWarning { system_folder: "nes".into(), extension: ".7z".into(), games: 2, accepted: ".nes .zip".into() }]);
        let fingerprint_before = none.fingerprint;
        assert_eq!(plan.fingerprint, fingerprint_before, "a warning doesn't change what would be copied");
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
    fn the_same_game_under_another_name_is_not_copied_twice() {
        let f = fixture(&[
            ("nes/10-Yard_Fight(J).zip", b"goodnes"),
            ("nes/Adventures of Lolo (U) [!].zip", b"lolo-u"),
            ("more/nes/Adventures of Lolo (Japan).zip", b"lolo-japan"),
            ("more/nes/Adventures of Lolo (U) [h1].zip", b"lolo hack"),
            ("nes/Legend of Zelda, The (U).zip", b"zelda"),
            ("nes/Brand New (USA).zip", b"new"),
            ("fds/Saga (Japan) (Disk 1).fds", b"d1"),
            ("fds/Saga (Japan) (Disk 2).fds", b"d2"),
        ]);
        write(&f.root, "roms/nes/10-Yard Fight (Japan).zip", b"no-intro naming");
        write(&f.root, "roms/nes/The Legend of Zelda (USA).zip", b"zelda, other dump");
        let before = snapshot(&f.root);
        let p = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        assert_eq!(p.games.same_game, 4, "10-Yard Fight and Zelda are on the drive; two other Lolos");
        assert_eq!(p.games.copy, 4, "the preferred Lolo, Brand New, both disks of Saga");
        let reason = |t: &str| p.same_game.iter().find(|g| g.title.starts_with(t)).unwrap_or_else(|| panic!("{t}")).reason.clone().unwrap();
        assert!(reason("10-Yard").contains("already on the drive as \"10-Yard Fight (Japan).zip\""));
        assert!(reason("Legend of Zelda").contains("The Legend of Zelda (USA).zip"), "\", The\" names match");
        assert!(p.same_game.iter().filter(|g| g.title.contains("Lolo")).all(|g| g.reason.as_deref().unwrap().contains("\"Adventures of Lolo (U) [!].zip\", is being copied")), "USA good dump wins over Japan and a hack");
        assert!(p.same_game.iter().all(|g| g.choice.as_ref().is_some_and(|c| c.situation == "same_game" && c.options == ["keep", "keep_both"])));
        assert_eq!(snapshot(&f.root), before, "planning reads only");

        // "Copy anyway" for one of them.
        let key = p.same_game.iter().find(|g| g.title.starts_with("10-Yard")).unwrap().choice.as_ref().unwrap().key.clone();
        let chosen = plan_choosing(&f, &Choices::from([(key, Choice::KeepBoth)]));
        assert_eq!((chosen.games.same_game, chosen.games.copy), (3, 5));
        assert!(chosen.attention.iter().any(|g| g.title.starts_with("10-Yard") && g.reason.as_deref().unwrap().contains("Copied anyway")));
    }

    #[test]
    fn sync_keeps_only_what_was_synced_and_asks_about_removed_games() {
        let f = fixture(&[("nes/Kid.nes", b"nes"), ("nes/Gone.nes", b"gone"), ("snes/Quest.sfc", b"snes!")]);
        // Nothing copied yet: nothing to sync.
        assert_eq!(plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap_err(), NOTHING_SYNCED);
        // A plan copy of NES records it; Sync then covers NES only, never SNES.
        add_to_sync_scope(&f.catalog, &f.id, &["nes".to_string()], None, &connected).unwrap();
        let p = plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap();
        assert_eq!(p.systems.iter().map(|s| (s.system_folder.as_str(), s.selected)).collect::<Vec<_>>(), [("nes", true)]);
        assert_eq!(p.games.copy, 2);
        // Pretend both NES games were copied before, then Gone was deleted from the drive.
        write(&f.root, "roms/nes/Kid.nes", b"nes");
        f.catalog.execute("INSERT INTO deployments (id, installation_id, adapter_id, adapter_contract, destination_root, plan_fingerprint, status) VALUES ('d1', ?1, 'retrobat', 1, 'x', 'f', 'completed')", [&f.id]).unwrap();
        // Destinations exactly as the plan writes them (CI's temp folder has short 8.3 names).
        for g in p.sample.iter().filter(|g| g.system_folder == "nes") {
            f.catalog.execute("INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, size_bytes, planned_action, state) VALUES ('d1', 0, ?1, 'src', ?2, 3, 'copy', 'verified')", [&g.title, &g.destination]).unwrap();
        }
        let before = snapshot(&f.root);
        let p = plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap();
        assert_eq!((p.games.removed, p.games.copy, p.games.already_there), (1, 0, 1), "Gone isn't copied back silently");
        let gone = &p.removed[0];
        assert_eq!((gone.title.as_str(), gone.action), ("Gone", Action::Removed));
        assert_eq!(gone.choice.as_ref().unwrap().options, ["keep", "keep_both", "keep_off"]);
        // Copy it again...
        let key = gone.choice.as_ref().unwrap().key.clone();
        let again = plan_sync(&f.catalog, &f.id, &Choices::from([(key, Choice::KeepBoth)]), &connected, &roomy).unwrap();
        assert_eq!((again.games.removed, again.games.copy), (0, 1));
        // ...or keep it off, remembered.
        installations::set_kept_off(&f.catalog, &f.id, &gone.destination, Some("Gone"), true).unwrap();
        let off = plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap();
        assert_eq!((off.games.kept_off, off.games.removed, off.games.copy), (1, 0, 0));
        installations::set_kept_off(&f.catalog, &f.id, &gone.destination, None, false).unwrap();
        assert_eq!(plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap().games.removed, 1);
        assert_eq!(snapshot(&f.root), before, "planning reads only");
    }

    #[test]
    fn sync_scope_starts_from_what_was_copied_and_grows_with_plans() {
        let f = fixture(&[("nes/Kid.nes", b"nes"), ("snes/Quest.sfc", b"snes!"), ("ps2/Moved.iso", b"ps2"), ("gb/Pal.gb", b"gb"), ("gb/Pal2.gb", b"gb2")]);
        let all = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        let dest = |title: &str| all.sample.iter().find(|g| g.title == title).unwrap().destination.clone();
        // Copied before: Kid (still on the drive) and Moved (moved off the drive since).
        write(&f.root, "roms/nes/Kid.nes", b"nes");
        f.catalog.execute("INSERT INTO deployments (id, installation_id, adapter_id, adapter_contract, destination_root, plan_fingerprint, status) VALUES ('d1', ?1, 'retrobat', 1, 'x', 'f', 'completed')", [&f.id]).unwrap();
        for title in ["Kid", "Moved"] {
            f.catalog.execute("INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, size_bytes, planned_action, state) VALUES ('d1', 0, ?1, 'src', ?2, 3, 'copy', 'verified')", [title, &dest(title)]).unwrap();
        }
        let scope = sync_scope(&f.catalog, &f.id, &connected).unwrap();
        assert_eq!(scope.systems, ["nes"], "a platform whose copied games were all moved off isn't synced");
        // A later plan copy of SNES adds it; a collection copy adds just its games.
        add_to_sync_scope(&f.catalog, &f.id, &["snes".to_string()], None, &connected).unwrap();
        let pal = f.catalog.query_row("SELECT id FROM game_files WHERE filename = 'Pal.gb'", [], |r| r.get::<_, String>(0)).unwrap();
        let party = crate::collections::create(&f.catalog, "Party").unwrap();
        crate::collections::add(&f.catalog, &party, &[pal]).unwrap();
        add_to_sync_scope(&f.catalog, &f.id, &["gb".to_string()], Some(&party), &connected).unwrap();
        let scope = installations::saved_sync_scope(&f.catalog, &f.id).unwrap().unwrap();
        assert_eq!((scope.systems.clone(), scope.collections.len()), (vec!["nes".to_string(), "snes".to_string()], 1));
        let p = plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap();
        let mut titles: Vec<&str> = p.sample.iter().map(|g| g.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Pal", "Quest"], "Kid is already there; only Pal from GB (the collection), never Pal2 or Moved");
        // Stop syncing SNES: saved, nothing on the drive changes.
        installations::set_sync_scope(&f.catalog, &f.id, &installations::SyncScope { systems: vec!["nes".into()], collections: scope.collections.clone() }).unwrap();
        assert!(!plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap().sample.iter().any(|g| g.title == "Quest"));
    }

    #[test]
    fn a_synced_game_changed_on_the_drive_is_shown_and_restored_only_if_chosen() {
        let f = fixture(&[("nes/Kid.nes", b"nes")]);
        let p = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        let dest = p.sample[0].destination.clone();
        write(&f.root, "roms/nes/Kid.nes", b"changed on the drive");
        f.catalog.execute("INSERT INTO deployments (id, installation_id, adapter_id, adapter_contract, destination_root, plan_fingerprint, status) VALUES ('d1', ?1, 'retrobat', 1, 'x', 'f', 'completed')", [&f.id]).unwrap();
        f.catalog.execute("INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, size_bytes, planned_action, state) VALUES ('d1', 0, 'Kid', 'src', ?1, 3, 'copy', 'verified')", [&dest]).unwrap();
        let s = plan_sync(&f.catalog, &f.id, &Choices::new(), &connected, &roomy).unwrap();
        let kid = s.attention.iter().find(|g| g.title == "Kid").unwrap();
        let choice = kid.choice.as_ref().unwrap();
        assert_eq!((kid.action, choice.situation, choice.options.clone()), (Action::Conflict, "changed", vec!["keep", "replace"]));
        let restore = plan_sync(&f.catalog, &f.id, &Choices::from([(choice.key.clone(), Choice::Replace)]), &connected, &roomy).unwrap();
        assert_eq!(restore.games.replace, 1, "restoring is the usual replace: the drive's file is backed up first");
    }

    #[test]
    fn bios_files_from_a_library_bios_folder_are_planned_into_bios() {
        // Owner request 2026-11-07: copy BIOS files too, keeping their subfolders.
        let f = fixture(&[("bios/dc/dc_boot.bin", b"dc boot"), ("bios/scph1001.bin", b"psx bios"), ("nes/Kid.nes", b"nes")]);
        let p = plan(&f.catalog, &f.id, None, &connected, &roomy).unwrap();
        let bios = p.systems.iter().find(|s| s.system_folder == "bios").expect("a BIOS row in the plan");
        assert_eq!((bios.games, bios.platforms.clone()), (2, vec!["BIOS".to_string()]));
        let mut dest: Vec<String> = p.sample.iter().filter(|g| g.system_folder == "bios").map(|g| g.destination.replace('\\', "/")).collect();
        dest.sort();
        assert!(dest[0].ends_with("RetroBat/bios/dc/dc_boot.bin") && dest[1].ends_with("RetroBat/bios/scph1001.bin"), "{dest:?}");
        assert!(p.unsupported.iter().all(|u| u.platform != "BIOS"), "nothing left unplaced");
    }

    #[test]
    fn decisions_are_saved_except_replace() {
        let f = fixture(&[("nes/Kid.nes", b"nes")]);
        let choices = Choices::from([
            ("a".to_string(), Choice::Use { file_id: "f1".into() }),
            ("b".to_string(), Choice::KeepBoth),
            ("c".to_string(), Choice::Replace),
            ("d".to_string(), Choice::Keep),
        ]);
        installations::set_saved_choices(&f.catalog, &f.id, &choices).unwrap();
        let saved = installations::saved_choices(&f.catalog, &f.id).unwrap();
        assert_eq!(saved, Choices::from([("a".to_string(), Choice::Use { file_id: "f1".into() }), ("b".to_string(), Choice::KeepBoth)]), "replace is never remembered");
        installations::set_saved_choices(&f.catalog, &f.id, &Choices::new()).unwrap();
        assert!(installations::saved_choices(&f.catalog, &f.id).unwrap().is_empty());
        assert!(installations::set_saved_choices(&f.catalog, "nope", &choices).is_err());
    }

    #[test]
    fn game_keys_ignore_tags_but_not_disks() {
        assert_eq!(game_key("10-Yard_Fight(J).zip"), game_key("10-Yard Fight (Japan).zip"));
        assert_eq!(game_key("Super Mario Bros. (W) [!].nes"), game_key("Super Mario Bros. (World).zip"));
        assert_eq!(game_key("Legend of Zelda, The (U).zip"), game_key("The Legend of Zelda (USA).zip"));
        assert_eq!(game_key("Addams.Family.The(Alt)(U).zip"), game_key("The Addams Family (USA).zip"));
        assert_eq!(game_key("Mr. Gimmick (Japan).zip"), game_key("Mr Gimmick (J).zip"));
        assert_ne!(game_key("Tank.zip"), game_key("Ta.zip"), "only a whole word is an article");
        assert_ne!(game_key("Saga (Disk 1).fds"), game_key("Saga (Disk 2).fds"));
        assert_ne!(game_key("Game.zip"), game_key("Game II.zip"));
        assert_eq!(game_key("(Unl).zip"), None);
        assert!(preference("Lolo (U) [!].zip") < preference("Lolo (Japan).zip"));
        assert!(preference("Lolo (Japan).zip") < preference("Lolo (U) [h1].zip"), "a hack is the last choice");
        assert!(preference("Lolo (Europe).zip") < preference("Lolo (Japan).zip"));
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

    fn plan_choosing(f: &Fixture, choices: &Choices) -> DeploymentPlan {
        plan_with(&f.catalog, &f.id, None, None, choices, &connected, &roomy).unwrap()
    }

    #[test]
    fn clash_choices_pick_a_version_or_keep_both() {
        let f = fixture(&[("a/nes/Clash.nes", b"one"), ("b/nes/Clash.nes", b"other one"), ("arcade/sf2.zip", b"x"), ("arcade2/arcade/sf2.zip", b"yy")]);
        installations::set_arcade_system(&f.catalog, &f.id, Some("mame")).unwrap();
        let base = plan_choosing(&f, &Choices::new());
        assert_eq!(base.games.blocked, 4);
        let clash = base.attention.iter().find(|g| g.title == "Clash").unwrap().choice.clone().unwrap();
        assert_eq!((clash.situation, clash.options.clone()), ("clash", vec!["keep", "use", "keep_both"]));
        let arcade = base.attention.iter().find(|g| g.title == "sf2").unwrap().choice.clone().unwrap();
        assert_eq!(arcade.options, vec!["keep", "use"], "arcade sets are never renamed");

        let mut use_first = Choices::new();
        use_first.insert(clash.key.clone(), Choice::Use { file_id: clash.file_id.clone() });
        let chosen = plan_choosing(&f, &use_first);
        assert_eq!((chosen.games.copy, chosen.games.duplicate), (1, 1), "the chosen version copies, the other is skipped");
        assert_ne!(chosen.fingerprint, base.fingerprint, "choices are part of the fingerprint");

        let mut both = Choices::new();
        both.insert(clash.key.clone(), Choice::KeepBoth);
        let kept = plan_choosing(&f, &both);
        assert_eq!(kept.games.copy, 2);
        let names: BTreeSet<String> = kept.attention.iter().filter(|g| g.title == "Clash").map(|g| g.destination.replace('\\', "/").rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(names, BTreeSet::from(["Clash.nes".to_string(), "Clash (2).nes".to_string()]));

        let mut bad = Choices::new();
        bad.insert(arcade.key.clone(), Choice::KeepBoth);
        assert_eq!(plan_choosing(&f, &bad).games.blocked, 4, "an option that isn't offered is ignored");
    }

    #[test]
    fn conflict_choices_and_multi_file_games() {
        let f = fixture(&[
            ("nes/Diff.nes", b"mine"),
            ("psx/Saga/Saga.m3u", b"Saga (Disc 1).cue
"),
            ("psx/Saga/Saga (Disc 1).cue", b"FILE \"d1.bin\" BINARY
"),
            ("psx/Saga/d1.bin", b"disc"),
        ]);
        write(&f.root, "roms/nes/Diff.nes", b"theirs, longer");
        write(&f.root, "roms/psx/Saga/Saga.m3u", b"someone else's playlist");
        let base = plan_choosing(&f, &Choices::new());
        let diff = base.attention.iter().find(|g| g.title == "Diff").unwrap().choice.clone().unwrap();
        assert_eq!((diff.situation, diff.options.clone()), ("conflict", vec!["keep", "keep_both", "replace"]));
        let saga = base.attention.iter().find(|g| g.title == "Saga").unwrap().choice.clone().unwrap();
        assert_eq!(saga.options, vec!["keep"], "multi-file games aren't renamed or replaced");

        let mut replace = Choices::new();
        replace.insert(diff.key.clone(), Choice::Replace);
        let p = plan_choosing(&f, &replace);
        assert_eq!(p.games.replace, 1);
        let row = p.attention.iter().find(|g| g.title == "Diff").unwrap();
        assert_eq!(row.action, Action::Replace);
        assert_eq!(row.choice.as_ref().unwrap().current, "replace", "the row stays listed so the choice can be changed");
    }

    #[test]
    fn a_plan_can_be_limited_to_a_collection() {
        let f = fixture(&[("nes/Kid.nes", b"k"), ("nes/Metroid.nes", b"m"), ("snes/Mario.sfc", b"s")]);
        let party = crate::collections::create(&f.catalog, "Party").unwrap();
        let mario: String = f.catalog.query_row("SELECT id FROM game_files WHERE filename = 'Mario.sfc'", [], |r| r.get(0)).unwrap();
        crate::collections::add(&f.catalog, &party, &[mario]).unwrap();
        let all = plan_with(&f.catalog, &f.id, None, None, &Choices::new(), &connected, &roomy).unwrap();
        let only = plan_with(&f.catalog, &f.id, None, Some(&party), &Choices::new(), &connected, &roomy).unwrap();
        assert_eq!((all.selected_games, only.selected_games), (3, 1));
        assert_eq!(only.systems.iter().map(|s| s.system_folder.as_str()).collect::<Vec<_>>(), ["snes"], "only the collection's systems are offered");
        assert_ne!(all.fingerprint, only.fingerprint);
    }
}
