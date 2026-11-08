//! Staged, verified copy of a deployment plan (RDS-010; docs/08, docs/18).
//!
//! This is the first code in RetroDrive that writes to a user's drive. Its safety
//! rules (each one has a test below):
//!
//! 1. **Re-check first.** The plan is rebuilt and must match the fingerprint the
//!    user reviewed; the destination must be the same volume (fingerprint) and have
//!    room. Otherwise nothing is written.
//! 2. **Journal before writing.** The deployment and every item are recorded
//!    before the first byte is copied; each item moves `pending -> copying ->
//!    verified`, with its temporary path recorded while it exists.
//! 3. **Stage, verify, then commit.** Each file is streamed into a new temporary
//!    file next to its destination (`create_new`, so nothing is ever opened for
//!    overwriting), hashed with SHA-256 on the way, synced to disk, read back and
//!    compared, then renamed with a rename that **refuses to replace** an existing
//!    file. Only then is it marked verified.
//! 4. **Never overwrite, never delete.** The only file RetroDrive ever removes is a
//!    temporary file it created itself in this run (or, after a crash, one recorded
//!    in the journal whose name carries this deployment's marker).
//! 5. **Sources are read-only.** A source that changes during the copy fails that
//!    file.
//! 6. **Same size isn't proof.** "Already there" files are compared by content
//!    before being skipped; a difference is a conflict and the existing file is kept.
//! 7. **Writes stay inside the installation** and never go through links.
//!
//! 8. **Replace never deletes** (step 5b, ADR-028): the existing file is first *moved*
//!    into `.retrodrive/backup/<deployment>/…` on the same drive and checked there; the
//!    new file is then copied the usual way. If that fails or is cancelled, the original
//!    is moved back. After a crash, startup puts the original back if the new file never
//!    got its name.
//!
//! Partial games are kept: if a multi-disc game fails partway, the verified files
//! stay and running the copy again finishes it (user decision, ADR-021).

use crate::deployment::{self, Action, Item, ItemFile, SpaceFn};
use crate::installations::PresenceFn;
use crate::paths;
use crate::volumes::Presence;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

const CHUNK: usize = 1 << 20;
/// Temporary files are named `<final name>.rdpart-<first 8 characters of the deployment id>`.
pub const TEMP_MARKER: &str = ".rdpart-";
const MANIFEST_DIR: &str = ".retrodrive/deployments";
/// Where replaced originals are kept: `<installation>/.retrodrive/backup/<deployment id>/<path>`.
pub(crate) const BACKUP_DIR: &str = ".retrodrive/backup";
const ISSUE_LIMIT: usize = 200;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

/// Receives progress and answers cancellation. The `before_*` methods are test
/// seams (fault injection); production observers leave them empty.
pub trait DeployObserver {
    fn cancelled(&mut self) -> bool {
        false
    }
    fn progress(&mut self, _progress: &DeployProgress) {}
    /// After the temporary file is written and synced, before it is read back.
    fn before_verify(&mut self, _temp: &Path, _source: &Path) {}
    /// Just before the final no-replace rename.
    fn before_rename(&mut self, _destination: &Path) {}
    /// Right after an existing file was moved to the backup folder (Replace).
    fn after_backup(&mut self, _destination: &Path) {}
    /// How many games to copy at once (1 = one at a time).
    fn workers(&self) -> usize {
        DEFAULT_WORKERS
    }
}

pub struct DeployRequest<'a> {
    pub installation_id: &'a str,
    pub systems: Option<&'a [String]>,
    /// The fingerprint of the plan the user reviewed.
    pub fingerprint: &'a str,
    /// The user's conflict and clash choices (part of the fingerprint).
    pub choices: &'a deployment::Choices,
    /// Copy only the games in this collection.
    pub collection: Option<&'a str>,
    /// A Sync: the plan covers the installation's sync scope (ADR-045).
    pub sync: bool,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct DeployProgress {
    pub deployment_id: String,
    pub games_done: i64,
    pub games_total: i64,
    pub files_done: i64,
    pub files_total: i64,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub current: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct DeployIssue {
    pub game: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct DeploySummary {
    pub deployment_id: String,
    /// `completed`, `completed_with_issues`, `cancelled` or `interrupted` (drive lost).
    pub status: String,
    pub games_copied: i64,
    /// Every file was already there with identical contents.
    pub games_already_there: i64,
    /// Some files were copied but not all (a retry finishes them).
    pub games_incomplete: i64,
    /// Not copied because an existing file differs; the existing file was kept.
    pub games_conflict: i64,
    pub files_copied: i64,
    /// Existing files replaced; their originals are in the backup folder.
    pub files_replaced: i64,
    pub bytes_copied: i64,
    pub files_identical: i64,
    pub files_failed: i64,
    pub files_not_attempted: i64,
    pub cancelled: bool,
    pub drive_lost: bool,
    pub destination: String,
    pub manifest_path: Option<String>,
    pub manifest_error: Option<String>,
    pub issues: Vec<DeployIssue>,
}

impl DeploySummary {
    fn issue(&mut self, game: &str, path: &Path, message: &str) {
        if self.issues.len() < ISSUE_LIMIT {
            self.issues.push(DeployIssue {
                game: game.to_string(),
                path: path.display().to_string(),
                message: message.to_string(),
            });
        }
    }
}

/// How one file ended.
enum Outcome {
    Verified(String),
    Replaced(String),
    Conflict(String),
    Failed(String),
    Cancelled,
}

/// Copy the plan for an installation. Returns `Err` (with nothing written) if any
/// pre-check fails; otherwise a summary, even when some files failed.
pub fn deploy(
    connection: &Connection,
    job_id: Option<&str>,
    request: &DeployRequest,
    presence: PresenceFn,
    space: SpaceFn,
    observer: &mut dyn DeployObserver,
) -> Result<DeploySummary, String> {
    // 1. Re-check everything. No writes before this passes.
    let scope = if request.sync {
        Some(deployment::sync_scope(
            connection,
            request.installation_id,
            presence,
        )?)
    } else {
        None
    };
    let systems = if request.sync { None } else { request.systems };
    let (plan, items) = deployment::build(
        connection,
        request.installation_id,
        systems,
        scope.as_ref(),
        request.collection,
        request.choices,
        presence,
        space,
    )?;
    if !plan.copy_supported {
        return Err(format!(
            "RetroDrive doesn't copy to {} yet, so nothing was copied. The plan is a preview.",
            plan.adapter_name
        ));
    }
    if plan.fingerprint != request.fingerprint {
        return Err("The drive or your library changed since you reviewed the plan, so nothing was copied. Review the updated plan and try again.".into());
    }
    match plan.capacity.fits {
        Some(true) => {}
        Some(false) => {
            return Err(
                "There isn't enough free space on the destination drive, so nothing was copied."
                    .into(),
            )
        }
        None => {
            return Err(
                "Free space on the destination drive couldn't be measured, so nothing was copied."
                    .into(),
            )
        }
    }
    let (volume_root, stable_id): (String, Option<String>) = connection
        .query_row(
            "SELECT s.root_path, s.stable_id FROM installations i JOIN storage_locations s ON s.id = i.storage_location_id WHERE i.id = ?1",
            [request.installation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let install_root = PathBuf::from(&plan.installation_path);
    let work: Vec<&Item> = items
        .iter()
        .filter(|i| {
            matches!(
                i.action(),
                Action::Copy | Action::AlreadyThere | Action::Replace
            )
        })
        .collect();
    // A network share is known by the id RetroDrive leaves on it (RDS-022, ADR-046):
    // the first copy gives it one and records it, so a different machine answering to
    // the same name later shows as a different drive and is never copied to.
    let stable_id = match stable_id {
        Some(id)
            if id.starts_with("unc:")
                && !id.contains('#')
                && presence(&volume_root, Some(&id)) == Presence::Connected =>
        {
            let claimed = crate::volumes::share_stable_id(
                &volume_root,
                Some(&crate::volumes::claim_share(&volume_root)?),
            );
            connection
                .execute(
                    "UPDATE storage_locations SET stable_id = ?2 WHERE stable_id = ?1",
                    params![id, claimed],
                )
                .map_err(|e| {
                    format!("Couldn't record the share's id, so nothing was copied: {e}")
                })?;
            Some(claimed)
        }
        other => other,
    };

    // Old leftovers on this destination are cleaned before starting.
    cleanup_leftovers(connection, presence)?;
    // The copy journal uses SQLite's NORMAL sync: still durable if the app crashes,
    // and about twice as fast for many small files (docs/15). After a power loss the
    // last journal entries may be missing, so recovery also finds RetroDrive's own
    // temporary files and backups from the plan itself (`recover_by_name`).
    let _sync = JournalSync::normal(connection)?;

    // 2. Journal before writing.
    let deployment_id = crate::db::new_id();
    let marker = format!("{TEMP_MARKER}{}", &deployment_id[..8]);
    let mut item_ids: Vec<Vec<i64>> = Vec::with_capacity(work.len());
    {
        let tx = connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO deployments (id, job_id, installation_id, adapter_id, adapter_contract, destination_root, destination_stable_id, plan_fingerprint, systems_json, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'running')",
            params![
                deployment_id,
                job_id,
                request.installation_id,
                plan.adapter_id,
                plan.adapter_contract,
                plan.installation_path,
                stable_id,
                plan.fingerprint,
                request.systems.map(|s| json!(s).to_string())
            ],
        )
        .map_err(|e| e.to_string())?;
        let mut insert = tx
            .prepare(
                "INSERT INTO deployment_items (deployment_id, game_index, game_title, game_file_id, source_path, destination_path, size_bytes, planned_action, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .map_err(|e| e.to_string())?;
        for (index, item) in work.iter().enumerate() {
            let mut ids = Vec::new();
            for file in &item.files {
                let state = if matches!(
                    file.action,
                    Action::Copy | Action::AlreadyThere | Action::Replace
                ) {
                    "pending"
                } else {
                    "skipped"
                };
                insert
                    .execute(params![
                        deployment_id,
                        index as i64,
                        item.title,
                        file.game_file_id,
                        file.source.display().to_string(),
                        file.destination.display().to_string(),
                        file.size,
                        format!("{:?}", file.action).to_lowercase(),
                        state
                    ])
                    .map_err(|e| e.to_string())?;
                ids.push(tx.last_insert_rowid());
            }
            item_ids.push(ids);
        }
        drop(insert);
        tx.commit().map_err(|e| e.to_string())?;
    }

    let result = run_journaled(
        connection,
        &deployment_id,
        &marker,
        &plan.installation_path,
        &install_root,
        &work,
        &item_ids,
        &volume_root,
        stable_id.as_deref(),
        presence,
        observer,
    );
    match result {
        Ok(summary) => Ok(summary),
        Err(e) => {
            // The copy started, so some files may already be in place (all verified).
            let _ = connection.execute("UPDATE deployments SET status = 'failed', finished_at = CURRENT_TIMESTAMP WHERE id = ?1", [&deployment_id]);
            let _ = cleanup_leftovers(connection, presence);
            Err(format!("The copy stopped partway because of an internal error ({e}). Files already verified were kept, and nothing was overwritten."))
        }
    }
}

/// Files at least this big are copied one at a time, even when games are copied in
/// parallel, so a USB hard drive isn't asked to write several big files at once.
const BIG_FILE: i64 = 32 << 20;
/// Games copied at the same time (docs/15 "Copy speed"; docs/18 §11).
pub const DEFAULT_WORKERS: usize = 4;

/// What copying one game needs: journaling, progress, cancellation and test hooks.
/// A sequential copy uses the connection and observer directly. Parallel workers
/// send every journal write to the main thread and wait until it's recorded, so
/// "journal before writing" holds exactly as in a sequential copy.
trait Ctx {
    fn set_item(
        &mut self,
        id: i64,
        state: &str,
        sha: Option<&str>,
        temp: Temp,
        message: Option<&str>,
    ) -> Result<(), String>;
    fn set_backup(&mut self, id: i64, backup: Option<&str>, state: &str) -> Result<(), String>;
    fn record_hash(&mut self, game_file_id: &str, hash: &str) -> Result<(), String>;
    fn cancelled(&mut self) -> bool;
    fn add_bytes(&mut self, bytes: i64);
    fn started(&mut self, title: &str);
    fn before_verify(&mut self, temp: &Path, source: &Path);
    fn before_rename(&mut self, destination: &Path);
    fn after_backup(&mut self, destination: &Path);
}

/// Sequential copies: straight to the catalog and the observer.
struct Direct<'a> {
    connection: &'a Connection,
    observer: &'a mut dyn DeployObserver,
    progress: &'a mut DeployProgress,
    last_progress: &'a mut Instant,
}

impl Ctx for Direct<'_> {
    fn set_item(
        &mut self,
        id: i64,
        state: &str,
        sha: Option<&str>,
        temp: Temp,
        message: Option<&str>,
    ) -> Result<(), String> {
        set_item(self.connection, id, state, sha, temp, message)
    }
    fn set_backup(&mut self, id: i64, backup: Option<&str>, state: &str) -> Result<(), String> {
        set_backup(self.connection, id, backup, state)
    }
    fn record_hash(&mut self, game_file_id: &str, hash: &str) -> Result<(), String> {
        record_hash(self.connection, game_file_id, hash)
    }
    fn cancelled(&mut self) -> bool {
        self.observer.cancelled()
    }
    fn add_bytes(&mut self, bytes: i64) {
        self.progress.bytes_done += bytes;
        if self.last_progress.elapsed() >= PROGRESS_EVERY {
            self.observer.progress(self.progress);
            *self.last_progress = Instant::now();
        }
    }
    fn started(&mut self, title: &str) {
        self.progress.current = title.to_string();
    }
    fn before_verify(&mut self, temp: &Path, source: &Path) {
        self.observer.before_verify(temp, source);
    }
    fn before_rename(&mut self, destination: &Path) {
        self.observer.before_rename(destination);
    }
    fn after_backup(&mut self, destination: &Path) {
        self.observer.after_backup(destination);
    }
}

enum OwnedTemp {
    Keep,
    Set(String),
    Clear,
}

enum Hook {
    BeforeVerify(PathBuf, PathBuf),
    BeforeRename(PathBuf),
    AfterBackup(PathBuf),
}

/// Requests from parallel workers to the main thread.
enum Msg {
    Item {
        id: i64,
        state: String,
        sha: Option<String>,
        temp: OwnedTemp,
        message: Option<String>,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Backup {
        id: i64,
        backup: Option<String>,
        state: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Hash {
        game_file_id: String,
        hash: String,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Hook(Hook, mpsc::Sender<()>),
    Bytes(i64),
    Started(String),
    Done(Result<GameReport, String>),
}

/// A parallel worker's view: every request goes to the main thread.
struct Remote<'a> {
    tx: mpsc::Sender<Msg>,
    cancel: &'a AtomicBool,
}

impl Remote<'_> {
    fn ask(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<(), String>>) -> Msg,
    ) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        self.tx
            .send(build(reply))
            .map_err(|_| "The copy coordinator stopped.".to_string())?;
        answer
            .recv()
            .map_err(|_| "The copy coordinator stopped.".to_string())?
    }
    fn hook(&self, hook: Hook) {
        let (reply, answer) = mpsc::channel();
        if self.tx.send(Msg::Hook(hook, reply)).is_ok() {
            let _ = answer.recv();
        }
    }
}

impl Ctx for Remote<'_> {
    fn set_item(
        &mut self,
        id: i64,
        state: &str,
        sha: Option<&str>,
        temp: Temp,
        message: Option<&str>,
    ) -> Result<(), String> {
        let temp = match temp {
            Temp::Keep => OwnedTemp::Keep,
            Temp::Set(path) => OwnedTemp::Set(path.to_string()),
            Temp::Clear => OwnedTemp::Clear,
        };
        let (state, sha, message) = (
            state.to_string(),
            sha.map(str::to_string),
            message.map(str::to_string),
        );
        self.ask(|reply| Msg::Item {
            id,
            state,
            sha,
            temp,
            message,
            reply,
        })
    }
    fn set_backup(&mut self, id: i64, backup: Option<&str>, state: &str) -> Result<(), String> {
        let (backup, state) = (backup.map(str::to_string), state.to_string());
        self.ask(|reply| Msg::Backup {
            id,
            backup,
            state,
            reply,
        })
    }
    fn record_hash(&mut self, game_file_id: &str, hash: &str) -> Result<(), String> {
        let (game_file_id, hash) = (game_file_id.to_string(), hash.to_string());
        self.ask(|reply| Msg::Hash {
            game_file_id,
            hash,
            reply,
        })
    }
    fn cancelled(&mut self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    fn add_bytes(&mut self, bytes: i64) {
        let _ = self.tx.send(Msg::Bytes(bytes));
    }
    fn started(&mut self, title: &str) {
        let _ = self.tx.send(Msg::Started(title.to_string()));
    }
    fn before_verify(&mut self, temp: &Path, source: &Path) {
        self.hook(Hook::BeforeVerify(temp.to_path_buf(), source.to_path_buf()));
    }
    fn before_rename(&mut self, destination: &Path) {
        self.hook(Hook::BeforeRename(destination.to_path_buf()));
    }
    fn after_backup(&mut self, destination: &Path) {
        self.hook(Hook::AfterBackup(destination.to_path_buf()));
    }
}

/// How a game ended, for the summary.
#[derive(Default, PartialEq)]
enum GameEnd {
    Copied,
    AlreadyThere,
    Incomplete,
    Conflict,
    /// Cancelled before anything of it was copied: not counted.
    #[default]
    Uncounted,
}

#[derive(Default)]
struct GameReport {
    title: String,
    identical: i64,
    copied: i64,
    replaced: i64,
    bytes_copied: i64,
    failed: i64,
    files_done: i64,
    issues: Vec<(PathBuf, String)>,
    end: GameEnd,
    cancelled: bool,
    /// A file failed: the coordinator checks the drive is still there.
    failure: bool,
    /// The installation folder disappeared mid-game.
    drive_gone: bool,
}

impl GameReport {
    fn issue(&mut self, path: &Path, message: &str) {
        self.issues.push((path.to_path_buf(), message.to_string()));
    }
}

impl DeploySummary {
    /// Add one game's results. Returns whether the game counts as finished for progress.
    fn add(&mut self, report: GameReport) -> bool {
        self.files_identical += report.identical;
        self.files_copied += report.copied;
        self.files_replaced += report.replaced;
        self.bytes_copied += report.bytes_copied;
        self.files_failed += report.failed;
        for (path, message) in &report.issues {
            self.issue(&report.title, path, message);
        }
        match report.end {
            GameEnd::Copied => self.games_copied += 1,
            GameEnd::AlreadyThere => self.games_already_there += 1,
            GameEnd::Incomplete => self.games_incomplete += 1,
            GameEnd::Conflict => self.games_conflict += 1,
            GameEnd::Uncounted => {}
        }
        if report.cancelled {
            self.cancelled = true;
        }
        !report.cancelled && !report.drive_gone
    }
}

/// Files an earlier copy to this installation confirmed by content, by destination
/// (lower-case, backslashes): (source, size, sha256, when confirmed as Unix seconds).
type Confirmed = HashMap<String, (String, i64, String, i64)>;

/// From earlier copies to the same installation as `deployment_id`.
fn confirmed_before(connection: &Connection, deployment_id: &str) -> Result<Confirmed, String> {
    let mut statement = connection
        .prepare(
            "SELECT i.destination_path, i.source_path, i.size_bytes, i.sha256, i.updated_at FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
             WHERE d.installation_id = (SELECT installation_id FROM deployments WHERE id = ?1) AND d.id <> ?1 AND i.state IN ('verified', 'identical', 'replaced') AND i.sha256 IS NOT NULL ORDER BY i.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([deployment_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Confirmed::new();
    for row in rows {
        let (destination, source, size, sha, at) = row.map_err(|e| e.to_string())?;
        if let Some(at) = unix_seconds(&at) {
            out.insert(
                destination.replace('/', "\\").to_lowercase(),
                (source.replace('/', "\\").to_lowercase(), size, sha, at),
            );
        }
    }
    Ok(out)
}

/// `YYYY-MM-DD HH:MM:SS` (SQLite's UTC `CURRENT_TIMESTAMP`) as Unix seconds.
fn unix_seconds(text: &str) -> Option<i64> {
    let (date, time) = text.split_once(' ')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|p| p.parse::<i64>());
    let (h, min, sec) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + h * 3600 + min * 60 + sec)
}

/// The hash an earlier copy confirmed for this file, if nothing changed since: same
/// source and size, and neither file modified or created at or after the confirmation (Sync, ADR-043).
/// Anything else is compared by content as usual.
fn still_confirmed(
    confirmed: &Confirmed,
    source: &Path,
    destination: &Path,
    size: i64,
) -> Option<String> {
    let key = destination
        .display()
        .to_string()
        .replace('/', "\\")
        .to_lowercase();
    let (was_source, was_size, sha, at) = confirmed.get(&key)?;
    // Both dates: copying a file in Explorer keeps its old modified date but sets a new
    // created date, so a file replaced that way is still noticed.
    let secs = |t: std::io::Result<std::time::SystemTime>| {
        t.ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    };
    let unchanged = |p: &Path| {
        fs::metadata(p).ok().is_some_and(|m| {
            secs(m.modified()).is_some_and(|t| t < *at) && secs(m.created()).is_none_or(|t| t < *at)
        })
    };
    (*was_source
        == source
            .display()
            .to_string()
            .replace('/', "\\")
            .to_lowercase()
        && *was_size == size
        && unchanged(source)
        && unchanged(destination))
    .then(|| sha.clone())
}

/// Copy one game: check "already there" files by content, then copy or replace the
/// rest. Large files take `big` (when given) so only one is written at a time.
/// `confirmed`: files an earlier copy already confirmed (not read again if unchanged).
#[allow(clippy::too_many_arguments)]
fn copy_game(
    ctx: &mut dyn Ctx,
    item: &Item,
    ids: &[i64],
    install_root: &Path,
    deployment_id: &str,
    marker: &str,
    big: Option<&Mutex<()>>,
    confirmed: &Confirmed,
) -> Result<GameReport, String> {
    let mut report = GameReport {
        title: item.title.clone(),
        ..GameReport::default()
    };
    ctx.started(&item.title);

    // Files reported "already there" must match by content before they count.
    let mut differs: Option<String> = None;
    let mut unreadable: Option<String> = None;
    for (file, &id) in item
        .files
        .iter()
        .zip(ids)
        .filter(|(f, _)| f.action == Action::AlreadyThere)
    {
        if let Some(hash) = still_confirmed(confirmed, &file.source, &file.destination, file.size) {
            ctx.set_item(
                id,
                "identical",
                Some(&hash),
                Temp::Keep,
                Some("Confirmed by an earlier copy and unchanged since."),
            )?;
            report.identical += 1;
            report.files_done += 1;
            ctx.add_bytes(file.size);
            continue;
        }
        let compared = same_contents(&file.source, &file.destination, &mut || ctx.cancelled());
        match compared {
            Ok(Some(hash)) => {
                ctx.set_item(id, "identical", Some(&hash), Temp::Keep, None)?;
                ctx.record_hash(&file.game_file_id, &hash)?;
                report.identical += 1;
            }
            Ok(None) => {
                let message = "A file with the same name and size is there, but its contents differ. It was kept.";
                ctx.set_item(id, "conflict", None, Temp::Keep, Some(message))?;
                report.issue(&file.destination, message);
                differs = Some(file.destination.display().to_string());
            }
            Err(Halt::Cancelled) => {
                report.cancelled = true;
                return Ok(report);
            }
            Err(Halt::Io(e)) => {
                let message = format!("Couldn't compare with the existing file: {e}");
                ctx.set_item(id, "failed", None, Temp::Keep, Some(&message))?;
                report.issue(&file.destination, &message);
                report.failed += 1;
                unreadable = Some(file.destination.display().to_string());
            }
        }
        report.files_done += 1;
        ctx.add_bytes(file.size);
    }
    if let Some(path) = unreadable.filter(|_| differs.is_none()) {
        for (file, &id) in item
            .files
            .iter()
            .zip(ids)
            .filter(|(f, _)| matches!(f.action, Action::Copy | Action::Replace))
        {
            ctx.set_item(
                id,
                "skipped",
                None,
                Temp::Keep,
                Some(&format!(
                    "Not copied because the existing file {path} couldn't be read; try again."
                )),
            )?;
            report.files_done += 1;
            ctx.add_bytes(file.size);
        }
        report.end = GameEnd::Incomplete;
        return Ok(report);
    }
    if let Some(path) = differs {
        for (file, &id) in item
            .files
            .iter()
            .zip(ids)
            .filter(|(f, _)| matches!(f.action, Action::Copy | Action::Replace))
        {
            ctx.set_item(
                id,
                "conflict",
                None,
                Temp::Keep,
                Some(&format!(
                    "Not copied because {path} differs; the game was left as it is."
                )),
            )?;
            report.files_done += 1;
            ctx.add_bytes(file.size);
        }
        report.end = GameEnd::Conflict;
        return Ok(report);
    }

    let mut copied = 0;
    let mut problems = 0;
    for (file, &id) in item
        .files
        .iter()
        .zip(ids)
        .filter(|(f, _)| matches!(f.action, Action::Copy | Action::Replace))
    {
        let _one_big_file = match big {
            Some(lock) if file.size >= BIG_FILE => {
                Some(lock.lock().unwrap_or_else(|e| e.into_inner()))
            }
            _ => None,
        };
        let mut reported = 0;
        let outcome = if file.action == Action::Replace {
            replace_one(
                ctx,
                id,
                file,
                install_root,
                deployment_id,
                marker,
                &mut reported,
            )?
        } else {
            copy_one(ctx, id, file, install_root, marker, &mut reported)?
        };
        ctx.add_bytes(file.size - reported);
        match outcome {
            Outcome::Verified(hash) => {
                ctx.set_item(id, "verified", Some(&hash), Temp::Clear, None)?;
                ctx.record_hash(&file.game_file_id, &hash)?;
                report.copied += 1;
                report.bytes_copied += file.size;
                copied += 1;
            }
            Outcome::Replaced(hash) => {
                ctx.set_item(id, "replaced", Some(&hash), Temp::Clear, Some("Replaced; the original file is in RetroDrive's backup folder on this drive."))?;
                ctx.record_hash(&file.game_file_id, &hash)?;
                report.replaced += 1;
                report.bytes_copied += file.size;
                copied += 1;
            }
            Outcome::Conflict(message) => {
                ctx.set_item(id, "conflict", None, Temp::Keep, Some(&message))?;
                report.issue(&file.destination, &message);
                problems += 1;
            }
            Outcome::Failed(message) => {
                ctx.set_item(id, "failed", None, Temp::Keep, Some(&message))?;
                report.issue(&file.destination, &message);
                report.failed += 1;
                report.failure = true;
                problems += 1;
                if !install_root.is_dir() {
                    report.drive_gone = true;
                    report.end = GameEnd::Incomplete;
                    return Ok(report);
                }
            }
            Outcome::Cancelled => {
                ctx.set_item(
                    id,
                    "cancelled",
                    None,
                    Temp::Keep,
                    Some("Cancelled; the unfinished temporary file was removed."),
                )?;
                report.cancelled = true;
                report.end = if copied > 0 {
                    GameEnd::Incomplete
                } else {
                    GameEnd::Uncounted
                };
                return Ok(report);
            }
        }
        report.files_done += 1;
    }
    report.end = if problems > 0 {
        GameEnd::Incomplete
    } else if copied > 0 {
        GameEnd::Copied
    } else {
        GameEnd::AlreadyThere
    };
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn run_journaled(
    connection: &Connection,
    deployment_id: &str,
    marker: &str,
    destination: &str,
    install_root: &Path,
    work: &[&Item],
    item_ids: &[Vec<i64>],
    volume_root: &str,
    stable_id: Option<&str>,
    presence: PresenceFn,
    observer: &mut dyn DeployObserver,
) -> Result<DeploySummary, String> {
    let confirmed = confirmed_before(connection, deployment_id)?;
    let deployment_id = deployment_id.to_string();
    let mut progress = DeployProgress {
        deployment_id: deployment_id.clone(),
        games_total: work.len() as i64,
        ..DeployProgress::default()
    };
    for item in work {
        for file in item.files.iter().filter(|f| {
            matches!(
                f.action,
                Action::Copy | Action::AlreadyThere | Action::Replace
            )
        }) {
            progress.files_total += 1;
            progress.bytes_total += file.size;
        }
    }
    observer.progress(&progress);
    let mut summary = DeploySummary {
        deployment_id: deployment_id.clone(),
        destination: destination.to_string(),
        ..DeploySummary::default()
    };
    let mut last_progress = Instant::now();
    let drive_ok =
        || presence(volume_root, stable_id) == Presence::Connected && install_root.is_dir();

    // 3. Copy: several games at once (each game's files in order), or one at a time.
    let workers = observer.workers().clamp(1, 8).min(work.len().max(1));
    if workers <= 1 {
        for (index, item) in work.iter().enumerate() {
            if observer.cancelled() {
                summary.cancelled = true;
                break;
            }
            if !drive_ok() {
                summary.drive_lost = true;
                break;
            }
            let report = {
                let mut ctx = Direct {
                    connection,
                    observer: &mut *observer,
                    progress: &mut progress,
                    last_progress: &mut last_progress,
                };
                copy_game(
                    &mut ctx,
                    item,
                    &item_ids[index],
                    install_root,
                    &deployment_id,
                    marker,
                    None,
                    &confirmed,
                )?
            };
            let (cancelled, failure, gone) = (report.cancelled, report.failure, report.drive_gone);
            progress.files_done += report.files_done;
            if summary.add(report) {
                progress.games_done += 1;
            }
            observer.progress(&progress);
            if cancelled {
                break;
            }
            if failure && (gone || !drive_ok()) {
                summary.drive_lost = true;
                break;
            }
        }
    } else {
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let cancel = AtomicBool::new(false);
        let big = Mutex::new(());
        let mut fatal: Option<String> = None;
        std::thread::scope(|scope| {
            // The channel lives inside the scope: if this thread ever panicked, dropping
            // `rx` would drop every pending reply, so waiting workers stop instead of hanging.
            let (tx, rx) = mpsc::channel::<Msg>();
            for _ in 0..workers {
                let tx = tx.clone();
                let (next, stop, cancel, big, deployment_id, confirmed) = (
                    &next,
                    &stop,
                    &cancel,
                    &big,
                    deployment_id.as_str(),
                    &confirmed,
                );
                scope.spawn(move || {
                    let mut ctx = Remote {
                        tx: tx.clone(),
                        cancel,
                    };
                    loop {
                        if stop.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
                            break;
                        }
                        let index = next.fetch_add(1, Ordering::SeqCst);
                        if index >= work.len() {
                            break;
                        }
                        let result = copy_game(
                            &mut ctx,
                            work[index],
                            &item_ids[index],
                            install_root,
                            deployment_id,
                            marker,
                            Some(big),
                            confirmed,
                        );
                        let broken = result.is_err();
                        if tx.send(Msg::Done(result)).is_err() || broken {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let mut last_drive_check = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Msg::Item {
                        id,
                        state,
                        sha,
                        temp,
                        message,
                        reply,
                    }) => {
                        let temp = match &temp {
                            OwnedTemp::Keep => Temp::Keep,
                            OwnedTemp::Set(path) => Temp::Set(path),
                            OwnedTemp::Clear => Temp::Clear,
                        };
                        let _ = reply.send(set_item(
                            connection,
                            id,
                            &state,
                            sha.as_deref(),
                            temp,
                            message.as_deref(),
                        ));
                    }
                    Ok(Msg::Backup {
                        id,
                        backup,
                        state,
                        reply,
                    }) => {
                        let _ = reply.send(set_backup(connection, id, backup.as_deref(), &state));
                    }
                    Ok(Msg::Hash {
                        game_file_id,
                        hash,
                        reply,
                    }) => {
                        let _ = reply.send(record_hash(connection, &game_file_id, &hash));
                    }
                    Ok(Msg::Hook(hook, reply)) => {
                        match &hook {
                            Hook::BeforeVerify(temp, source) => {
                                observer.before_verify(temp, source)
                            }
                            Hook::BeforeRename(destination) => observer.before_rename(destination),
                            Hook::AfterBackup(destination) => observer.after_backup(destination),
                        }
                        // A hook may cancel; record it before the worker continues.
                        if !cancel.load(Ordering::SeqCst) && observer.cancelled() {
                            cancel.store(true, Ordering::SeqCst);
                            stop.store(true, Ordering::SeqCst);
                            summary.cancelled = true;
                        }
                        let _ = reply.send(());
                    }
                    Ok(Msg::Bytes(bytes)) => progress.bytes_done += bytes,
                    Ok(Msg::Started(title)) => progress.current = title,
                    Ok(Msg::Done(Ok(report))) => {
                        let (failure, gone) = (report.failure, report.drive_gone);
                        progress.files_done += report.files_done;
                        if summary.add(report) {
                            progress.games_done += 1;
                        }
                        if failure && (gone || !drive_ok()) {
                            summary.drive_lost = true;
                            stop.store(true, Ordering::SeqCst);
                        }
                        observer.progress(&progress);
                        last_progress = Instant::now();
                    }
                    Ok(Msg::Done(Err(e))) => {
                        fatal.get_or_insert(e);
                        stop.store(true, Ordering::SeqCst);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if !cancel.load(Ordering::SeqCst) && observer.cancelled() {
                    cancel.store(true, Ordering::SeqCst);
                    stop.store(true, Ordering::SeqCst);
                    summary.cancelled = true;
                }
                if !stop.load(Ordering::SeqCst)
                    && last_drive_check.elapsed() >= Duration::from_secs(1)
                {
                    last_drive_check = Instant::now();
                    if !drive_ok() {
                        summary.drive_lost = true;
                        stop.store(true, Ordering::SeqCst);
                    }
                }
                if last_progress.elapsed() >= PROGRESS_EVERY {
                    observer.progress(&progress);
                    last_progress = Instant::now();
                }
            }
        });
        if let Some(e) = fatal {
            return Err(e);
        }
    }

    // Anything not reached stays recorded as not attempted.
    summary.files_not_attempted = connection
        .execute("UPDATE deployment_items SET state = 'not_attempted', updated_at = CURRENT_TIMESTAMP WHERE deployment_id = ?1 AND state = 'pending'", [&deployment_id])
        .map_err(|e| e.to_string())? as i64;

    summary.status = if summary.drive_lost {
        "interrupted"
    } else if summary.cancelled {
        "cancelled"
    } else if summary.games_incomplete + summary.games_conflict > 0 || summary.files_failed > 0 {
        "completed_with_issues"
    } else {
        "completed"
    }
    .into();

    // 4. Manifest: in the catalog (the journal above) and, when the drive is still
    // there, as a file in the installation.
    if !summary.drive_lost {
        match write_manifest(connection, &deployment_id, install_root, marker, &summary) {
            Ok(path) => summary.manifest_path = Some(path.display().to_string()),
            Err(e) => summary.manifest_error = Some(e),
        }
    }
    connection
        .execute(
            "UPDATE deployments SET status = ?2, manifest_path = ?3, finished_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![deployment_id, summary.status, summary.manifest_path],
        )
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

/// Copy one file: temporary file (journaled first), SHA-256 while writing, sync,
/// read back from the drive, then a no-replace rename. `reported` counts the bytes
/// already reported as progress.
fn copy_one(
    ctx: &mut dyn Ctx,
    id: i64,
    file: &ItemFile,
    install_root: &Path,
    marker: &str,
    reported: &mut i64,
) -> Result<Outcome, String> {
    if ctx.cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let started = match fs::metadata(&file.source) {
        Ok(m) => m,
        Err(e) => {
            return Ok(Outcome::Failed(format!(
                "The source file can't be read: {e}"
            )))
        }
    };
    if started.len() != file.size as u64 {
        return Ok(Outcome::Failed(
            "The source file changed since the plan was made (its size is different).".into(),
        ));
    }
    if let Err(message) = ensure_folders(install_root, &file.relative) {
        return Ok(Outcome::Failed(message));
    }
    if fs::symlink_metadata(&file.destination).is_ok() {
        return Ok(Outcome::Conflict(
            "A file appeared at this name after the plan was made. It was kept.".into(),
        ));
    }
    let Some(name) = file.destination.file_name().and_then(|n| n.to_str()) else {
        return Ok(Outcome::Failed("The destination name isn't valid.".into()));
    };
    let temp = file.destination.with_file_name(format!("{name}{marker}"));

    // Journal the temporary path before creating it, so a crash can be cleaned up.
    ctx.set_item(
        id,
        "copying",
        None,
        Temp::Set(&temp.display().to_string()),
        None,
    )?;
    let mut out = match OpenOptions::new().write(true).create_new(true).open(&temp) {
        Ok(f) => f,
        Err(e) => {
            // Not ours (create_new failed), so never removed.
            ctx.set_item(id, "copying", None, Temp::Clear, None)?;
            return Ok(Outcome::Failed(format!(
                "Couldn't create the temporary file: {e}"
            )));
        }
    };

    let result = (|| -> Result<Outcome, Halt> {
        let mut source = File::open(&file.source)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; CHUNK];
        let mut written: u64 = 0;
        loop {
            if ctx.cancelled() {
                return Err(Halt::Cancelled);
            }
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            out.write_all(&buffer[..n])?;
            written += n as u64;
            ctx.add_bytes(n as i64);
            *reported += n as i64;
        }
        out.sync_all()?;
        let expected = hex(&hasher.finalize());
        ctx.before_verify(&temp, &file.source);

        let after = fs::metadata(&file.source)?;
        if written != started.len()
            || after.len() != started.len()
            || after.modified().ok() != started.modified().ok()
        {
            return Ok(Outcome::Failed(
                "The source file changed while it was being copied.".into(),
            ));
        }
        let actual = hash_file_from_disk(&temp, &mut || ctx.cancelled())?;
        if actual != expected {
            return Ok(Outcome::Failed(
                "The copy didn't match the original when read back, so it wasn't used.".into(),
            ));
        }
        ctx.before_rename(&file.destination);
        match rename_no_replace(&temp, &file.destination) {
            Ok(()) => Ok(Outcome::Verified(expected)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(Outcome::Conflict(
                "A file appeared at this name during the copy. It was kept.".into(),
            )),
            Err(e) => Err(Halt::Io(e)),
        }
    })();
    drop(out);

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(Halt::Cancelled) => Outcome::Cancelled,
        Err(Halt::Io(e)) => Outcome::Failed(format!("Copy failed: {e}")),
    };
    if !matches!(outcome, Outcome::Verified(_)) {
        // Our own temporary file (created with create_new above): safe to remove.
        let removed = remove_own_temp(&temp, marker);
        if removed {
            ctx.set_item(id, "copying", None, Temp::Clear, None)?;
        }
    }
    Ok(outcome)
}

/// Replace an existing file without ever deleting it: move it into the backup folder,
/// check it arrived, copy the new file the usual way, and put the original back if
/// the copy doesn't complete.
#[allow(clippy::too_many_arguments)]
fn replace_one(
    ctx: &mut dyn Ctx,
    id: i64,
    file: &ItemFile,
    install_root: &Path,
    deployment_id: &str,
    marker: &str,
    reported: &mut i64,
) -> Result<Outcome, String> {
    if ctx.cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let existing = match fs::symlink_metadata(&file.destination) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => m,
        Ok(_) => {
            return Ok(Outcome::Failed(
                "What's at this name isn't a plain file, so it wasn't replaced.".into(),
            ))
        }
        // The file went away since the plan: nothing to replace, so this is a plain copy.
        Err(_) => return copy_one(ctx, id, file, install_root, marker, reported),
    };
    let backup_relative = format!("{BACKUP_DIR}/{deployment_id}/{}", file.relative);
    if let Err(message) = ensure_folders(install_root, &backup_relative) {
        return Ok(Outcome::Failed(format!(
            "Couldn't prepare the backup folder, so nothing was changed: {message}"
        )));
    }
    let backup = paths::join(&install_root.display().to_string(), &backup_relative);
    // Journal first, then move (never delete) the original.
    ctx.set_backup(id, Some(&backup.display().to_string()), "replacing")?;
    if let Err(e) = rename_no_replace(&file.destination, &backup) {
        ctx.set_backup(id, None, "pending")?;
        return Ok(Outcome::Failed(format!(
            "Couldn't move the existing file to the backup folder, so nothing was changed: {e}"
        )));
    }
    ctx.after_backup(&file.destination);
    let arrived = fs::metadata(&backup)
        .map(|m| m.len() == existing.len())
        .unwrap_or(false);
    let outcome = if arrived {
        copy_one(ctx, id, file, install_root, marker, reported)?
    } else {
        Outcome::Failed("The backup of the existing file couldn't be confirmed.".into())
    };
    if let Outcome::Verified(hash) = outcome {
        return Ok(Outcome::Replaced(hash));
    }
    // Put the original back, but only into a free name (never over anything).
    let restored = fs::symlink_metadata(&file.destination).is_err()
        && rename_no_replace(&backup, &file.destination).is_ok();
    if restored {
        ctx.set_backup(id, None, "copying")?;
    }
    Ok(match (restored, outcome) {
        (true, Outcome::Cancelled) => Outcome::Cancelled,
        (true, Outcome::Failed(m) | Outcome::Conflict(m)) => {
            Outcome::Failed(format!("{m} Your original file was put back."))
        }
        (false, Outcome::Failed(m) | Outcome::Conflict(m)) => Outcome::Failed(format!(
            "{m} Your original file is safe in {}.",
            backup.display()
        )),
        (false, _) => Outcome::Failed(format!(
            "The replace didn't finish. Your original file is safe in {}.",
            backup.display()
        )),
        (true, other) => other,
    })
}

/// Sets `PRAGMA synchronous = NORMAL` for the copy and restores FULL afterwards.
struct JournalSync<'c>(&'c Connection);

impl<'c> JournalSync<'c> {
    fn normal(connection: &'c Connection) -> Result<Self, String> {
        connection
            .execute_batch("PRAGMA synchronous = NORMAL;")
            .map_err(|e| e.to_string())?;
        Ok(Self(connection))
    }
}

impl Drop for JournalSync<'_> {
    fn drop(&mut self) {
        let _ = self.0.execute_batch("PRAGMA synchronous = FULL;");
    }
}

fn set_backup(
    connection: &Connection,
    id: i64,
    backup: Option<&str>,
    state: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE deployment_items SET backup_path = ?2, state = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, backup, state],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

enum Halt {
    Cancelled,
    Io(io::Error),
}

impl From<io::Error> for Halt {
    fn from(e: io::Error) -> Self {
        Halt::Io(e)
    }
}

/// Create each missing folder between the installation and the file, one level at a
/// time. Refuses links and files in the way.
pub(crate) fn ensure_folders(install_root: &Path, relative: &str) -> Result<(), String> {
    let parts: Vec<&str> = relative.split(['/', '\\']).collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(':'))
    {
        return Err("The destination would be outside the installation folder.".into());
    }
    let mut current = install_root.to_path_buf();
    for part in &parts[..parts.len().saturating_sub(1)] {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a link to another location; RetroDrive won't write through it.",
                    current.display()
                ))
            }
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "{} is a file where a folder is needed.",
                    current.display()
                ))
            }
            Err(_) => match fs::create_dir(&current) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && current.is_dir() => {}
                Err(e) => {
                    return Err(format!(
                        "Couldn't create the folder {}: {e}",
                        current.display()
                    ))
                }
            },
        }
    }
    Ok(())
}

/// Remove a temporary file only if its name carries the expected marker and it is a
/// plain file (never a link or folder).
pub(crate) fn remove_own_temp(temp: &Path, marker: &str) -> bool {
    let named = temp
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(marker) && n.len() > marker.len());
    match fs::symlink_metadata(temp) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        // Can't check it (drive gone, access denied): keep tracking it.
        Err(_) => false,
        Ok(m) if named && m.is_file() && !m.file_type().is_symlink() => {
            fs::remove_file(temp).is_ok()
        }
        Ok(_) => false,
    }
}

/// SHA-256 of a file (used by undo to confirm a file is unchanged).
pub(crate) fn sha256_of(path: &Path) -> io::Result<String> {
    hash_file(path, &mut || false).map_err(|e| match e {
        Halt::Io(e) => e,
        Halt::Cancelled => io::Error::other("cancelled"),
    })
}

fn hash_file(path: &Path, cancelled: &mut dyn FnMut() -> bool) -> Result<String, Halt> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancelled() {
            return Err(Halt::Cancelled);
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Hash a just-written file reading from the drive, not the Windows file cache
/// (`FILE_FLAG_NO_BUFFERING`; Windows writes any cached data out first, so the
/// bytes come from the device). Falls back to a normal read where the filesystem
/// doesn't support unbuffered reads.
#[cfg(windows)]
fn hash_file_from_disk(path: &Path, cancelled: &mut dyn FnMut() -> bool) -> Result<String, Halt> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_NO_BUFFERING;
    // Unbuffered reads need sector-aligned buffers and sizes; 64 KiB covers every sector size.
    const ALIGN: usize = 64 * 1024;
    let Ok(mut file) = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING)
        .open(path)
    else {
        return hash_file(path, cancelled);
    };
    let mut storage = vec![0u8; CHUNK + ALIGN];
    let offset = storage.as_ptr().align_offset(ALIGN);
    let buffer = &mut storage[offset..offset + CHUNK];
    let mut hasher = Sha256::new();
    loop {
        if cancelled() {
            return Err(Halt::Cancelled);
        }
        let n = match file.read(buffer) {
            Ok(n) => n,
            Err(e) if e.raw_os_error() == Some(87) => return hash_file(path, cancelled), // ERROR_INVALID_PARAMETER: unsupported
            Err(e) => return Err(Halt::Io(e)),
        };
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

#[cfg(not(windows))]
fn hash_file_from_disk(path: &Path, cancelled: &mut dyn FnMut() -> bool) -> Result<String, Halt> {
    hash_file(path, cancelled)
}

/// `Some(source hash)` when both files have identical contents. Reads both side by
/// side and stops at the first difference.
fn same_contents(
    source: &Path,
    existing: &Path,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<String>, Halt> {
    if fs::metadata(source)?.len() != fs::metadata(existing)?.len() {
        return Ok(None);
    }
    let (mut a, mut b) = (File::open(source)?, File::open(existing)?);
    let (mut left, mut right) = (vec![0u8; CHUNK], vec![0u8; CHUNK]);
    let mut hasher = Sha256::new();
    loop {
        if cancelled() {
            return Err(Halt::Cancelled);
        }
        let n = read_full(&mut a, &mut left)?;
        let m = read_full(&mut b, &mut right)?;
        if n != m || left[..n] != right[..m] {
            return Ok(None);
        }
        if n == 0 {
            return Ok(Some(hex(&hasher.finalize())));
        }
        hasher.update(&left[..n]);
    }
}

/// Fill `buffer` as far as the file allows (short reads are normal for some devices).
fn read_full(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Rename that fails with `AlreadyExists` instead of replacing the destination.
/// (`std::fs::rename` replaces existing files on Windows.)
#[cfg(windows)]
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let wide = |p: &Path| -> Vec<u16> {
        long_path(p)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (f, t) = (wide(from), wide(to));
    // SAFETY: both buffers are valid NUL-terminated UTF-16 strings. Without
    // MOVEFILE_REPLACE_EXISTING the call fails if `to` exists.
    if unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::hard_link(from, to)?;
    fs::remove_file(from)
}

/// Win32 calls made directly need the `\\?\` form for paths over 260 characters.
#[cfg(windows)]
fn long_path(path: &Path) -> PathBuf {
    // `\\?\` paths are taken literally by Windows, so `/` must become `\` first.
    let text = path.display().to_string().replace('/', "\\");
    if text.starts_with(r"\\?\") {
        path.to_path_buf()
    } else if let Some(rest) = text.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{rest}"))
    } else {
        PathBuf::from(format!(r"\\?\{text}"))
    }
}

/// What to do with an item's recorded temporary path.
#[derive(Clone, Copy)]
enum Temp<'a> {
    Keep,
    Set(&'a str),
    Clear,
}

fn set_item(
    connection: &Connection,
    id: i64,
    state: &str,
    sha: Option<&str>,
    temp: Temp,
    message: Option<&str>,
) -> Result<(), String> {
    let (change, value) = match temp {
        Temp::Keep => (false, None),
        Temp::Set(path) => (true, Some(path)),
        Temp::Clear => (true, None),
    };
    connection
        .execute(
            "UPDATE deployment_items SET state = ?2, sha256 = COALESCE(?3, sha256), temp_path = CASE WHEN ?6 THEN ?4 ELSE temp_path END,
                 message = COALESCE(?5, message), updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![id, state, sha, value, message, change],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Remember a verified checksum on the catalog file (helps duplicate detection later).
fn record_hash(connection: &Connection, game_file_id: &str, hash: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE game_files SET sha256 = ?2 WHERE id = ?1 AND sha256 IS NULL",
            params![game_file_id, hash],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Write `<installation>/.retrodrive/deployments/<id>.json` the same safe way as
/// game files: new temporary file, sync, no-replace rename.
fn write_manifest(
    connection: &Connection,
    deployment_id: &str,
    install_root: &Path,
    marker: &str,
    summary: &DeploySummary,
) -> Result<PathBuf, String> {
    let mut statement = connection
        .prepare("SELECT game_title, destination_path, size_bytes, planned_action, state, sha256, message, backup_path FROM deployment_items WHERE deployment_id = ?1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let root_text = install_root.display().to_string();
    let items: Vec<serde_json::Value> = statement
        .query_map([deployment_id], |r| {
            let destination: String = r.get(1)?;
            let relative = destination.strip_prefix(&root_text).unwrap_or(&destination).trim_start_matches(['\\', '/']).replace('\\', "/");
            Ok(json!({
                "game": r.get::<_, String>(0)?,
                "path": relative,
                "size": r.get::<_, i64>(2)?,
                "planned": r.get::<_, String>(3)?,
                "outcome": r.get::<_, String>(4)?,
                "sha256": r.get::<_, Option<String>>(5)?,
                "message": r.get::<_, Option<String>>(6)?,
                "backup": r.get::<_, Option<String>>(7)?.map(|b| b.strip_prefix(&root_text).unwrap_or(&b).trim_start_matches(['\\', '/']).replace('\\', "/")),
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let created: Option<String> = connection
        .query_row(
            "SELECT created_at FROM deployments WHERE id = ?1",
            [deployment_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let document = json!({
        "format": "retrodrive-deployment-manifest",
        "format_version": 1,
        "app_version": env!("CARGO_PKG_VERSION"),
        "deployment_id": deployment_id,
        "created_at": created,
        "status": summary.status,
        "games_copied": summary.games_copied,
        "files_copied": summary.files_copied,
        "files_replaced": summary.files_replaced,
        "bytes_copied": summary.bytes_copied,
        "items": items,
    });
    let relative = format!("{MANIFEST_DIR}/{deployment_id}.json");
    ensure_folders(install_root, &relative)?;
    let destination = paths::join(&root_text, &relative);
    let temp = destination.with_file_name(format!("{deployment_id}.json{marker}"));
    let body = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
    let written = (|| -> io::Result<()> {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        out.write_all(&body)?;
        out.sync_all()?;
        drop(out);
        rename_no_replace(&temp, &destination)
    })();
    if let Err(e) = written {
        remove_own_temp(&temp, marker);
        return Err(format!("Couldn't write the manifest file: {e}"));
    }
    Ok(destination)
}

/// Whether `path` is strictly inside `root`, compared by path components and
/// without regard to case (works for drive roots like `E:\`).
pub(crate) fn is_inside(path: &str, root: &str) -> bool {
    let (path, root) = (
        PathBuf::from(path.to_lowercase()),
        PathBuf::from(root.to_lowercase()),
    );
    path != root && path.starts_with(&root)
}

/// At startup: deployments still `running` from a previous session become
/// `interrupted`, and their unfinished items say so. Temporary paths are kept so
/// `cleanup_leftovers` can remove them once the drive is confirmed.
pub fn recover(connection: &Connection) -> Result<usize, String> {
    connection
        .execute(
            "UPDATE deployment_items SET state = 'interrupted', message = 'The application closed before this file finished.', updated_at = CURRENT_TIMESTAMP
             WHERE state IN ('pending', 'copying') AND deployment_id IN (SELECT id FROM deployments WHERE status = 'running')",
            [],
        )
        .map_err(|e| e.to_string())?;
    connection
        .execute("UPDATE deployments SET status = 'interrupted', finished_at = CURRENT_TIMESTAMP WHERE status = 'running'", [])
        .map_err(|e| e.to_string())
}

/// Remove temporary files left by interrupted deployments, but only when the
/// destination drive is connected and confirmed by its fingerprint, the path is
/// inside that deployment's installation, and the name carries that deployment's
/// marker. Returns how many files were removed.
pub fn cleanup_leftovers(connection: &Connection, presence: PresenceFn) -> Result<usize, String> {
    let rows: Vec<(i64, String, String, Option<String>, String)> = {
        let mut statement = connection
            .prepare(
                "SELECT i.id, i.temp_path, d.destination_root, d.destination_stable_id, d.id
                 FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
                 WHERE i.temp_path IS NOT NULL AND d.status <> 'running'",
            )
            .map_err(|e| e.to_string())?;
        let mapped = statement
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut removed = 0;
    for (id, temp, root, stable, deployment) in rows {
        let Some((volume, _)) = paths::split_volume(Path::new(&root)) else {
            continue;
        };
        if presence(&volume, stable.as_deref()) != Presence::Connected {
            continue;
        }
        let temp_path = PathBuf::from(&temp);
        let inside = is_inside(&temp, &root);
        let marker = format!("{TEMP_MARKER}{}", &deployment[..8.min(deployment.len())]);
        if !inside || !temp.ends_with(&marker) {
            // Never touch a path that doesn't look like one of ours; just stop tracking it.
            connection
                .execute(
                    "UPDATE deployment_items SET temp_path = NULL WHERE id = ?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
            continue;
        }
        let existed = fs::symlink_metadata(&temp_path).is_ok();
        if remove_own_temp(&temp_path, &marker) {
            if existed {
                removed += 1;
            }
            connection
                .execute(
                    "UPDATE deployment_items SET temp_path = NULL, state = CASE WHEN state IN ('copying', 'interrupted') THEN 'failed' ELSE state END,
                         message = 'Interrupted; the unfinished temporary file was removed.', updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(removed
        + recover_by_name(connection, presence)?
        + restore_interrupted_replaces(connection, presence)?
        + crate::restore::recover_undo(connection, presence)?
        + crate::customizer::recover(connection, presence)?)
}

/// Recovery that doesn't rely on the last journal entries (which a power loss can
/// drop): for unfinished items of interrupted deployments, look for the temporary
/// file RetroDrive would have used (`<destination><marker>`) and remove it if it's
/// ours; for unfinished replaces, look in the deployment's backup folder.
fn recover_by_name(connection: &Connection, presence: PresenceFn) -> Result<usize, String> {
    let rows: Vec<(i64, String, String, Option<String>, String, String)> = {
        let mut statement = connection
            .prepare(
                "SELECT i.id, i.destination_path, d.destination_root, d.destination_stable_id, d.id, i.planned_action
                 FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
                 WHERE d.status IN ('interrupted', 'failed', 'cancelled') AND i.temp_path IS NULL
                   AND i.state NOT IN ('verified', 'identical', 'replaced', 'restored', 'skipped', 'conflict', 'undoing', 'undone')",
            )
            .map_err(|e| e.to_string())?;
        let mapped = statement
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut fixed = 0;
    let mut confirmed: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for (id, destination, root, stable, deployment, planned) in rows {
        let Some((volume, _)) = paths::split_volume(Path::new(&root)) else {
            continue;
        };
        let online = *confirmed
            .entry(root.clone())
            .or_insert_with(|| presence(&volume, stable.as_deref()) == Presence::Connected);
        if !online || !is_inside(&destination, &root) {
            continue;
        }
        let marker = format!("{TEMP_MARKER}{}", &deployment[..8.min(deployment.len())]);
        let temp = PathBuf::from(format!("{destination}{marker}"));
        if fs::symlink_metadata(&temp).is_ok() && remove_own_temp(&temp, &marker) {
            fixed += 1;
            connection
                .execute("UPDATE deployment_items SET state = 'failed', message = 'Interrupted; the unfinished temporary file was removed.' WHERE id = ?1", [id])
                .map_err(|e| e.to_string())?;
        }
        if planned == "replace" {
            let relative = destination[root.len()..]
                .trim_start_matches(['\\', '/'])
                .to_string();
            let backup = paths::join(
                &root,
                &format!("{BACKUP_DIR}/{deployment}/{}", relative.replace('\\', "/")),
            );
            let dest = PathBuf::from(&destination);
            if fs::symlink_metadata(&dest).is_err()
                && fs::metadata(&backup).is_ok_and(|m| m.is_file())
                && rename_no_replace(&backup, &dest).is_ok()
            {
                fixed += 1;
                connection
                    .execute("UPDATE deployment_items SET state = 'restored', message = 'Interrupted during replace; the original file was put back.' WHERE id = ?1", [id])
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(fixed)
}

/// After a crash during Replace: if the new file never got its name, move the
/// original back from the backup folder; if it did, the replace completed. Only on
/// the confirmed drive, only for paths inside that installation's backup folder.
fn restore_interrupted_replaces(
    connection: &Connection,
    presence: PresenceFn,
) -> Result<usize, String> {
    let rows: Vec<(i64, String, String, String, Option<String>)> = {
        let mut statement = connection
            .prepare(
                "SELECT i.id, i.backup_path, i.destination_path, d.destination_root, d.destination_stable_id
                 FROM deployment_items i JOIN deployments d ON d.id = i.deployment_id
                 WHERE i.backup_path IS NOT NULL AND i.state NOT IN ('replaced', 'restored', 'undoing', 'undone') AND d.status <> 'running'",
            )
            .map_err(|e| e.to_string())?;
        let mapped = statement
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut restored = 0;
    for (id, backup, destination, root, stable) in rows {
        let Some((volume, _)) = paths::split_volume(Path::new(&root)) else {
            continue;
        };
        if presence(&volume, stable.as_deref()) != Presence::Connected {
            continue;
        }
        let backup_root = format!(
            "{}\\{}",
            root.trim_end_matches(['\\', '/']),
            BACKUP_DIR.replace('/', "\\")
        );
        if !is_inside(&backup, &backup_root)
            && !is_inside(&backup.replace('\\', "/"), &backup_root.replace('\\', "/"))
            || !is_inside(&destination, &root)
        {
            continue; // never move anything that isn't ours
        }
        let (backup_path, destination_path) = (PathBuf::from(&backup), PathBuf::from(&destination));
        if fs::symlink_metadata(&destination_path).is_ok() {
            connection
                .execute("UPDATE deployment_items SET state = 'replaced', message = 'Replaced (finished before an interruption); the original is in the backup folder.' WHERE id = ?1", [id])
                .map_err(|e| e.to_string())?;
        } else if fs::metadata(&backup_path).is_ok_and(|m| m.is_file())
            && rename_no_replace(&backup_path, &destination_path).is_ok()
        {
            restored += 1;
            connection
                .execute(
                    "UPDATE deployment_items SET state = 'restored', backup_path = NULL, message = 'Interrupted during replace; the original file was put back.' WHERE id = ?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::{VolumeProbe, VolumeSpace};
    use crate::{db, installations, scanner};

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }
    fn unknown(_: &str) -> VolumeProbe {
        VolumeProbe::unknown()
    }
    fn roomy(_: &Path) -> Option<VolumeSpace> {
        Some(VolumeSpace {
            free_bytes: 500 << 30,
            total_bytes: 1 << 40,
            filesystem: Some("NTFS".into()),
            cluster_bytes: 4096,
        })
    }

    #[derive(Default)]
    struct Quiet;
    impl DeployObserver for Quiet {}

    fn write(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    struct Fixture {
        dir: tempfile::TempDir,
        root: PathBuf,
        library: PathBuf,
        catalog: Connection,
        id: String,
        choices: deployment::Choices,
    }

    impl Fixture {
        fn new(files: &[(&str, &[u8])]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("RetroBat");
            for d in ["emulationstation", "emulators", "roms", "bios"] {
                fs::create_dir_all(root.join(d)).unwrap();
            }
            fs::write(root.join("RetroBat.exe"), b"").unwrap();
            fs::write(root.join("retrobat.ini"), b"").unwrap();
            let library = dir.path().join("library");
            for (path, contents) in files {
                write(&library, path, contents);
            }
            let mut catalog = Connection::open_in_memory().unwrap();
            db::migrate(&mut catalog, None).unwrap();
            scanner::scan(&mut catalog, &library, &unknown).unwrap();
            let id = installations::add(&catalog, &root, "retrobat", &unknown).unwrap();
            Fixture {
                dir,
                root,
                library,
                catalog,
                id,
                choices: deployment::Choices::new(),
            }
        }
        fn fingerprint(&self) -> String {
            deployment::plan_with(
                &self.catalog,
                &self.id,
                None,
                None,
                &self.choices,
                &connected,
                &roomy,
            )
            .unwrap()
            .fingerprint
        }
        fn deploy_with(
            &mut self,
            observer: &mut dyn DeployObserver,
        ) -> Result<DeploySummary, String> {
            let fingerprint = self.fingerprint();
            deploy(
                &self.catalog,
                None,
                &DeployRequest {
                    installation_id: &self.id,
                    systems: None,
                    fingerprint: &fingerprint,
                    choices: &self.choices,
                    collection: None,
                    sync: false,
                },
                &connected,
                &roomy,
                observer,
            )
        }
        fn deploy(&mut self) -> DeploySummary {
            self.deploy_with(&mut Quiet).unwrap()
        }
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        walkdir::WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .map(|e| e.unwrap())
            .map(|e| {
                let contents = if e.file_type().is_file() {
                    fs::read(e.path()).unwrap()
                } else {
                    Vec::new()
                };
                (e.path().to_path_buf(), contents)
            })
            .collect()
    }

    fn temps_left(root: &Path) -> Vec<PathBuf> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .map(|e| e.unwrap().path().to_path_buf())
            .filter(|p| p.display().to_string().contains(TEMP_MARKER))
            .collect()
    }

    fn item_states(c: &Connection) -> Vec<String> {
        let mut s = c
            .prepare("SELECT state FROM deployment_items ORDER BY id")
            .unwrap();
        s.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn saga() -> Vec<(&'static str, &'static [u8])> {
        vec![
            (
                "psx/Saga/Saga.m3u",
                b"Saga (Disc 1).cue\nSaga (Disc 2).cue\n",
            ),
            ("psx/Saga/Saga (Disc 1).cue", b"FILE \"d1.bin\" BINARY\n"),
            ("psx/Saga/Saga (Disc 2).cue", b"FILE \"d2.bin\" BINARY\n"),
        ]
    }

    fn batocera_share(dir: &Path) -> (PathBuf, PathBuf) {
        let share = dir.join("share");
        for entry in ["roms/snes", "bios", "system"] {
            fs::create_dir_all(share.join(entry)).unwrap();
        }
        fs::write(share.join("system/batocera.conf"), b"").unwrap();
        let library = dir.join("library");
        write(&library, "snes/Mario.sfc", b"mario");
        write(&library, "snes/Zelda.sfc", b"zelda!");
        (share, library)
    }

    #[test]
    fn batocera_is_copied_to_like_retrobat() {
        // RDS-022: a Batocera game folder (e.g. <drive>\batocera) gets verified copies.
        let dir = tempfile::tempdir().unwrap();
        let (share, library) = batocera_share(dir.path());
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &share, "batocera", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let plan =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert!(plan.copy_supported);
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(&catalog, None, &request, &connected, &roomy, &mut Quiet).unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 2)
        );
        assert_eq!(
            fs::read(share.join("roms/snes/Mario.sfc")).unwrap(),
            b"mario"
        );
        assert!(
            !share.join(".retrodrive").join("share-id").exists(),
            "a local drive needs no share id"
        );
    }

    #[test]
    fn retropie_is_copied_to_in_both_layouts() {
        // RDS-023 step 2: the roms share holds the system folders; a USB retropie folder has roms inside.
        for usb in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = if usb {
                dir.path().join("stick").join("retropie")
            } else {
                dir.path().join("share").join("roms")
            };
            let games = if usb { root.join("roms") } else { root.clone() };
            for d in ["snes", "atarilynx", "ports", "coleco"] {
                fs::create_dir_all(games.join(d)).unwrap();
            }
            let library = dir.path().join("library");
            write(&library, "snes/Mario.sfc", b"mario");
            write(&library, "lynx/Racer.lnx", b"lynx!");
            let mut catalog = Connection::open_in_memory().unwrap();
            db::migrate(&mut catalog, None).unwrap();
            scanner::scan(&mut catalog, &library, &unknown).unwrap();
            let id = installations::add(&catalog, &root, "retropie", &unknown).unwrap();
            let choices = deployment::Choices::new();
            let plan =
                deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy)
                    .unwrap();
            assert!(plan.copy_supported);
            assert_eq!(plan.systems_prefix, if usb { "roms\\" } else { "" });
            let request = DeployRequest {
                installation_id: &id,
                systems: None,
                fingerprint: &plan.fingerprint,
                choices: &choices,
                collection: None,
                sync: false,
            };
            let summary = deploy(&catalog, None, &request, &connected, &roomy, &mut Quiet).unwrap();
            assert_eq!(
                (summary.status.as_str(), summary.games_copied),
                ("completed", 2),
                "usb: {usb}"
            );
            assert_eq!(fs::read(games.join("snes/Mario.sfc")).unwrap(), b"mario");
            assert_eq!(
                fs::read(games.join("atarilynx/Racer.lnx")).unwrap(),
                b"lynx!",
                "RetroPie's folder name"
            );
        }
    }

    #[test]
    fn recalbox_shares_are_copied_to() {
        // RDS-025 step 5: a Recalbox share (\RECALBOX\share, a drive's recalbox folder or the SD card's SHARE).
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("recalbox");
        for d in ["roms/snes", "roms/o2em", "bios", "saves", "system"] {
            fs::create_dir_all(share.join(d)).unwrap();
        }
        fs::write(share.join("system/recalbox.conf"), b"").unwrap();
        let library = dir.path().join("library");
        write(&library, "snes/Mario.sfc", b"mario");
        write(&library, "odyssey2/Pickaxe.bin", b"o2!");
        write(&library, "bios/scph1001.bin", b"psx bios");
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &share, "recalbox", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let plan =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert!(plan.copy_supported);
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(&catalog, None, &request, &connected, &roomy, &mut Quiet).unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 3),
            "{:?}",
            summary.issues
        );
        assert_eq!(
            fs::read(share.join("roms/o2em/Pickaxe.bin")).unwrap(),
            b"o2!",
            "Recalbox's folder name"
        );
        assert_eq!(
            fs::read(share.join("bios/scph1001.bin")).unwrap(),
            b"psx bios",
            "BIOS too"
        );
    }

    #[test]
    fn rocknix_roms_folders_are_copied_to() {
        // ADR-077: ROCKNIX's games-roms share is its roms folder: system folders at the top,
        // ROCKNIX's folder names, BIOS in its bios folder.
        let dir = tempfile::tempdir().unwrap();
        let roms = dir.path().join("games-roms");
        for d in ["snes", "atarilynx", "fbn", "bios", "bezels", "savestates"] {
            fs::create_dir_all(roms.join(d)).unwrap();
        }
        let library = dir.path().join("library");
        write(&library, "snes/Mario.sfc", b"mario");
        write(&library, "lynx/Racer.lnx", b"lynx!");
        write(&library, "bios/scph1001.bin", b"psx bios");
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &roms, "rocknix", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let plan =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert!(plan.copy_supported && plan.systems_prefix.is_empty());
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(&catalog, None, &request, &connected, &roomy, &mut Quiet).unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 3),
            "{:?}",
            summary.issues
        );
        assert_eq!(
            fs::read(roms.join("atarilynx/Racer.lnx")).unwrap(),
            b"lynx!",
            "ROCKNIX's folder name"
        );
        assert_eq!(
            fs::read(roms.join("bios/scph1001.bin")).unwrap(),
            b"psx bios",
            "BIOS in ROCKNIX's bios folder"
        );
        // A second copy finds everything already there.
        let again =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert_eq!(again.bytes_to_copy, 0);
    }

    #[test]
    fn esde_roms_folders_are_copied_to() {
        // RDS-025 step 2: ES-DE's ROMs folder holds the system folders; ES-DE's names.
        let dir = tempfile::tempdir().unwrap();
        let roms = dir.path().join("ROMs");
        for d in ["snes", "atarilynx", "ports", "colecovision"] {
            fs::create_dir_all(roms.join(d)).unwrap();
            fs::write(roms.join(d).join("systeminfo.txt"), b"").unwrap();
        }
        fs::write(
            roms.join("systems.txt"),
            b"snes: Nintendo SNES
",
        )
        .unwrap();
        let library = dir.path().join("library");
        write(&library, "snes/Mario.sfc", b"mario");
        write(&library, "lynx/Racer.lnx", b"lynx!");
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &roms, "esde", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let plan =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy).unwrap();
        assert!(plan.copy_supported && plan.systems_prefix.is_empty());
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(&catalog, None, &request, &connected, &roomy, &mut Quiet).unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 2)
        );
        assert_eq!(
            fs::read(roms.join("atarilynx/Racer.lnx")).unwrap(),
            b"lynx!",
            "ES-DE's folder name"
        );
        assert_eq!(
            fs::read(roms.join("snes/systeminfo.txt")).unwrap(),
            b"",
            "ES-DE's own files untouched"
        );
    }

    /// The same over real SMB, the way RetroPie's roms share is reached (run by hand:
    /// `cargo test -- --ignored retropie_over_a_network_share`; needs this PC's C$ admin share).
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn retropie_over_a_network_share_is_known_by_its_id() {
        use crate::volumes;
        let dir = tempfile::tempdir().unwrap();
        let roms = dir.path().join("roms");
        for d in ["snes", "ports", "coleco"] {
            fs::create_dir_all(roms.join(d)).unwrap();
        }
        let library = dir.path().join("library");
        write(&library, "snes/Mario.sfc", b"mario");
        let local = fs::canonicalize(&roms)
            .unwrap()
            .display()
            .to_string()
            .trim_start_matches(r"\\?\")
            .to_string();
        let unc = format!(
            r"\\localhost\{}$\{}\",
            &local[..1],
            local[3..].trim_end_matches('\\')
        );
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &roms, "retropie", &unknown).unwrap();
        let location: String = catalog
            .query_row(
                "SELECT storage_location_id FROM installations WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        catalog.execute("INSERT INTO storage_locations (id, kind, stable_id, display_name, root_path) VALUES ('share', 'network', ?1, ?2, ?2)", params![volumes::share_stable_id(&unc, None), unc]).unwrap();
        catalog.execute("UPDATE installations SET storage_location_id = 'share', relative_path = '' WHERE storage_location_id = ?1", [&location]).unwrap();
        let choices = deployment::Choices::new();
        let plan = deployment::plan_with(
            &catalog,
            &id,
            None,
            None,
            &choices,
            &volumes::presence,
            &volumes::space,
        )
        .unwrap();
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(
            &catalog,
            None,
            &request,
            &volumes::presence,
            &volumes::space,
            &mut Quiet,
        )
        .unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 1),
            "{:?}",
            summary.issues
        );
        assert_eq!(fs::read(roms.join("snes/Mario.sfc")).unwrap(), b"mario");
        assert!(volumes::share_id(&unc).is_some(), "the share got its id");
        fs::write(roms.join(".retrodrive").join("share-id"), "another-pi").unwrap();
        assert!(deployment::plan_with(
            &catalog,
            &id,
            None,
            None,
            &choices,
            &volumes::presence,
            &volumes::space
        )
        .unwrap_err()
        .contains("isn't connected"));
    }

    /// Real SMB through this PC's own admin share (run by hand: `cargo test -- --ignored
    /// over_a_network_share`; needs `\\localhost\C$`). The catalog records the scratch
    /// folder as the share's root, exactly as `\\BATOCERA\share\` would be.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn batocera_over_a_network_share_is_known_by_its_id() {
        use crate::volumes;
        let dir = tempfile::tempdir().unwrap();
        let (share, library) = batocera_share(dir.path());
        let long = fs::canonicalize(&share).unwrap().display().to_string();
        let local = long.trim_start_matches(r"\\?\").to_string();
        let unc = format!(
            r"\\localhost\{}$\{}\",
            &local[..1],
            local[3..].trim_end_matches('\\')
        );
        assert!(Path::new(&unc).is_dir(), "{unc} must be reachable");
        let mut catalog = Connection::open_in_memory().unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &library, &unknown).unwrap();
        let id = installations::add(&catalog, &share, "batocera", &unknown).unwrap();
        // Point the record at the share, as adding \\BATOCERA\share does.
        let location: String = catalog
            .query_row(
                "SELECT storage_location_id FROM installations WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        catalog.execute("INSERT INTO storage_locations (id, kind, stable_id, display_name, root_path) VALUES ('share', 'network', ?1, ?2, ?2)", params![volumes::share_stable_id(&unc, None), unc]).unwrap();
        catalog.execute("UPDATE installations SET storage_location_id = 'share' WHERE storage_location_id = ?1", [&location]).unwrap();
        catalog
            .execute("UPDATE installations SET relative_path = ''", [])
            .unwrap();
        let choices = deployment::Choices::new();
        let plan = deployment::plan_with(
            &catalog,
            &id,
            None,
            None,
            &choices,
            &volumes::presence,
            &volumes::space,
        )
        .unwrap();
        assert!(
            plan.installation_path.starts_with(r"\\localhost\"),
            "{}",
            plan.installation_path
        );
        let request = DeployRequest {
            installation_id: &id,
            systems: None,
            fingerprint: &plan.fingerprint,
            choices: &choices,
            collection: None,
            sync: false,
        };
        let summary = deploy(
            &catalog,
            None,
            &request,
            &volumes::presence,
            &volumes::space,
            &mut Quiet,
        )
        .unwrap();
        assert_eq!(
            (summary.status.as_str(), summary.games_copied),
            ("completed", 2),
            "{:?}",
            summary.issues
        );
        assert_eq!(
            fs::read(share.join("roms/snes/Zelda.sfc")).unwrap(),
            b"zelda!"
        );
        // The first copy gave the share an id and recorded it.
        let share_id = volumes::share_id(&unc).expect("share id written");
        let recorded: String = catalog
            .query_row(
                "SELECT stable_id FROM storage_locations WHERE id = 'share'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(recorded, volumes::share_stable_id(&unc, Some(&share_id)));
        assert_eq!(
            volumes::presence(&unc, Some(&recorded)),
            Presence::Connected
        );
        // Another machine answering to the same name (a different id): never copied to.
        fs::write(
            share.join(".retrodrive").join("share-id"),
            "another-batocera",
        )
        .unwrap();
        assert_eq!(
            volumes::presence(&unc, Some(&recorded)),
            Presence::OtherVolume
        );
        let before = snapshot(&share);
        let error = deployment::plan_with(
            &catalog,
            &id,
            None,
            None,
            &choices,
            &volumes::presence,
            &volumes::space,
        )
        .unwrap_err();
        assert!(error.contains("isn't connected"), "{error}");
        assert_eq!(snapshot(&share), before, "nothing was written");
    }

    #[test]
    fn copies_verify_and_write_a_manifest() {
        let mut files = saga();
        let d1 = vec![7u8; 2_500_000];
        let d2 = vec![9u8; 300];
        files.push(("psx/Saga/d1.bin", &d1));
        files.push(("psx/Saga/d2.bin", &d2));
        files.push(("nes/Kid.nes", b"kid"));
        let mut f = Fixture::new(&files);
        let library_before = snapshot(&f.library);
        let summary = f.deploy();

        assert_eq!(summary.status, "completed");
        assert_eq!((summary.games_copied, summary.files_copied), (2, 6));
        assert_eq!(fs::read(f.root.join("roms/psx/Saga/d1.bin")).unwrap(), d1);
        assert_eq!(fs::read(f.root.join("roms/nes/Kid.nes")).unwrap(), b"kid");
        assert!(temps_left(&f.root).is_empty(), "no temporary files remain");
        assert_eq!(
            snapshot(&f.library),
            library_before,
            "sources are never changed"
        );
        assert!(item_states(&f.catalog).iter().all(|s| s == "verified"));
        let hashed: i64 = f
            .catalog
            .query_row(
                "SELECT COUNT(*) FROM game_files WHERE sha256 IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hashed, 6, "verified checksums are kept in the catalog");

        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(summary.manifest_path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(manifest["format"], "retrodrive-deployment-manifest");
        assert_eq!(manifest["items"].as_array().unwrap().len(), 6);
        assert!(manifest["items"][0]["path"]
            .as_str()
            .unwrap()
            .starts_with("roms/"));
        let status: String = f
            .catalog
            .query_row("SELECT status FROM deployments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "completed");

        // Running it again copies nothing: everything is verified as identical.
        let again = f.deploy();
        assert_eq!(
            (
                again.games_copied,
                again.games_already_there,
                again.files_identical
            ),
            (0, 2, 6)
        );
    }

    #[test]
    fn nothing_is_written_when_the_plan_changed_or_space_is_short() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        let stale = f.fingerprint();
        // The library changes after the user reviewed the plan.
        write(&f.library, "nes/Late.nes", b"late");
        scanner::scan(&mut f.catalog, &f.library, &unknown).unwrap();
        let before = snapshot(&f.root);
        let err = deploy(
            &f.catalog,
            None,
            &DeployRequest {
                installation_id: &f.id,
                systems: None,
                fingerprint: &stale,
                choices: &deployment::Choices::new(),
                collection: None,
                sync: false,
            },
            &connected,
            &roomy,
            &mut Quiet,
        )
        .unwrap_err();
        assert!(err.contains("changed since you reviewed"), "{err}");

        let fresh = f.fingerprint();
        let tiny = |_: &Path| {
            Some(VolumeSpace {
                free_bytes: 1 << 20,
                total_bytes: 10 << 30,
                filesystem: Some("NTFS".into()),
                cluster_bytes: 4096,
            })
        };
        let err = deploy(
            &f.catalog,
            None,
            &DeployRequest {
                installation_id: &f.id,
                systems: None,
                fingerprint: &fresh,
                choices: &deployment::Choices::new(),
                collection: None,
                sync: false,
            },
            &connected,
            &tiny,
            &mut Quiet,
        )
        .unwrap_err();
        assert!(err.contains("enough free space"), "{err}");
        let err = deploy(
            &f.catalog,
            None,
            &DeployRequest {
                installation_id: &f.id,
                systems: None,
                fingerprint: &fresh,
                choices: &deployment::Choices::new(),
                collection: None,
                sync: false,
            },
            &connected,
            &|_: &Path| None,
            &mut Quiet,
        )
        .unwrap_err();
        assert!(err.contains("couldn't be measured"), "{err}");
        let err = deploy(
            &f.catalog,
            None,
            &DeployRequest {
                installation_id: &f.id,
                systems: None,
                fingerprint: &fresh,
                choices: &deployment::Choices::new(),
                collection: None,
                sync: false,
            },
            &|_, _| Presence::OtherVolume,
            &roomy,
            &mut Quiet,
        )
        .unwrap_err();
        assert!(
            err.contains("isn't connected"),
            "a different drive at the same letter is refused: {err}"
        );

        assert_eq!(snapshot(&f.root), before, "refusals write nothing");
        let deployments: i64 = f
            .catalog
            .query_row("SELECT COUNT(*) FROM deployments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(deployments, 0);
    }

    /// Many small games, with some multi-disc ones, as owned file contents.
    fn many_games(n: usize) -> Vec<(String, Vec<u8>)> {
        let mut files = Vec::new();
        for i in 0..n {
            files.push((
                format!("nes/Game {i:03}.nes"),
                format!("nes game {i} ").repeat(1 + i % 7).into_bytes(),
            ));
            if i % 10 == 0 {
                files.push((
                    format!("psx/Saga {i}/Saga {i}.m3u"),
                    format!("Saga {i} (Disc 1).cue\nSaga {i} (Disc 2).cue\n").into_bytes(),
                ));
                files.push((
                    format!("psx/Saga {i}/Saga {i} (Disc 1).cue"),
                    format!("FILE \"a{i}.bin\" BINARY\n").into_bytes(),
                ));
                files.push((
                    format!("psx/Saga {i}/Saga {i} (Disc 2).cue"),
                    format!("FILE \"b{i}.bin\" BINARY\n").into_bytes(),
                ));
            }
        }
        files
    }

    struct Workers(usize);
    impl DeployObserver for Workers {
        fn workers(&self) -> usize {
            self.0
        }
    }

    fn destination_files(root: &Path) -> Vec<(String, Vec<u8>)> {
        let roms = root.join("roms");
        walkdir::WalkDir::new(&roms)
            .sort_by_file_name()
            .into_iter()
            .map(|e| e.unwrap())
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                (
                    e.path().strip_prefix(&roms).unwrap().display().to_string(),
                    fs::read(e.path()).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn parallel_and_one_at_a_time_copies_end_the_same() {
        let files = many_games(60);
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_slice()))
            .collect();
        let mut one = Fixture::new(&borrowed);
        let mut four = Fixture::new(&borrowed);
        let a = one.deploy_with(&mut Workers(1)).unwrap();
        let b = four.deploy_with(&mut Workers(4)).unwrap();
        for s in [&a, &b] {
            assert_eq!(s.status, "completed");
            assert_eq!(
                (s.games_copied, s.files_copied, s.files_failed),
                (66, 78, 0)
            );
        }
        assert_eq!(a.bytes_copied, b.bytes_copied);
        assert_eq!(
            destination_files(&one.root),
            destination_files(&four.root),
            "same files, same contents"
        );
        assert!(temps_left(&four.root).is_empty());
        assert!(item_states(&four.catalog).iter().all(|s| s == "verified"));
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(b.manifest_path.as_ref().unwrap()).unwrap()).unwrap();
        assert_eq!(manifest["items"].as_array().unwrap().len(), 78);
    }

    /// Cancels once `after` files have reached their final rename.
    struct CancelLater {
        after: usize,
        renames: usize,
    }
    impl DeployObserver for CancelLater {
        fn cancelled(&mut self) -> bool {
            self.renames >= self.after
        }
        fn before_rename(&mut self, _destination: &Path) {
            self.renames += 1;
        }
    }

    #[test]
    fn cancelling_a_parallel_copy_leaves_only_verified_files() {
        let files = many_games(80);
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_slice()))
            .collect();
        let mut f = Fixture::new(&borrowed);
        let summary = f
            .deploy_with(&mut CancelLater {
                after: 10,
                renames: 0,
            })
            .unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.status, "cancelled");
        assert!(
            temps_left(&f.root).is_empty(),
            "no unfinished temporary files"
        );
        let states = item_states(&f.catalog);
        assert!(
            states
                .iter()
                .all(|s| ["verified", "cancelled", "not_attempted"].contains(&s.as_str())),
            "{states:?}"
        );
        let verified = states.iter().filter(|s| *s == "verified").count();
        assert!((10..80).contains(&verified), "stopped partway: {verified}");
        // Every file on the drive is a verified, complete copy.
        let mut s = f.catalog.prepare("SELECT destination_path, source_path FROM deployment_items WHERE state = 'verified'").unwrap();
        let pairs: Vec<(String, String)> = s
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            destination_files(&f.root).len(),
            pairs.len(),
            "nothing else was left on the drive"
        );
        for (destination, source) in pairs {
            assert_eq!(fs::read(&destination).unwrap(), fs::read(&source).unwrap());
        }
    }

    /// Writes "theirs" at one destination just before its rename.
    struct IntruderAt(&'static str);
    impl DeployObserver for IntruderAt {
        fn before_rename(&mut self, destination: &Path) {
            if destination.ends_with(self.0) {
                fs::write(destination, b"theirs").unwrap();
            }
        }
    }

    #[test]
    fn a_parallel_copy_never_overwrites_a_file_that_appears() {
        let files = many_games(30);
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_slice()))
            .collect();
        let mut f = Fixture::new(&borrowed);
        let summary = f.deploy_with(&mut IntruderAt("Game 017.nes")).unwrap();
        assert_eq!(
            fs::read(f.root.join("roms/nes/Game 017.nes")).unwrap(),
            b"theirs"
        );
        assert_eq!((summary.games_incomplete, summary.games_copied), (1, 32));
        assert_eq!(summary.status, "completed_with_issues");
        assert!(temps_left(&f.root).is_empty());
    }

    /// Before each big file is read back, checks no other big file is being written.
    struct BigWatcher {
        root: PathBuf,
        checks: usize,
    }
    impl DeployObserver for BigWatcher {
        fn before_verify(&mut self, temp: &Path, _source: &Path) {
            if fs::metadata(temp)
                .map(|m| m.len() as i64 >= BIG_FILE)
                .unwrap_or(false)
            {
                self.checks += 1;
                // Other workers create and remove files meanwhile, so skip entries that vanish.
                let others = walkdir::WalkDir::new(&self.root)
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    // Compare names, not paths: temp folders can show up as 8.3 short paths (CI).
                    .filter(|name| {
                        Some(name.as_str()) != temp.file_name().and_then(|n| n.to_str())
                            && name.contains(TEMP_MARKER)
                            && name.contains("Big")
                    })
                    .count();
                assert_eq!(
                    others, 0,
                    "another big file was being written at the same time"
                );
            }
        }
    }

    #[test]
    fn big_files_are_copied_one_at_a_time() {
        let big = vec![7u8; BIG_FILE as usize];
        let mut files: Vec<(String, Vec<u8>)> = (0..3)
            .map(|i| (format!("psx/Big {i}.iso"), big.clone()))
            .collect();
        files.extend(many_games(12));
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_slice()))
            .collect();
        let mut f = Fixture::new(&borrowed);
        let root = f.root.clone();
        let mut watcher = BigWatcher { root, checks: 0 };
        let summary = f.deploy_with(&mut watcher).unwrap();
        assert_eq!(summary.status, "completed");
        assert_eq!(watcher.checks, 3);
    }

    struct PanicsAtRename;
    impl DeployObserver for PanicsAtRename {
        fn before_rename(&mut self, _destination: &Path) {
            panic!("observer failure");
        }
    }

    /// A failure on the coordinating thread must end the copy, never leave workers waiting.
    #[test]
    #[should_panic(expected = "observer failure")]
    fn a_coordinator_failure_stops_the_workers_instead_of_hanging() {
        let files = many_games(20);
        let borrowed: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_slice()))
            .collect();
        let mut f = Fixture::new(&borrowed);
        let _ = f.deploy_with(&mut PanicsAtRename);
    }

    struct Intruder;
    impl DeployObserver for Intruder {
        fn before_rename(&mut self, destination: &Path) {
            fs::write(destination, b"theirs").unwrap();
        }
    }

    #[test]
    fn a_file_that_appears_before_the_rename_is_never_overwritten() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"mine")]);
        let summary = f.deploy_with(&mut Intruder).unwrap();
        assert_eq!(
            fs::read(f.root.join("roms/nes/Kid.nes")).unwrap(),
            b"theirs"
        );
        assert_eq!(summary.games_incomplete, 1);
        assert_eq!(summary.status, "completed_with_issues");
        assert!(summary.issues[0].message.contains("It was kept"));
        assert!(temps_left(&f.root).is_empty());
    }

    #[test]
    fn rename_never_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::write(&a, b"new").unwrap();
        fs::write(&b, b"existing").unwrap();
        assert_eq!(
            rename_no_replace(&a, &b).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&b).unwrap(), b"existing");
        assert!(a.exists());
    }

    struct Corrupter;
    impl DeployObserver for Corrupter {
        fn before_verify(&mut self, temp: &Path, _source: &Path) {
            let mut bytes = fs::read(temp).unwrap();
            bytes[0] ^= 0xff;
            fs::write(temp, bytes).unwrap();
        }
    }

    #[test]
    fn a_copy_that_does_not_verify_is_discarded() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        let summary = f.deploy_with(&mut Corrupter).unwrap();
        assert!(
            !f.root.join("roms/nes/Kid.nes").exists(),
            "an unverified copy never gets the real name"
        );
        assert!(temps_left(&f.root).is_empty());
        assert_eq!(summary.files_failed, 1);
        assert!(summary.issues[0].message.contains("didn't match"));
        assert_eq!(item_states(&f.catalog), ["failed"]);
    }

    struct SourceEditor;
    impl DeployObserver for SourceEditor {
        fn before_verify(&mut self, _temp: &Path, source: &Path) {
            fs::write(source, b"edited while copying").unwrap();
        }
    }

    #[test]
    fn a_source_that_changes_during_the_copy_fails_that_file() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid"), ("snes/Quest.sfc", b"quest")]);
        let summary = f.deploy_with(&mut SourceEditor).unwrap();
        // The edit happens after streaming; the source re-check catches it.
        assert_eq!(summary.files_failed, 2);
        assert!(temps_left(&f.root).is_empty());
        assert!(!f.root.join("roms/nes/Kid.nes").exists());
    }

    struct CancelAfter(usize);
    impl DeployObserver for CancelAfter {
        fn workers(&self) -> usize {
            1
        }
        fn cancelled(&mut self) -> bool {
            if self.0 == 0 {
                return true;
            }
            self.0 -= 1;
            false
        }
    }

    #[test]
    fn cancelling_mid_file_leaves_no_partial_files_and_no_false_completion() {
        let big = vec![1u8; 3 * CHUNK + 10];
        let mut f = Fixture::new(&[("psx/Big.iso", &big), ("snes/Zed.sfc", b"zed")]);
        // Calls: game start, copy start, then one per chunk: cancel inside the copy.
        let summary = f.deploy_with(&mut CancelAfter(4)).unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.status, "cancelled");
        assert!(!f.root.join("roms/psx/Big.iso").exists());
        assert!(
            temps_left(&f.root).is_empty(),
            "the unfinished temporary file was removed"
        );
        assert_eq!(item_states(&f.catalog), ["cancelled", "not_attempted"]);
        assert_eq!(summary.files_not_attempted, 1);
    }

    #[test]
    fn already_there_files_are_compared_by_content() {
        let mut f = Fixture::new(&[("nes/Same.nes", b"same"), ("nes/Looks.nes", b"mine")]);
        write(&f.root, "roms/nes/Same.nes", b"same");
        write(&f.root, "roms/nes/Looks.nes", b"MINE");
        let summary = f.deploy();
        assert_eq!(summary.games_already_there, 1);
        assert_eq!(summary.games_conflict, 1);
        assert_eq!(
            fs::read(f.root.join("roms/nes/Looks.nes")).unwrap(),
            b"MINE",
            "the existing file is kept"
        );
        assert!(summary
            .issues
            .iter()
            .any(|i| i.message.contains("contents differ")));
    }

    #[test]
    fn a_later_copy_trusts_earlier_checks_only_while_nothing_changed() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid"), ("nes/Zelda.nes", b"zelda")]);
        assert_eq!(f.deploy().files_copied, 2);
        let reused = |c: &Connection| -> i64 {
            c.query_row("SELECT COUNT(*) FROM deployment_items WHERE message LIKE 'Confirmed by an earlier copy%'", [], |r| r.get(0)).unwrap()
        };
        // A confirmation in the same second as the files last changed isn't trusted: checked again by content.
        // (Pinned to the files' own latest time, so the result doesn't depend on the clock.)
        let latest = ["Kid.nes", "Zelda.nes"]
            .iter()
            .flat_map(|n| {
                [
                    f.root.join("roms/nes").join(n),
                    f.library.join("nes").join(n),
                ]
            })
            .flat_map(|p| {
                let m = fs::metadata(&p).unwrap();
                [m.modified().ok(), m.created().ok()]
            })
            .flatten()
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .max()
            .unwrap();
        f.catalog
            .execute(
                "UPDATE deployment_items SET updated_at = datetime(?1, 'unixepoch')",
                [latest],
            )
            .unwrap();
        assert_eq!(f.deploy().files_identical, 2);
        assert_eq!(
            reused(&f.catalog),
            0,
            "a confirmation isn't trusted for files modified in the same second"
        );
        // Confirmations made after the files last changed are trusted (no reading).
        f.catalog
            .execute(
                "UPDATE deployment_items SET updated_at = '2999-01-01 00:00:00'",
                [],
            )
            .unwrap();
        assert_eq!(f.deploy().files_identical, 2);
        assert_eq!(reused(&f.catalog), 2);
        // A file changed on the drive after its confirmation (same size) is compared again and kept.
        f.catalog
            .execute(
                "UPDATE deployment_items SET updated_at = '2000-01-01 00:00:00'",
                [],
            )
            .unwrap();
        write(&f.root, "roms/nes/Zelda.nes", b"ZELDA");
        let after = f.deploy();
        assert_eq!(
            (after.files_identical, after.games_conflict),
            (1, 1),
            "the change is noticed"
        );
        assert_eq!(
            fs::read(f.root.join("roms/nes/Zelda.nes")).unwrap(),
            b"ZELDA",
            "and the drive's file is kept"
        );
        assert_eq!(unix_seconds("1970-01-02 00:00:01"), Some(86_401));
        assert_eq!(unix_seconds("2026-11-07 12:00:00"), Some(1_794_052_800));
    }

    #[test]
    fn a_partly_copied_game_is_finished_by_running_again() {
        let mut files = saga();
        files.push(("psx/Saga/d1.bin", b"disc one"));
        files.push(("psx/Saga/d2.bin", b"disc two"));
        let mut f = Fixture::new(&files);
        struct FailDiscTwo;
        impl DeployObserver for FailDiscTwo {
            fn before_verify(&mut self, temp: &Path, _source: &Path) {
                if temp.display().to_string().contains("d2.bin") {
                    fs::write(temp, b"corrupt!").unwrap();
                }
            }
        }
        let first = f.deploy_with(&mut FailDiscTwo).unwrap();
        assert_eq!(first.games_incomplete, 1);
        assert!(
            f.root.join("roms/psx/Saga/d1.bin").exists(),
            "verified discs are kept"
        );
        assert!(!f.root.join("roms/psx/Saga/d2.bin").exists());

        let second = f.deploy();
        assert_eq!(second.status, "completed");
        assert_eq!(second.games_copied, 1);
        assert_eq!(second.files_copied, 1, "only the missing disc is copied");
        assert_eq!(second.files_identical, 4, "the rest are checked by content");
        assert_eq!(
            fs::read(f.root.join("roms/psx/Saga/d2.bin")).unwrap(),
            b"disc two"
        );
    }

    #[test]
    fn a_hundred_unicode_names_with_spaces_copy_and_verify() {
        let names: Vec<String> = (0..100)
            .map(|i| format!("nes/Gämé Nº {i} – テスト.nes"))
            .collect();
        let contents: Vec<Vec<u8>> = (0..100).map(|i| format!("rom {i}").into_bytes()).collect();
        let files: Vec<(&str, &[u8])> = names
            .iter()
            .zip(&contents)
            .map(|(n, c)| (n.as_str(), c.as_slice()))
            .collect();
        let mut f = Fixture::new(&files);
        let summary = f.deploy();
        assert_eq!(
            (summary.status.as_str(), summary.files_copied),
            ("completed", 100)
        );
        for (name, c) in names.iter().zip(&contents) {
            assert_eq!(&fs::read(f.root.join("roms").join(name)).unwrap(), c);
        }
    }

    #[test]
    fn paths_longer_than_260_characters_are_copied() {
        let name = format!("nes/{}.nes", "Long Name ".repeat(22));
        let mut f = Fixture::new(&[(name.as_str(), b"long")]);
        let summary = f.deploy();
        let dest = f.root.join("roms").join(&name);
        assert!(dest.display().to_string().len() > 260, "{}", dest.display());
        assert_eq!(summary.files_copied, 1);
        assert_eq!(fs::read(dest).unwrap(), b"long");
    }

    #[test]
    fn interrupted_copies_are_cleaned_up_safely_on_the_next_start() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        f.deploy();
        let deployment: String = f
            .catalog
            .query_row("SELECT id FROM deployments", [], |r| r.get(0))
            .unwrap();
        let marker = format!("{TEMP_MARKER}{}", &deployment[..8]);
        // Simulate a crash mid-copy: a running deployment with a recorded temp file.
        // Use the canonical installation path, as RetroDrive records it (CI temp dirs use 8.3 short names).
        let root = crate::paths::normalize(&f.root).unwrap();
        let temp = root.join("roms/nes").join(format!("Half.nes{marker}"));
        fs::write(&temp, b"half").unwrap();
        let precious = root.join("roms/nes/Kid.nes");
        let outside = f.dir.path().join(format!("outside.nes{marker}"));
        fs::write(&outside, b"not in the installation").unwrap();
        f.catalog
            .execute("UPDATE deployments SET status = 'running'", [])
            .unwrap();
        for path in [&temp, &precious, &outside] {
            f.catalog
                .execute(
                    "INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, temp_path, size_bytes, planned_action, state)
                     VALUES (?1, 0, 'x', 'x', 'x', ?2, 4, 'copy', 'copying')",
                    params![deployment, path.display().to_string()],
                )
                .unwrap();
        }

        assert_eq!(recover(&f.catalog).unwrap(), 1);
        // Drive not confirmed: nothing is touched.
        assert_eq!(
            cleanup_leftovers(&f.catalog, &|_, _| Presence::OtherVolume).unwrap(),
            0
        );
        assert!(temp.exists());
        assert_eq!(cleanup_leftovers(&f.catalog, &connected).unwrap(), 1);
        assert!(!temp.exists(), "our own leftover is removed");
        assert!(
            precious.exists(),
            "a path without our marker is never removed"
        );
        assert!(
            outside.exists(),
            "a path outside the installation is never removed"
        );
        let status: String = f
            .catalog
            .query_row("SELECT status FROM deployments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "interrupted");
    }

    #[cfg(windows)]
    #[test]
    fn inside_checks_work_for_drive_roots_and_folders() {
        assert!(is_inside(r"E:\roms\nes\Kid.nes.rdpart-1", r"E:\"));
        assert!(is_inside(r"C:\Games\RetroBat\roms\x", r"c:\games\retrobat"));
        assert!(!is_inside(
            r"C:\Games\RetroBatOther\x",
            r"C:\Games\RetroBat"
        ));
        assert!(!is_inside(r"C:\Games\RetroBat", r"C:\Games\RetroBat"));
        assert!(!is_inside(r"D:\x", r"E:\"));
    }

    #[test]
    fn reading_back_past_the_cache_matches_a_normal_read() {
        let dir = tempfile::tempdir().unwrap();
        for size in [0usize, 5, CHUNK, CHUNK + 123, 3 * CHUNK + 4097] {
            let path = dir.path().join(format!("f{size}"));
            fs::write(
                &path,
                (0..size).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
            )
            .unwrap();
            assert_eq!(
                hash_file_from_disk(&path, &mut || false).ok(),
                hash_file(&path, &mut || false).ok(),
                "size {size}"
            );
        }
    }

    #[test]
    fn content_comparison_spots_a_single_changed_byte() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..CHUNK + 10).map(|i| (i % 7) as u8).collect();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::write(&a, &data).unwrap();
        fs::write(&b, &data).unwrap();
        assert_eq!(
            same_contents(&a, &b, &mut || false).ok().flatten(),
            hash_file(&a, &mut || false).ok()
        );
        let mut changed = data.clone();
        *changed.last_mut().unwrap() ^= 1;
        fs::write(&b, &changed).unwrap();
        assert_eq!(same_contents(&a, &b, &mut || false).ok().flatten(), None);
    }

    /// A temporary file that can't be removed (here: held open without delete
    /// sharing, like an antivirus scan) stays recorded, so it's cleaned up later.
    #[cfg(windows)]
    #[test]
    fn a_temp_file_that_cannot_be_removed_stays_tracked() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        struct Locker(Option<File>);
        impl DeployObserver for Locker {
            fn before_verify(&mut self, temp: &Path, _source: &Path) {
                fs::write(temp, b"bad").unwrap();
                // FILE_SHARE_READ | FILE_SHARE_WRITE, but not FILE_SHARE_DELETE.
                self.0 = Some(
                    OpenOptions::new()
                        .read(true)
                        .share_mode(1 | 2)
                        .open(temp)
                        .unwrap(),
                );
            }
        }
        let mut locker = Locker(None);
        let summary = f.deploy_with(&mut locker).unwrap();
        assert_eq!(summary.files_failed, 1);
        let temp: Option<String> = f
            .catalog
            .query_row("SELECT temp_path FROM deployment_items", [], |r| r.get(0))
            .unwrap();
        let temp = PathBuf::from(temp.expect("the leftover is still recorded"));
        assert!(temp.exists());
        assert!(!f.root.join("roms/nes/Kid.nes").exists());
        drop(locker);
        assert_eq!(cleanup_leftovers(&f.catalog, &connected).unwrap(), 1);
        assert!(!temp.exists());
    }

    #[cfg(windows)]
    #[test]
    fn copies_never_go_through_a_junction() {
        let f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        let fingerprint = f.fingerprint();
        let elsewhere = f.dir.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        // The junction appears after the plan was reviewed.
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(f.root.join("roms").join("nes"))
            .arg(&elsewhere)
            .output()
            .unwrap();
        assert!(made.status.success());
        let result = deploy(
            &f.catalog,
            None,
            &DeployRequest {
                installation_id: &f.id,
                systems: None,
                fingerprint: &fingerprint,
                choices: &deployment::Choices::new(),
                collection: None,
                sync: false,
            },
            &connected,
            &roomy,
            &mut Quiet,
        );
        // Either the changed plan is refused, or the copy refuses the link; nothing lands elsewhere.
        if let Ok(summary) = result {
            assert_eq!(summary.files_copied, 0);
        }
        assert_eq!(
            fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "nothing was written through the junction"
        );
    }

    fn conflict_fixture() -> Fixture {
        let f = Fixture::new(&[("nes/Diff.nes", b"mine")]);
        write(&f.root, "roms/nes/Diff.nes", b"theirs, longer");
        f
    }

    #[test]
    fn replace_moves_the_original_to_the_backup_folder_then_copies() {
        let mut f = conflict_fixture();
        f.choices
            .insert("roms/nes/diff.nes".into(), deployment::Choice::Replace);
        let summary = f.deploy();
        assert_eq!(summary.status, "completed", "{:?}", summary.issues);
        assert_eq!(summary.files_replaced, 1);
        assert_eq!(fs::read(f.root.join("roms/nes/Diff.nes")).unwrap(), b"mine");
        let backup = f
            .root
            .join(".retrodrive/backup")
            .join(&summary.deployment_id)
            .join("roms/nes/Diff.nes");
        assert_eq!(
            fs::read(&backup).unwrap(),
            b"theirs, longer",
            "the original is kept, never deleted"
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(summary.manifest_path.unwrap()).unwrap()).unwrap();
        assert_eq!(manifest["items"][0]["outcome"], "replaced");
        assert!(manifest["items"][0]["backup"]
            .as_str()
            .unwrap()
            .starts_with(".retrodrive/backup/"));
        assert!(temps_left(&f.root).is_empty());
    }

    #[test]
    fn a_failed_replace_puts_the_original_back() {
        let mut f = conflict_fixture();
        f.choices
            .insert("roms/nes/diff.nes".into(), deployment::Choice::Replace);
        let summary = f.deploy_with(&mut Corrupter).unwrap();
        assert_eq!(summary.files_failed, 1);
        assert!(
            summary.issues[0].message.contains("put back"),
            "{:?}",
            summary.issues
        );
        assert_eq!(
            fs::read(f.root.join("roms/nes/Diff.nes")).unwrap(),
            b"theirs, longer",
            "the original is back under its name"
        );
        assert!(temps_left(&f.root).is_empty());
    }

    #[test]
    fn cancelling_during_a_replace_puts_the_original_back() {
        let mut f = conflict_fixture();
        f.choices
            .insert("roms/nes/diff.nes".into(), deployment::Choice::Replace);
        struct CancelAfterBackup(bool);
        impl DeployObserver for CancelAfterBackup {
            fn cancelled(&mut self) -> bool {
                self.0
            }
            fn after_backup(&mut self, _destination: &Path) {
                self.0 = true;
            }
        }
        let summary = f.deploy_with(&mut CancelAfterBackup(false)).unwrap();
        assert!(summary.cancelled);
        assert_eq!(
            fs::read(f.root.join("roms/nes/Diff.nes")).unwrap(),
            b"theirs, longer"
        );
    }

    #[test]
    fn keep_both_copies_under_the_next_free_number() {
        let mut f = conflict_fixture();
        write(&f.root, "roms/nes/Diff (2).nes", b"someone else's (2)");
        f.choices
            .insert("roms/nes/diff.nes".into(), deployment::Choice::KeepBoth);
        let summary = f.deploy();
        assert_eq!(summary.files_copied, 1, "{:?}", summary.issues);
        assert_eq!(
            fs::read(f.root.join("roms/nes/Diff.nes")).unwrap(),
            b"theirs, longer",
            "the existing file is kept"
        );
        assert_eq!(
            fs::read(f.root.join("roms/nes/Diff (2).nes")).unwrap(),
            b"someone else's (2)",
            "a taken number is skipped"
        );
        assert_eq!(
            fs::read(f.root.join("roms/nes/Diff (3).nes")).unwrap(),
            b"mine"
        );
    }

    #[test]
    fn an_interrupted_replace_is_put_right_on_the_next_start() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid"), ("nes/Done.nes", b"new")]);
        f.deploy();
        let deployment: String = f
            .catalog
            .query_row("SELECT id FROM deployments", [], |r| r.get(0))
            .unwrap();
        let root = crate::paths::normalize(&f.root).unwrap();
        // Crash 1: the original was moved to the backup folder, the new file never arrived.
        let dest = root.join("roms/nes/Lost.nes");
        let backup = root
            .join(".retrodrive/backup")
            .join(&deployment)
            .join("roms/nes/Lost.nes");
        write(
            &root,
            &format!(".retrodrive/backup/{deployment}/roms/nes/Lost.nes"),
            b"original",
        );
        // Crash 2: the new file did get its name before the crash.
        let done_backup = root
            .join(".retrodrive/backup")
            .join(&deployment)
            .join("roms/nes/Done.nes");
        write(
            &root,
            &format!(".retrodrive/backup/{deployment}/roms/nes/Done.nes"),
            b"old done",
        );
        f.catalog
            .execute("UPDATE deployments SET status = 'running'", [])
            .unwrap();
        for (d, b) in [
            (&dest, &backup),
            (&root.join("roms/nes/Done.nes"), &done_backup),
        ] {
            f.catalog
                .execute(
                    "INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, backup_path, size_bytes, planned_action, state)
                     VALUES (?1, 0, 'x', 'x', ?2, ?3, 3, 'replace', 'replacing')",
                    params![deployment, d.display().to_string(), b.display().to_string()],
                )
                .unwrap();
        }
        recover(&f.catalog).unwrap();
        assert_eq!(
            cleanup_leftovers(&f.catalog, &|_, _| Presence::OtherVolume).unwrap(),
            0,
            "nothing moves on an unconfirmed drive"
        );
        assert!(!dest.exists());
        assert_eq!(cleanup_leftovers(&f.catalog, &connected).unwrap(), 1);
        assert_eq!(
            fs::read(&dest).unwrap(),
            b"original",
            "the original is back"
        );
        assert!(done_backup.exists(), "a completed replace keeps its backup");
        let states: Vec<String> = f
            .catalog
            .prepare(
                "SELECT state FROM deployment_items WHERE planned_action = 'replace' ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(states, ["restored", "replaced"]);
    }

    /// Measures copying many small files with a real on-disk catalog (docs/15).
    /// Run: cargo test --release copy_benchmark_small_files -- --ignored --nocapture
    #[test]
    #[ignore]
    fn copy_benchmark_small_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("RetroBat");
        for d in ["emulationstation", "emulators", "roms", "bios"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("RetroBat.exe"), b"").unwrap();
        let library = dir.path().join("library/nes");
        fs::create_dir_all(&library).unwrap();
        let count = 2000;
        for i in 0..count {
            fs::write(
                library.join(format!("Game {i:05} (USA).nes")),
                vec![(i % 251) as u8; 40 * 1024],
            )
            .unwrap();
        }
        let db_path = dir.path().join("catalog.sqlite");
        let mut catalog = db::open(&db_path).unwrap();
        db::migrate(&mut catalog, None).unwrap();
        scanner::scan(&mut catalog, &dir.path().join("library"), &unknown).unwrap();
        let id = installations::add(&catalog, &root, "retrobat", &unknown).unwrap();
        let choices = deployment::Choices::new();
        let fingerprint =
            deployment::plan_with(&catalog, &id, None, None, &choices, &connected, &roomy)
                .unwrap()
                .fingerprint;
        // RETRODRIVE_BENCH_WORKERS picks how many games are copied at once (default: the app's).
        let workers = std::env::var("RETRODRIVE_BENCH_WORKERS")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(DEFAULT_WORKERS);
        let started = std::time::Instant::now();
        let summary = deploy(
            &catalog,
            None,
            &DeployRequest {
                installation_id: &id,
                systems: None,
                fingerprint: &fingerprint,
                choices: &choices,
                collection: None,
                sync: false,
            },
            &connected,
            &roomy,
            &mut Workers(workers),
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(summary.files_copied, count);
        eprintln!(
            "{workers} worker(s): copied {count} x 40 KB files in {:?} ({:.1} files/s)",
            elapsed,
            count as f64 / elapsed.as_secs_f64()
        );
    }

    #[test]
    fn leftovers_are_found_by_name_even_without_journal_entries() {
        let mut f = Fixture::new(&[("nes/Kid.nes", b"kid")]);
        f.deploy();
        let deployment: String = f
            .catalog
            .query_row("SELECT id FROM deployments", [], |r| r.get(0))
            .unwrap();
        let marker = format!("{TEMP_MARKER}{}", &deployment[..8]);
        let root = crate::paths::normalize(&f.root).unwrap();
        // A power loss dropped the journal's temp and backup paths.
        let lost_dest = root.join("roms").join("nes").join("Half.nes");
        let temp = PathBuf::from(format!("{}{marker}", lost_dest.display()));
        fs::write(&temp, b"half").unwrap();
        let replaced_dest = root.join("roms").join("nes").join("Orig.nes");
        let backup = root
            .join(".retrodrive")
            .join("backup")
            .join(&deployment)
            .join("roms")
            .join("nes")
            .join("Orig.nes");
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        fs::write(&backup, b"original").unwrap();
        f.catalog
            .execute("UPDATE deployments SET status = 'running'", [])
            .unwrap();
        for (dest, planned) in [(&lost_dest, "copy"), (&replaced_dest, "replace")] {
            f.catalog
                .execute(
                    "INSERT INTO deployment_items (deployment_id, game_index, game_title, source_path, destination_path, size_bytes, planned_action, state)
                     VALUES (?1, 0, 'x', 'x', ?2, 4, ?3, 'pending')",
                    params![deployment, dest.display().to_string(), planned],
                )
                .unwrap();
        }
        recover(&f.catalog).unwrap();
        assert_eq!(
            cleanup_leftovers(&f.catalog, &|_, _| Presence::OtherVolume).unwrap(),
            0
        );
        assert!(temp.exists() && backup.exists());
        assert_eq!(cleanup_leftovers(&f.catalog, &connected).unwrap(), 2);
        assert!(
            !temp.exists(),
            "our own temporary file is found by its marked name"
        );
        assert_eq!(
            fs::read(&replaced_dest).unwrap(),
            b"original",
            "the original comes back from the backup folder"
        );
        assert!(
            root.join("roms/nes/Kid.nes").exists(),
            "verified copies are untouched"
        );
    }
}
