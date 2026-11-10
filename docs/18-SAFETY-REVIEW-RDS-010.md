# 18 | Safety Review: First Disk Writes (RDS-010)

This is the dedicated safety review that AGENTS.md requires before RetroDrive writes to a user's drive. It covers the staged copier in `src-tauri/src/copier.rs` and its wiring in `lib.rs`. Raw disk imaging, formatting and partitioning are **out of scope** and still forbidden (docs/10).

_Review date: 2026-11-05. Status: **implemented and tested; owner sign-off pending** (see §6)._

## 1. Scope and authorization

| Item | Decision |
|---|---|
| What may be written | Game files from the reviewed deployment plan, inside one saved installation folder; folders needed for them; one manifest file per copy at `<installation>/.retrodrive/deployments/<id>.json` |
| What may be removed | Only temporary files RetroDrive created (`<name>.rdpart-<deployment id prefix>`), either in the same run or, after a crash, when the journal records them and the drive is confirmed |
| Never | Overwriting or deleting any other file; writing outside the installation; writing through links or junctions; changing source files; elevated privileges |
| Who starts it | The user, after reviewing the plan and confirming a summary dialog (“Yes, copy now”) |
| Authorized by | Project owner, 2026-11-05 (design approved in conversation, including: keep verified files of a partly copied game and let a retry finish it; manifest in the catalog **and** as a file in the installation; the `sha2` dependency) |

## 2. Hazards, mitigations and evidence

| # | Hazard | Mitigation | Test (all in `copier::tests` unless noted) |
|---|---|---|---|
| H1 | The drive or library changed after the user reviewed the plan | The plan is rebuilt and its fingerprint must match before anything is written | `nothing_is_written_when_the_plan_changed_or_space_is_short`; manual: a stale plan was refused in the app |
| H2 | A different drive now has the same letter (QA-015) | The volume fingerprint is checked before starting **and before every game** | same test (`OtherVolume` refused) |
| H3 | Not enough space (QA-013) | Capacity re-measured; refuses if it doesn't fit **or can't be measured** | same test |
| H4 | Overwriting a user's file | Temp files are opened with `create_new`; the final rename uses `MoveFileExW` **without** `MOVEFILE_REPLACE_EXISTING`, so it fails if the name exists. `std::fs::rename` is never used, because it replaces files on Windows | `rename_never_replaces`, `a_file_that_appears_before_the_rename_is_never_overwritten` |
| H5 | A file that only looks identical (same name and size) is skipped | Contents are compared side by side before skipping; a difference is a conflict and the existing file is kept | `already_there_files_are_compared_by_content`, `content_comparison_spots_a_single_changed_byte` |
| H6 | Corrupt copy marked complete | SHA-256 while copying, `sync_all`, then read back **past the Windows file cache** (`FILE_FLAG_NO_BUFFERING`) and compared; only then renamed and marked verified | `a_copy_that_does_not_verify_is_discarded`, `reading_back_past_the_cache_matches_a_normal_read` |
| H7 | Source changes during the copy | Size and modified time checked before and after; mismatch fails the file | `a_source_that_changes_during_the_copy_fails_that_file` |
| H8 | Cancel or crash leaves half-written files (QA-012) | The journal records the temp path before the file is created. Cancel removes the in-progress temp. After a crash, startup marks the deployment interrupted and removes recorded temps only on the confirmed drive, only inside that installation, only with that deployment's marker | `cancelling_mid_file_leaves_no_partial_files_and_no_false_completion`, `interrupted_copies_are_cleaned_up_safely_on_the_next_start`; manual: cancelled a 1 GB copy in the app, no temp left |
| H9 | A temp file can't be removed (antivirus lock, unplugged drive) and is forgotten | Only "not found" counts as gone; otherwise the path stays in the journal for later cleanup | `a_temp_file_that_cannot_be_removed_stays_tracked` |
| H10 | Writing through a junction or symlink, or outside the installation (QA-014/016) | Plan blocks them; the copier re-checks every folder level before creating it and refuses links, files in the way and `..`/absolute parts | `copies_never_go_through_a_junction`; `deployment::tests::unsafe_relative_paths_are_rejected` |
| H11 | A partly copied multi-disc game | Verified files are kept, the game is reported incomplete, and running the copy again checks them by content and copies only what's missing (owner decision) | `a_partly_copied_game_is_finished_by_running_again` |
| H12 | Unicode, spaces and long paths | Paths are handled by the standard library; direct Win32 calls use `\\?\` paths | `a_hundred_unicode_names_with_spaces_copy_and_verify`, `paths_longer_than_260_characters_are_copied` |
| H13 | An internal error after copying started is reported as "nothing copied" | Errors after the journal starts mark the deployment `failed`, clean up temps and say that verified files were kept | code review fix (lib.rs/copier.rs) |
| H14 | The UI shows a copy running forever after an instant refusal | Finished job ids are remembered so a late start reply can't resurrect the progress bar | code review fix; manual: stale-plan refusal in the app showed no stuck bar |

## 3. Independent review

A separate high-effort code review (Claude Code `/code-review`) was run on the full change before the PR. It reported 10 findings, all addressed:

| Finding | Resolution |
|---|---|
| Temp reported as removed when it couldn't be checked | Only `NotFound` counts as gone (H9) |
| A kept temp path was erased by the next journal update | `set_item` takes an explicit keep / set / clear for the temp path |
| UI could stay on "Copying" after an instant refusal | Finished-job tracking (H14) |
| Drive-root installations: leftover temps never cleaned | Component-wise `is_inside` check, unit-tested with `E:\` |
| Re-checking "already there" read both files in full twice | Side-by-side comparison that stops at the first difference |
| Errors mid-copy reported as "nothing was copied" | H13 |
| Unreadable existing file reported as "differs" | Reported as "couldn't be read; try again", counted incomplete |
| Progress bar could stall below 100% | Each file advances progress by its full size |
| Read-back could be served from the file cache | Unbuffered read-back (H6) |
| Docs not updated | This document plus docs/04, 05, 08, 11, 13, 14, 15, 17, Features.md, README |

## 4. Residual risks (accepted for now)

- **Drive write caches.** The unbuffered read-back bypasses Windows' cache, but a USB device's own cache could still answer it. Unplugging without "Safely remove" right after a copy is the remaining risk; the next copy's content check would detect a damaged file and report it as a conflict, never overwrite it.
- **Unbuffered reads unsupported.** On filesystems that refuse them, the read-back falls back to a normal read.
- **Re-check cost.** "Already there" files are read in full on both sides each run (that is the price of never trusting size alone).
- **Hard crash.** Temporary files from a crash stay on the drive until RetroDrive next starts with that drive connected.
- **Folders.** Folders created for a copy that later fails are left in place (empty); nothing is deleted.
- **Failing drives.** A drive with bad blocks can hang a write inside Windows; Cancel then waits until Windows gives up. Nothing is marked verified without a successful read-back.
- **Unplug test waived** (owner decision, 2026-11-05).

## 5. Test record

- `cargo test`: 105 passing (17 in `copier::tests`), `cargo clippy -D warnings` clean, `npm run build` OK, license gate passed (2026-11-05, Windows 11).
- Manual in the app with a mock RetroBat (2026-11-05):
  - **Copy:** 3 games (7 files, 67 MB) copied and verified, and 1 game already there was checked by content. All 7 files were hash-identical to their sources, the conflicting `Diff.nes` was untouched, no temporary files were left, and the manifest listed 8 items.
  - **Stale plan:** starting a copy from a plan built before a library rescan was refused with "nothing was copied", and no progress bar got stuck.
  - **Cancel:** a 1 GB copy was cancelled at about 160 MB. The temporary file was removed, `Big.iso` never appeared, and earlier copies were intact.

### Real-world run (owner, 2026-11-05)
- RetroBat 8.3.1.1 installed by the owner on a healthy 2 TB exFAT USB drive (`G:`). 3,436 NES games copied from a NAS library: **copy finished, every file verified**; independently re-checked: 3,436 files / 351.1 MB on the drive, no `.rdpart-` files, manifest shows 3,436 verified, 5 random copies re-hashed equal to the recorded SHA-256.
- The plan correctly accounted for exFAT's large clusters (351 MB of games needed 1.7 GB of space).
- **Field finding:** an earlier USB drive with bad blocks (Windows `disk` event 7) froze RetroBat's own installer and needed reboots. A copy to such a drive could likewise block inside a Windows write call, where Cancel can't interrupt it; verification still prevents a damaged copy from being marked complete. Added to residual risks.

## 6. Owner sign-off (to complete)

- [x] Real USB drive copy: done 2026-11-05 (above).
- [x] ~~Unplug the drive mid-copy~~: **waived by the owner** on 2026-11-05 (the available healthy drive also holds other data). Covered by automated tests only (H8).
- [ ] Real RetroBat: confirm copied games appear and launch in RetroBat.
- [ ] Accept the residual risks in §4.

## 7. Addendum: copy choices (step 5b, 2026-11-05)

Authorized by the owner on 2026-11-05 ("Approved, build it all", with kept duplicates named `(2)`). Adds three user-chosen outcomes to the plan. Nothing changes unless the user picks it; the defaults are unchanged (keep existing files, copy neither clashing version).

| Choice | Offered for | What happens on disk |
|---|---|---|
| **Use this version** | Clashes (two different library files, one destination name) | The chosen file copies; the others are skipped. |
| **Keep both** | Clashes and conflicts, **single-file games only**, **never** arcade-type sets (Arcade, Neo Geo, CPS1–3, Naomi, Atomiswave: names are romset names) | The extra copy is written as `Name (2).ext`, or the next number not used on the drive or in the plan. The existing file is untouched. |
| **Replace** | Conflicts (a different file already on the drive), single-file games only | The existing file is **moved** (no-replace rename, same drive) to `<installation>/.retrodrive/backup/<deployment id>/<same path>`, its arrival is checked by size, then the new file is copied the usual staged, verified way. |

### Hazards, mitigations and evidence

| # | Hazard | Mitigation | Test |
|---|---|---|---|
| H15 | Replace destroys the user's file | The original is moved, never deleted or overwritten; the move uses the no-replace rename; the backup is never removed by RetroDrive | `copier::tests::replace_moves_the_original_to_the_backup_folder_then_copies` |
| H16 | The new copy fails after the original was moved | The original is moved back into its (free) name; the item reports "your original file was put back"; if the name is somehow taken, the message gives the backup location | `a_failed_replace_puts_the_original_back` |
| H17 | Cancel during a replace | Same as H16: cancel puts the original back | `cancelling_during_a_replace_puts_the_original_back` |
| H18 | Crash or unplugged drive during a replace | The backup path is journaled **before** the move. On the next start, on the confirmed drive only and only for paths inside that installation's backup folder: if the new file never got its name, the original is moved back (`restored`); if it did, the replace completed (`replaced`) and the backup stays | `an_interrupted_replace_is_put_right_on_the_next_start` |
| H19 | Keep both overwrites another file | The numbered name must be free on the drive and unused by the plan; taken numbers are skipped; the copy still uses `create_new` and the no-replace rename | `keep_both_copies_under_the_next_free_number` |
| H20 | Renaming breaks games | Keep both and Replace are offered only for single-file games (playlists and cue sheets keep working) and never for arcade-type sets | `deployment::tests::clash_choices_pick_a_version_or_keep_both`, `conflict_choices_and_multi_file_games` |
| H21 | The user reviewed different choices than the copy uses | Choices change destinations and actions, which are part of the plan fingerprint; the copy rebuilds the plan with the same choices and refuses on a mismatch | `clash_choices_pick_a_version_or_keep_both` (fingerprint differs) |
| H22 | A choice that isn't offered sneaks in | Unknown or not-offered choices fall back to the default | same test ("an option that isn't offered is ignored") |
| H23 | Windows long-path handling | Found while testing: `\\?\` paths are literal, so `/` is now converted to `\` before Win32 renames | covered by H18's test |

### Evidence
- `cargo test`: 129 passing (7 new for 5b), clippy clean.
- Manual in the app (separate test catalog, mock RetroBat, 2026-11-05): a conflict set to **Replace** and a clash set to **Keep both** → copy finished, every file verified; on disk: `Diff.nes` = new version, original in `.retrodrive\backup\<id>\roms\nes\Diff.nes`; `Clash.nes` and `Clash (2).nes` hold the two versions; manifest records the backup path.

### Residual risks (5b)
- **Backups accumulate.** RetroDrive never deletes them; the user removes `.retrodrive\backup` folders when no longer needed. A "restore from backup" and "clear old backups" screen is a follow-up.
- **Space.** The backup stays on the same drive, so a replace needs room for both files (counted in the plan as a copy).

### Owner sign-off
- [ ] Try one Replace and one Keep both on a real drive; confirm the backup folder and the `(2)` file.

## 8. Addendum: faster journal (step 6, 2026-11-05)

| # | Hazard | Mitigation | Test |
|---|---|---|---|
| H24 | With SQLite `synchronous = NORMAL` during copies (about 1.7× faster for small files), a power loss can drop the journal's last entries, so a temporary file or a replace backup might not be recorded | Recovery no longer depends on those entries: for unfinished items of interrupted deployments it looks for `<destination><marker>` (removed only if it carries that deployment's marker and is a plain file inside the installation) and for the deployment's backup path (moved back only if the destination name is free), on the confirmed drive only. The journal still survives app crashes; the catalog stays consistent (WAL). | `copier::tests::leftovers_are_found_by_name_even_without_journal_entries` |

## 9. Addendum: Batocera is preview-only (RDS-008, 2026-11-06)
- **Scope:** Batocera installations can be recognized and planned for, but **not copied to**. This review covered copies to local Windows drives only.
- **Enforcement:** `Capabilities::copy_games` is false for Batocera. `deployment::build` puts it in the plan (`copy_supported`), and `copier::deploy` refuses as its very first check, before leftovers cleanup, journaling or any file access on the destination. The UI shows a preview note instead of the Copy button. Test: `copier::tests::preview_only_platforms_are_never_copied_to` (destination snapshot unchanged, no deployment row).
- **Superseded 2026-11-07 by §21** (copying turned on). Original note: **Before turning it on (RDS-022):** review no-replace rename and unbuffered read-back over SMB, the share disappearing mid-copy (Batocera shut down), drive identity for `\\BATOCERA\share` (no volume serial guarantees), and exFAT game drives; test on a real Batocera.

## 10. Addendum: undoing a replace (2026-11-06)
**Scope and authorization:** the owner approved this plan on 2026-11-06: list Replace backups per installation and offer *Undo replace*, as a swap that never deletes; no "delete backups" action, only "Open backup folder". Code: `src-tauri/src/restore.rs`; schema v15.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Undo moves a file the user changed or swapped after the copy | Refused unless the file on the drive has the size and SHA-256 RetroDrive recorded when it verified the copy | `undo_refuses_when_anything_changed` |
| Wrong drive under the same letter | Drive confirmed by its fingerprint (as for copies) | `undo_refuses_when_anything_changed` (drive away) |
| A recorded path points outside the installation | Backup and undo paths must be inside `<installation>\.retrodrive\backup`; the destination inside the installation | same test |
| Data loss during the swap | Journal `undoing` first; both moves are no-replace renames on the same drive; if the second fails, the first is reversed; nothing is deleted | `undo_puts_the_original_back_and_keeps_retrodrives_copy` |
| Crash or power loss mid-undo | At startup, with the drive confirmed: both moved → `undone`; only the first → RetroDrive's copy goes back (`replaced`); none → `replaced`; anything else is left for the user | `an_interrupted_undo_is_finished_or_rolled_back_at_startup` |
| The copier's own recovery misreads undo items | Its recovery queries exclude `undoing` and `undone` | same test (last step) |
| Undo races a scan, copy or import | Refused while any of them run; scans, copies and imports are refused during an undo | code review (`lib.rs` `undoing` flag) |
| Backups fill the drive | Shown with their total size; the user deletes them in File Explorer | UI |

**Residual risks:** the deployment manifest on the drive still describes the original copy (the catalog records the undo). An undone file can't be re-applied from this screen; build a new plan and choose Replace again.

## 11. Addendum: copying several games at once (2026-11-06)
**Scope:** the same per-file steps as §2 (journal, `create_new` temporary file, SHA-256 while writing, sync, unbuffered read-back, no-replace rename; Replace moves the original to the backup folder first), now run for up to 4 games at a time on worker threads. No new kind of disk action. Owner request: "work through them all" (2026-11-06), item "copy several files at once".

| Hazard | Mitigation | Evidence |
|---|---|---|
| A worker writes before its journal entry is recorded | Workers send every journal write to the main thread (the only catalog user) and wait for the confirmation before touching the drive | code review (`Remote::ask`); all crash and recovery tests pass unchanged |
| Two workers write the same destination | Each destination appears once in a plan (clash detection, docs/08); temporary names are per destination; the final rename never replaces | `parallel_and_one_at_a_time_copies_end_the_same`, `a_parallel_copy_never_overwrites_a_file_that_appears` |
| Cancel leaves half-written files | Cancel is checked per chunk in every worker; each removes its own temporary file; unstarted games stay `not_attempted` | `cancelling_a_parallel_copy_leaves_only_verified_files` |
| Drive unplugged mid-copy | The main thread checks the drive every second and after any failed file, then stops new games; in-flight games fail safely as before; recovery unchanged | existing recovery tests; same per-file code |
| Several big writes overload a USB hard drive | Files ≥ 32 MB take a shared lock: one at a time | `big_files_are_copied_one_at_a_time` |
| Test hooks or a cancelling observer race the workers | Hooks run on the main thread while the worker waits; a hook that cancels is recorded before the worker continues; call-counting test observers use 1 worker | copier tests repeated 15× without failure |
| An internal error in one worker | The error stops all workers and goes through the existing failure path (deployment `failed`, leftovers cleaned up) | code review |
| The coordinating thread fails while workers wait for it | The request channel lives inside the thread scope, so a failure drops every pending reply and the workers stop; the copy ends with an error instead of hanging (found on CI by a faulty test, 2026-11-06) | `a_coordinator_failure_stops_the_workers_instead_of_hanging` |

**Residual risk:** the per-file order on the drive differs from the plan order, so a crash leaves up to 4 games partly copied instead of 1; the next run finishes them (ADR-021).

## 12. Addendum: RetroBat emulator settings (RDS-021 version 1, 2026-11-06)
**Scope and authorization:** the owner approved this plan on 2026-11-06: per-system emulator/core, shader, bezel, aspect ratio and integer scaling in RetroBat's `es_settings.cfg`; exact before → after shown first; refused while RetroBat runs; previous file moved to `.retrodrive\backup`, never deleted; untouched settings kept exactly; undo. Code: `src-tauri/src/customizer.rs`; schema v16.

| Hazard | Mitigation | Evidence |
|---|---|---|
| RetroBat overwrites the change (it saves settings when it closes) | Refused while `RetroBat.exe`, `emulationstation.exe` or `emulatorLauncher.exe` runs (checked before apply and undo; the UI disables Apply) | `apply_backs_up_then_changes_and_undo_puts_it_back`, `undo_refuses_when_retrobat_changed_the_settings_since` |
| Losing settings written since the user reviewed | Apply needs the file's SHA-256 from the review; undo needs the file to still be exactly what RetroDrive wrote | same tests |
| Damaging other settings | Only the changed entries are edited; the result must parse as XML, keep every other entry's value and count, and set exactly the wanted values; files with duplicate keys or unusual layouts are refused | `edits_touch_only_the_changed_lines`; real-file check (`real_retrobat_files_parse_and_edit_cleanly`) |
| Writing a value RetroBat doesn't know (or a path) | Only choices read from the install; emulator and core must belong together; no newlines | `only_offered_choices_are_accepted` |
| Data loss during the write | Journal first; new file written with `create_new`, synced and read back; current file moved (no-replace) to the backup folder; new file renamed in (no-replace); on failure the current file is moved back | `apply_backs_up_then_changes_and_undo_puts_it_back` |
| Crash mid-change | At startup with the drive confirmed: apply finished → `applied`; not started → temp removed, `failed`; half done → previous file moved back. Undo half done → changed file moved back (`applied`); finished → `undone` | `interrupted_changes_are_put_right_at_startup` |
| Racing a copy, scan, import or undo | Refused while any runs; they're refused during a settings change (shared drive-change flag) | code review (`lib.rs`) |

**Residual risks:** RetroBat could start between the running check and the rename (a window of milliseconds). A setting is ignored when the chosen emulator doesn't support it (RetroBat's own behaviour). The file format is undocumented; it was checked on RetroBat 8.3.1.1 and in RetroBat's launcher source.

## 13. Addendum: theme remix (2026-11-06)
**Scope and authorization:** owner request ("creating themes and also changing the settings"), with the plan shown on 2026-11-06: a remix is saved as a new copy and the original is never touched; switching uses the reviewed, backed-up, undoable settings change (§12). Code: `src-tauri/src/themes.rs`, `customizer::apply_theme`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Changing or losing the installed theme | The base theme is only read; the remix is a new folder; names that exist (any case) are refused | `a_remix_is_a_new_copy_and_the_original_is_untouched` |
| A half-written theme RetroBat might load | Built under a temporary marked name, renamed only when complete; on failure only that marked folder inside the themes folder is removed | same test (no temp folder left); code review |
| Writing outside the theme | Base folder name without separators; font and include paths resolved only inside the theme (no `..`, no drive letters); writes into the copy are checked to stay inside it | `bad_requests_change_nothing` |
| Bad values in theme.xml | Only the theme's own color (RRGGBB/RRGGBBAA) and picture variables can change; values are escaped | same test |
| Filling the drive | Free space checked for the full copy plus 200 MB; the size is shown before saving | same test (no space) |
| Switching the theme while RetroBat runs, or losing other settings | Same rules as §12 (refused while running, SHA check, minimal edit, backup, undo) | `switching_the_theme_is_reviewed_backed_up_and_undoable` |
| Reading arbitrary files for the preview | Only pictures and fonts, 25 MB at most | `previews_only_read_pictures_and_fonts` |

**Residual risks:** subsets defined outside `theme.xml` aren't given a RetroDrive entry (their variables would still override). The preview is approximate. Each remix copies the whole base theme (1.1 GB for Carbon).

## 14. Addendum: boot video, music and retrobat.ini options (2026-11-06)
**Scope and authorization:** owner request on 2026-11-06 ("let's build your suggestions"; "add their own video and music"): Customize page with boot video, menu music and curated retrobat.ini options, all with review, backup and undo. Code: `customizer.rs` (`look_options`, `apply_look`, `add_boot_video`, `add_music`, `write_file`).

| Hazard | Mitigation | Evidence |
|---|---|---|
| Damaging retrobat.ini (comments, other keys) | Only values of existing keys are replaced; the result must keep every other key's value and the key count; layout and line endings kept | `boot_video_and_frontend_options_edit_retrobat_ini_safely`; real-file check |
| Risky RetroBat options | Not offered: Autostart (Windows startup), ResetConfigMode (resets settings), AppLauncher (runs programs), window sizes | same test |
| Bad values | Booleans 0/1, choices from fixed lists, numbers 0-60,000; the boot video must be a file in RetroBat's video folder | same test |
| Losing changes RetroBat makes / racing it | Same as §12: refused while running, SHA check from the review, journal, backup, undo, crash repair (shared `write_file`) | same test; §12 tests |
| Replacing the user's media | Own videos/music are copied under a temporary marked name, compared by SHA-256 with the source, then renamed without replacing; an existing name is refused | `adding_a_boot_video_never_replaces_one` |
| Copying something that isn't media | Only .mp4 (videos) and .ogg/.mp3 (music), 2 GB at most | same test |

**Residual risk:** a custom `FilePath` makes RetroBat play videos from another folder; the page says so instead of guessing.

## 15. Addendum: designs (2026-11-06)
**Scope and authorization:** owner choice on 2026-11-06 ("Designs as described"): designs edited in a studio without touching RetroBat, finished or abandoned, applied to an installation as one previewed step and undone as one step. Code: `src-tauri/src/designs.rs`, `src/Customize.tsx`; schema v17.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A design half-applied (e.g. settings changed, files missing) | Everything is checked before any write (both edits computed, every value validated against the install, every file to add checked); files are added first; es_settings.cfg is written, then retrobat.ini, and the first is undone if the second fails | `a_design_that_cant_fully_apply_changes_nothing`; code review |
| Replacing the user's files | Media are added as new files (an identical file is reused; a different file with that name stops the apply before anything is written); the remixed theme is a new folder; a theme with the design's name made by something else stops the apply | same test; `applying_a_design…` (re-apply recognises its own theme and files) |
| Misread music settings | Bool and int settings are written as `<bool>`/`<int>` (RetroBat ignores the wrong element type); values range-checked | `applying_a_design…` |
| Racing RetroBat or a stale review | Refused while RetroBat runs; both files' SHA-256 from the preview must still match | same test |
| Undo losing later changes | Undo goes through each file's own undo, which refuses if that file changed since | §12 tests |
| Designing changing RetroBat by accident | The studio only writes the design's draft in the catalog | `drafts_finish_or_are_abandoned_without_touching_retrobat` |

**Residual risks:** added theme folders and media stay after undo (by design; never deleted). If both the second write and its rollback fail, the user is told to undo the first file from the change history.

## 16. Addendum: choosing which songs play; creating files (2026-11-07)
**Scope:** owner request ("turn off only the music I want… as part of our customization"). A design can list the tracks to play; applying writes `music\favorites.m3u` and sets `audio.useFavoriteMusic`. Code: `designs.rs` (`play_tracks`), `customizer.rs` (`favorites_text`, `favorite_tracks`, `music_tracks`; `write_file` creating files).

| Hazard | Mitigation | Evidence |
|---|---|---|
| Losing music the user owns | Nothing is moved or deleted; unwanted tracks just aren't in the playlist | `a_design_chooses_which_tracks_play` |
| An empty or broken playlist (RetroBat would turn favorites off) | At least one track required; every track must exist on the drive or be added by the design; names can't contain path tricks or line breaks | same test |
| Replacing an existing playlist | It's backed up like any settings file; undo puts it back | §12 tests |
| Creating a file that wasn't there | Journaled with "no previous version"; written to a temporary name, read back, renamed without replacing; undo moves it into the backup folder; crash repair finishes or removes only RetroDrive's own temporary file | `a_new_file_is_created_journaled_and_undone_by_moving_it_aside` |
| A design blocked by values it didn't change | Values equal to the RetroBat's current ones are skipped before validation | `a_design_can_start_from_a_retrobats_current_setup` |

**Residual risk:** the playlist holds full paths with the drive letter (RetroBat's format); on another letter, apply the design again.

## 17. Addendum: game art and details in RetroBat's game lists (2026-11-07)
**Scope:** owner request ("apply those [game images] to RetroBat"; answers: when copying and from a button; keep RetroBat's art; box art, logos, catalog details). Code: `gamelists.rs`; `lib.rs` (`preview_retrobat_art`, `apply_retrobat_art`, `start_deployment` `add_art`); scanner stores marquees.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Overwriting art or names RetroBat already shows | Only missing tags are added; a name is replaced only if it equals the file name; existing entries are never removed or reordered | `art_and_details_fill_gaps_and_keep_retrobats_own` |
| Replacing an image file | Images are written to a temporary name, read back (SHA-256) and renamed without replacing; a different file already at the name is left alone and the tag isn't added | `a_missing_list_is_created_and_a_stale_review_is_refused` |
| Corrupting `gamelist.xml` | Edits are text insertions; the result must be well-formed and keep every old entry before it's written; written through the journaled settings writer (backup, temp + read-back, crash repair) | same test; `gamelists_are_read_with_their_layout` |
| RetroBat overwriting the change, or reading a half-written list | Refused while RetroBat/EmulationStation runs | `art_and_details_fill_gaps_and_keep_retrobats_own` |
| Acting on an old review | The Customize apply sends the lists' SHA-256 from the review; any change is refused | `a_missing_list_is_created_and_a_stale_review_is_refused` |
| A failure part-way | Lists already written are put back; copied images stay as harmless new files | code review (rollback uses the tested settings undo; no failure-injection test) |
| Undo | One *Applied designs* row restores every list (created lists are moved into the backup folder); images are not deleted (never-delete rule) | `art_and_details_fill_gaps_and_keep_retrobats_own`; `a_new_file_is_created_journaled_and_undone_by_moving_it_aside` |
| Copyright | Only the user's own images, from their own folders, copied to their own drive; RetroDrive ships no art | docs/16 |

**Residual risk:** images on a NAS that isn't connected are skipped (reported); undo leaves the copied images in `images` (unused once the list is restored).

**Update (2026-11-07, speed):** checks and image copies now run 16 at a time. Each image is still copied to a temporary name, read back and renamed without replacing; an image already at the name is compared by size, then contents. If any image fails, no game list is changed (images already copied are new, unused files).

## 18. Addendum: Sync (2026-11-07)
**Scope:** owner request (one-click Sync that keeps the selected platforms up to date, asks about games deleted from the drive, and re-checks what's there). Code: `deployment.rs` (`plan_sync`, `mark_removed`), `installations.rs` (sync selection, kept-off list), `copier.rs` (`confirmed_before`, `still_confirmed`), `App.tsx` (Sync panel). Schema v18.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Sync writes more than the user expects | It's the normal plan and copy: one summary with counts and space before anything starts; fingerprint re-checked; no overwrites; every copy verified | existing copier tests; app check |
| A game the user deleted comes back | Games copied before and now missing are *removed*, not copied, until the user chooses *copy again*; *keep off* is remembered | `sync_keeps_the_saved_selection_and_asks_about_removed_games` |
| Sync deletes something | Nothing deletes: games removed from the library stay on the drive; the kept-off list is catalog-only | same test (planning writes nothing) |
| Skipping the content check hides a changed file | Only when source path and size match an earlier confirmation **and** neither file was modified or created at or after it (Explorer copies get a new created time); otherwise compared by content as before. Skipping never writes anything | `a_later_copy_trusts_earlier_checks_only_while_nothing_changed` |
| Left-out platforms forgotten | Saved per installation and shown in every Sync summary ("left out: …") | sync test |

**Residual risk:** a tool that rewrites a file on the drive and then sets both its modified and created times back to before RetroDrive's confirmation would not be re-read; nothing is written in that case, only the "already there" report could be wrong.

## 19. Addendum: saved choices (2026-11-07)
**Scope:** choices kept out of the way and saved per installation (ADR-044). Code: `src/Decisions.tsx`, `App.tsx`, `installations.rs` (`saved_choices`, `set_saved_choices`), schema v19.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A saved choice replaces a drive file later without the user seeing it | *Replace* is never saved; it must be chosen in that plan, and the drive file is still moved to the backup folder first | `decisions_are_saved_except_replace` |
| "For all" does something drastic | "For all" offers only skip, a version, the bigger file, copy both, copy again or keep off; never replace | `Decisions.tsx` `bulkOptions` |
| Hidden choices hide what will happen | The summary always states how many games are skipped until a choice, and counts every outcome; the copier receives exactly the choices shown | app check |
| A saved version no longer exists | The plan falls back to the default (skip both) | existing `clash_choices_pick_a_version_or_keep_both` handling |

## 20. Addendum: sync scope (2026-11-07)
**Scope:** Sync limited to what was chosen in copied plans (ADR-045). Code: `deployment.rs` (`sync_scope`, `add_to_sync_scope`, scope filter in `build`, `changed` relabel), `copier.rs` (`DeployRequest.sync`), `lib.rs`, `App.tsx`, `Decisions.tsx`; schema v20.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Sync copies far more than the user chose | Sync plans only the scope; a plan starts with nothing ticked; with no scope Sync refuses with a message | `sync_keeps_only_what_was_synced_and_asks_about_removed_games` |
| A platform the user moved off the drive comes back | The derived scope only includes platforms with a copied game still on the drive | `sync_scope_starts_from_what_was_copied_and_grows_with_plans` |
| Restoring a changed file loses the user's edit | Restore is the existing replace: drive file moved to `.retrodrive\backup` first, restored on failure; per game, never saved, never "for all" | `a_synced_game_changed_on_the_drive_is_shown_and_restored_only_if_chosen`; existing replace tests |
| The copy runs a different plan than shown | The copier rebuilds the sync plan from the same scope and checks the fingerprint (which includes the scope) | existing fingerprint tests |
| Stop syncing deletes games | It only edits the catalog scope; the drive isn't touched | `set_sync_scope` |

## 21. Addendum: copying to Batocera (RDS-022, 2026-11-07)
**Scope and authorization:** the owner approved step 1 of the Batocera plan on 2026-11-07 ("Yes, start step 1"): copying to a Batocera game drive plugged into this PC and over `\\BATOCERA\share`, same rules as RetroBat. Code: `adapters/batocera.rs` (`copy_games: true`), `volumes.rs` (share id), `copier.rs` (claims the share before journaling). No new kind of disk action except one small id file.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A different machine with the same name (`\\BATOCERA\share` is the default) is written to | The first copy writes a random id to `<share>\.retrodrive\share-id` (create-new) and records it in the fingerprint; from then on presence needs the same id, so another machine is *a different drive* and plans, copies and recovery refuse it | `a_share_is_known_by_its_id_once_it_has_one`; `batocera_over_a_network_share_is_known_by_its_id` (real SMB) |
| The id file overwrites something | Written only if missing, with create-new; an existing file is never replaced; unreadable contents count as no id | same unit test |
| Batocera shuts down mid-copy | The copier checks the share every second and after any failed file (`presence` → the folder is gone), stops new games and keeps verified ones; temporary files are cleaned on the next copy once the share (with its id) is back | existing drive-loss handling (§11) and recovery tests; per-file steps unchanged |
| Read-back served from the PC's cache over SMB | Read-back uses `FILE_FLAG_NO_BUFFERING`, which makes the SMB client fetch from the server; falls back to a normal read only if the filesystem refuses unbuffered reads | real SMB test (copies verified); code review |
| Rename replaces a file over SMB or exFAT | `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` fails if the target exists on both | real SMB test; existing `a_parallel_copy_never_overwrites_a_file_that_appears` |
| exFAT game drive limits | exFAT has no 4 GB limit (only FAT32 does, already checked) and supports the same rename and unbuffered reads; free space and cluster size are measured as for any drive | code review; **not yet tried on an exFAT drive** |
| Copying into the BATOCERA system partition | Detection never accepts it (it explains to pick the game folder instead), so it can't become an installation | `adapters::batocera::tests::near_misses_explain_which_folder_to_choose` |

**Residual risks:** not yet tried on a real Batocera (Samba rather than Windows' SMB server) or on an exFAT drive; the in-app note says network copies are new. A share that loses its `.retrodrive` folder after a copy shows as a different drive until it's re-added (Forget, then add; the next copy gives it a new id).

## 22. Addendum: Batocera Sync, art and details (2026-11-07)
**Scope:** step 2 of the owner-approved Batocera plan. No new kind of disk action: the reviewed game-list step (§17: new image files only, journaled list edits with backups, undo) and Sync (§18, §20) now also run on Batocera installations. Code: `gamelists.rs`, `customizer.rs` (`install_for`, `never_running`), `lib.rs` (`running_for`), `App.tsx`, `Customize.tsx`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Batocera overwrites RetroDrive's list edits | EmulationStation is believed to update only the entries it changed when saving (to verify); RetroDrive only adds what's missing, every list is backed up and undoable; the user is told to use Update Gamelists so the running Batocera reloads | design note (ADR-047); not yet seen on a real Batocera |
| A RetroBat running on this PC blocks Batocera changes, or Batocera changes skip a needed check | The running check follows the installation: RetroBat's for RetroBat, none for Batocera | code review (`running_for`); RetroBat tests unchanged |
| RetroBat-only settings written to a Batocera | Emulator settings, themes and designs still use `customizer::install`, which refuses Batocera; Customize hides them for Batocera | `batocera_game_lists_get_the_same_art_and_details` (last assertion) |
| Undo on a share that went away | Undo checks the drive (and the share id, §21) like any undo | existing undo tests |

**Residual risk:** not yet tried on a real Batocera; if it does rewrite whole lists, art added while it runs could be dropped (adding art again restores it; nothing is lost).

## 23. Addendum: Batocera settings and theme (2026-11-07)
**Scope:** step 3 of the owner-approved Batocera plan. Edits two Batocera files through the reviewed settings writer (§12: SHA check, temporary file read back, original moved into `.retrodrive\backup`, journal, undo, startup repair). Code: `batocera_settings.rs`, `BatoceraSettings.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Other settings in batocera.conf changed or lost (Wi-Fi password, hostname…) | Only offered keys can be changed; after editing, every other active key must be identical or nothing is written; lines are edited in place like Batocera's menu | `edits_change_only_the_wanted_keys_like_batoceras_menu`; `conf_edit` self-check |
| A value that breaks Batocera or injects a line | Only Batocera's own values for each setting; line breaks refused; scopes limited to all systems or systems with games | `settings_and_theme_are_reviewed_saved_and_undone` |
| Batocera's menu and RetroDrive overwrite each other | Batocera re-reads the file and changes only its own lines (`SystemConf::saveSystemConf`); RetroDrive checks the file is unchanged since review | design note; stale-review test |
| Theme switch lost when Batocera quits | Batocera rewrites es_settings.cfg on quit only if a menu setting changed; the app says to restart Batocera right after switching over the network | message in `apply_theme` |
| Theme set to something that doesn't exist | Only the built-in Carbon or folders in `themes`; Batocera also falls back to an available theme | same test |

**Residual risk:** not yet tried on a real Batocera.

## 24. Addendum: install helper (RDS-027, 2026-11-07)
**Scope and authorization:** owner approved the guided plan on 2026-11-07 ("Approve, build it"). RetroDrive reads an image file the user downloaded and starts an image writer the user installed. **No disk writes at all** in this feature. Code: `install_helper.rs`, `InstallHelper.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Erasing the wrong drive | RetroDrive never writes images; the user picks the drive in Raspberry Pi Imager or balenaEtcher, which warn before erasing. Imager is started without `--cli`, so it only opens with the image chosen | design (ADR-049); `open_writer` |
| Starting an arbitrary program | Only the two writers, only from their install folders (Program Files, or Etcher's per-user folder); the screen sends an id, never a path | `only_known_programs_and_pages_are_opened` |
| A crafted file name or path runs commands | The image path is passed as one process argument (no shell) and must be an existing file | code review |
| Opening unsafe links | Only fixed official URLs by id (batocera.org, retropie.org.uk, raspberrypi.com, etcher.balena.io) | same test |
| Writing a damaged or wrong image | Type and size checks, xz end-of-file check, name → system/hardware shown, wrong system noted, optional checksum; Write buttons stay off for a bad file or a checksum mismatch | `images_are_recognized_and_short_downloads_caught`, `checksums_from_the_download_page_are_compared` |
| Distributing images | RetroDrive links to the official pages; it doesn't download, bundle or host images | design |

**Residual risk:** gzip files can't be proven complete without unpacking; the checksum covers that, and the writers verify what they write.

## 25. Addendum: RetroPie is preview-only (RDS-023 step 1, 2026-11-07)
- **Scope:** RetroPie installations can be recognized and planned for, but **not copied to** (`copy_games: false`); the copier refuses before any write, as for Batocera before §21. No writes of any kind in this step.
- **Detection safety:** RetroBat and Batocera folders are never claimed; a Raspberry Pi boot partition is never accepted as a game folder.
- **Superseded 2026-11-07 by §26** (copying turned on). Original note: **Before turning on copying (step 2):** the Samba share (`\\RETROPIE\roms`) gets the share id of §21 (every RetroPie is `RETROPIE` by default); test on a real RetroPie (Samba on a Pi), including the Pi shutting down mid-copy; USB sticks are FAT32/exFAT (4 GB limit on FAT32 already enforced).

## 26. Addendum: copying to RetroPie (RDS-023 step 2, 2026-11-07)
**Scope and authorization:** owner go-ahead on 2026-11-07 ("go") for copying to RetroPie's `\\RETROPIE\roms` share and a USB `retropie`/`retropie-mount` folder, same rules as RetroBat and Batocera. Code: `adapters/retropie.rs` (`copy_games: true`, layout check wording). No new kind of disk action: the copier (§2, §11), the share id (§21) and recovery are unchanged.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Another Pi named RETROPIE (the default) is written to | Share id written on the first copy and required afterwards (§21) | `retropie_over_a_network_share_is_known_by_its_id` (real SMB) |
| Games land in the wrong folder (share root vs USB `roms`) | Placement follows `has_roms_folder`; plans show the real prefix | `retropie_is_copied_to_in_both_layouts` |
| The Pi shuts down mid-copy | Same drive-loss handling as §11/§21: stop, keep verified games, clean temporary files when the share (with its id) is back | existing tests; per-file steps unchanged |
| FAT32 USB sticks | Files over 4 GB are blocked by the existing filesystem check | existing `max_file_bytes` handling |

**Residual risk:** not yet tried on a real RetroPie (Samba on a Pi).

## 27. Addendum: RetroPie Sync and art (2026-11-07)
**Scope:** step 3 of the RetroPie work, owner's choice "only existing lists, opt in". No new kind of disk action: Sync (§18, §20) and the game-list step (§17: new image files only, journaled list edits with backups, undo). Code: `gamelists.rs`, `lib.rs`, `App.tsx`, `Customize.tsx`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Creating a rom-folder list hides RetroPie's home-folder list (scraped art, favorites, play counts) | Only lists already in rom folders are filled in; creating one needs the per-check opt-in with a warning; never after a copy | `retropie_lists_are_filled_in_and_only_created_when_asked` |
| Lists or images in the wrong place on the share (no `roms` level) | `games_dir` uses `roms\` only when it exists; the RetroPie test checks `nes\images` at the share root | same test |
| A created list is unwanted | Undo moves it aside (the journal's "no file before" case); RetroPie goes back to its home-folder list | existing undo tests |
| RetroPie running while lists change | Its EmulationStation reads lists at start; RetroDrive says to restart it; it doesn't run on this PC, so no local "close it first" check | design note |

**Residual risk:** not yet tried on a real RetroPie.

## 28. Addendum: RetroPie default emulators (2026-11-07)
**Scope:** step 4 of the RetroPie work (owner: "do in your proposed order 1-3", item 1 = RetroPie Sync, art and settings). Edits `configs\<system>\emulators.cfg` on RetroPie's `configs` share through the reviewed settings writer (§12). Code: `retropie_settings.rs`, `RetroPieSettings.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Writing to a different Pi's configs | The roms share must be connected with its share id (§21) before anything is read or written; configs is derived from the same host; undo requires the exact file RetroDrive wrote | code (`context`); undo check |
| An emulator that isn't installed, or an injected line | Only ids already in that file; ids limited to letters, digits and `-_.+`; after editing, the emulator list must be identical and the default the new one | `only_the_default_line_changes_to_an_installed_emulator` |
| Other lines or systems changed | Only the `default` line; only the reviewed systems' files; each checked against its reviewed SHA | `emulator_choices_are_reviewed_saved_and_undone` |
| Path tricks in a system name | System names with `\ / : .` refused | same test |
| A failure halfway through several systems | Files already written are undone; otherwise each is listed for Undo | code review |

**Residual risk:** not yet tried on a real RetroPie or its configs share; backups live in `.retrodrive\backup` on the configs share.

## 29. Addendum: ES-DE is preview-only (RDS-025 step 1, 2026-11-07)
- **Scope:** ES-DE ROMs folders can be recognized and planned for, but not copied to (`copy_games: false`); the copier refuses before any write. No writes in this step.
- **Detection safety:** RetroBat and Batocera folders are never claimed; ES-DE's data folder (settings, game lists, media) is never accepted as a game folder; RetroPie no longer claims a folder with ES-DE's markers.
- **Superseded 2026-11-07 by §30.** Original note: ES-DE's ROMs folder is a normal Windows folder (local drive, USB or NAS), so the reviewed copier applies as for RetroBat; a NAS path gets the share id (§21).

## 30. Addendum: copying to ES-DE (RDS-025 step 2, 2026-11-07)
**Scope and authorization:** owner go-ahead on 2026-11-07 ("merged, go"). ES-DE's ROMs folder gets the same copier as RetroBat (§2, §11); Sync is offered (§18, §20) without the art step. Code: `adapters/esde.rs` (`copy_games: true`), `App.tsx`. No new kind of disk action.

| Hazard | Mitigation | Evidence |
|---|---|---|
| ES-DE's own files in the ROMs folder overwritten | Copies never overwrite; only game files go into system folders; `systems.txt`/`systeminfo.txt` untouched | `esde_roms_folders_are_copied_to` |
| ROMs folder on a NAS shared with other machines | Network paths get the share id (§21) | existing share-id tests |
| RetroDrive's records confuse ES-DE | Records are in `.retrodrive`, which isn't an ES-DE system name | ES-DE loads only es_systems.xml folders |

**Residual risk:** not yet tried with a real ES-DE installation (not installed on the test PC); the folder layout follows ES-DE's source.

**§30 note (2026-11-07, portable ES-DE):** the `ROMs` folder next to `ES-DE.exe` is accepted even while empty; `ROMs_ALL` (ES-DE's catalog of empty system folders) and the program/data folders are never accepted as game folders. No new writes.

## 31. Addendum: copying BIOS files (2026-11-07)
**Scope and authorization:** owner request ("I think we should"). BIOS files from library `bios` folders are copied with the reviewed copier into the installation's BIOS folder. No new kind of disk action. Code: `classify.rs`, `deployment.rs` (`bios_path`), `adapters/mod.rs` (`plan_bios`), adapters, `App.tsx`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A BIOS (or emulator file) already on the drive is replaced | The copier never overwrites; a different file with the same name is a conflict and kept (62 on the owner's RetroBat, e.g. newer PPSSPP shaders) | read-only plan on the owner's files; existing copier tests |
| Files land outside the BIOS folder | `bios_path` comes from the catalog path below `bios`; `..`/empty parts refused; destinations stay inside the installation | `bios_files_keep_their_subfolders_in_the_bios_folder` |
| Two library versions of a BIOS | The usual clash choice (pick one or keep both) | existing clash handling; 8 on the owner's files |
| Junk copied | Housekeeping files (`.gitkeep`, `.DS_Store`, `_info.txt`, `Thumbs.db`, `desktop.ini`) aren't placed | same test |
| Distributing BIOS | RetroDrive copies only the user's own files from their library; nothing is shipped | design |

**Residual risk:** libraries indexed before this change need re-importing so files in BIOS subfolders are known.

## 32. Addendum: ES-DE art and details (2026-11-07)
**Scope:** the reviewed game-list step (§17) for ES-DE, writing in ES-DE's data folder instead of the ROMs folder. Code: `gamelists.rs` (`Layout::EsDe`), `customizer.rs` (`esde_running`), `lib.rs`, `App.tsx`, `Customize.tsx`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| ES-DE overwrites the edited lists when it closes | Refused while `ES-DE.exe` runs (same check as RetroBat) | `esde_gets_details_in_its_own_list_and_pictures_named_after_each_game` |
| ES-DE's own pictures replaced | A picture ES-DE already has for that game and type (`.png`/`.jpg`/`.jpeg`) is kept; copies are new files only (temporary name, checked, no-replace rename) | same test |
| Writing in the wrong ES-DE | The data folder must be the one next to the portable `ES-DE.exe`, or the user folder's ES-DE whose `ROMDirectory` is this ROMs folder; otherwise art is refused with an explanation | `esde_data_dir` |
| A created list is unwanted | Journaled with backup; Undo moves it aside; pictures stay (as for other platforms) | same test |
| Path tricks from game paths | Picture names come from the catalog's game path; empty or `..` parts are skipped | code review |

**Residual risk:** not yet tried on the owner's real ES-DE (needs the owner's go-ahead to write in `G:" + B + "ES-DE" + B + "ES-DE`).

## 33. Addendum: Recalbox is preview-only (RDS-025 step 4, 2026-11-07)
- **Scope:** Recalbox shares can be recognized and planned for, but not copied to (`copy_games: false`); the copier refuses before any write. No writes in this step.
- **Detection safety:** the boot partition is never accepted; RetroBat folders are never claimed; Batocera, RetroPie and ES-DE no longer claim a folder with `system\recalbox.conf`.
- **Superseded 2026-11-07 by §34.** Original note: `\\RECALBOX\share` gets the share id (§21); an external drive's `recalbox` folder and the SD card's exFAT SHARE partition use the reviewed copier as for Batocera (§21).

## 34. Addendum: copying to Recalbox (RDS-025 step 5, 2026-11-07)
**Scope and authorization:** owner go-ahead on 2026-11-07 ("go Recalbox"). Recalbox shares get the same copier as Batocera (§21): games into `roms\<system>`, BIOS into `bios`; Sync offered. Code: `adapters/recalbox.rs` (`copy_games: true`), `App.tsx`. No new kind of disk action.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Another Recalbox named RECALBOX (the default) is written to | Share id on the first network copy (§21) | existing share-id tests |
| Recalbox's own files replaced | Never overwrite; BIOS already there is kept | `recalbox_shares_are_copied_to`; copier tests |
| Wrong folder names | Recalbox's names (`o2em`, `amiga600`, `segacd`, `gw`) | `placement_uses_recalbox_folder_names` |

**Residual risk:** not yet tried on a real Recalbox.

## 35. Addendum: Recalbox art and details (2026-11-07)
**Scope:** owner go-ahead ("go recallbox art"). The reviewed game-list step (§17) on Recalbox shares, same in-folder layout as Batocera (§22). No new kind of disk action. Code: `gamelists.rs` (`GAMELIST_PLATFORMS`), `lib.rs`, `App.tsx`, `Customize.tsx`.
- Existing art and details are kept; only missing fields are added; pictures are new files only; lists are journaled with backups and undoable (`recalbox_game_lists_get_the_same_art_and_details`).
- Recalbox runs on its own machine: no local running check; the user is told to update the game lists or restart Recalbox. Same residual risk as Batocera (§22): if a running Recalbox rewrote whole lists, art added meanwhile could be dropped (adding it again restores it).

## 36. Addendum: BIOS files outside the installation folder (2026-11-07)
**Scope and authorization:** owner ("Do #1 next"). Copies library BIOS files to RetroPie's `\\<host>\bios` share or ES-DE's RetroArch `system` folder. Code: `bios_step.rs`, `BiosStep.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A BIOS or emulator file replaced | New files only: temporary name (create-new), checked, renamed without replacing; anything already there is left alone (different ones listed as kept) | `missing_bios_files_are_copied_and_nothing_is_replaced` |
| Writing to another Pi's bios share | The installation (roms share) must be connected with its share id before anything is planned; the bios share is on the same host | `target_for` |
| Writing to the wrong RetroArch | Only RetroArch next to the portable ES-DE (or RetroArch's default `C:\RetroArch-Win64`); its own `system_directory` setting is respected | `locations_for_retropie_and_retroarch` |
| Two library versions of a BIOS | Not copied; listed so the user can decide | same test |
| Crash mid-copy | At worst a temporary `…rdpart` file stays next to the target (never a half file under the real name); no journal is needed because nothing is replaced | `copy_image` behavior (§17) |

**Residual risk:** not tried on a real RetroPie or with RetroArch installed for ES-DE (the owner's ES-DE has no emulators yet).

## 37. Addendum: Controllers workspace, step 1 (2026-11-07)
**Scope and authorization:** owner (RDS-028, "I like all three"). Step 1 only reads: it lists HID game controllers through Windows Raw Input, opens each device with zero access rights just to read its product name, and looks it up in a list compiled into the app. Code: `controllers.rs`, `Controllers.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Writing to a device or a system | No writes anywhere: device handles are opened with access 0 (query only) and closed; no files are created | code review of `controllers.rs` |
| Misleading advice for look-alike controllers | Rows that share a USB ID are never merged; a type is only shown when the row's names agree; the card says how many devices share the ID | `the_owners_list_is_loaded_and_looked_up` |
| Licence obligations for bundled data | zlib and MIT notices shipped (`THIRD_PARTY_CONTROLLERS.txt`) and shown in the workspace with source versions | same test (notices present) |

**Update (controller confirm, ADR-063):** the only new write is one row in RetroDrive's own catalog (`controller_choices`) when the user confirms a style; type ids and USB IDs are validated. Nothing is written to devices or systems.

**Residual risk:** detection not yet tried with a real controller; steps 2 and 3 (pushing setups, light guns) write files and need their own scoped plan and review.

## 38. Addendum: controller setup, step 2 (2026-11-07)
**Scope and authorization:** owner ("Yes, start step 2", and yes to bundling RetroArch's setup files). Writes controller setup files on the user's systems from the Controllers page. Code: `controller_setup.rs`, `Controllers.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A working controller setup changed or replaced | Add only: RetroArch files are new files (a name already taken is skipped); RetroBat's user `gamecontrollerdb.txt` keeps every existing line and only gets lines added at the end. Nothing is planned when the system already knows the controller | `retrobat_gets_the_sdl_line_only_when_it_needs_it`, `retropie_and_retroarch_get_new_files_and_keep_existing_ones` |
| A wrong mapping breaks the controller | No hand-made button numbers: only SDL GameControllerDB lines (applied by SDL with its own GUID matching) and RetroArch's own setup files; Batocera/Recalbox get menu steps instead of a guessed `es_input.cfg` | ADR-064 |
| Can't take it back | Each file goes through `customizer::write_file` (journal, read-back check, no-replace renames, backup of a changed file); listed under *Show every settings change*; if one file fails, the earlier ones are undone | `a_setup_is_added_journaled_and_undone` |
| Writing to the wrong place | Only the installation's own folder (RetroBat), the same Pi's `configs` share (RetroPie, roms share connected first), or the RetroArch next to the portable ES-DE (or `C:\RetroArch-Win64`) and its own `joypad_autoconfig_dir` | `plan_for`, tests |
| Accidental click | Two clicks, the second names the files to be added | UI |

**Update (Plan/Sync tick box, ADR-065):** the same add-only setup can run right after a copy when the user leaves the tick box ticked; it only runs for controllers the user confirmed and only where RetroDrive's plan says *add*; a failure is reported in the result and never affects the copied games.

**Residual risk:** not yet run against a real RetroPie or a RetroArch for ES-DE; the owner's controllers are Xbox-style, for which nothing needs writing.

## 39. Addendum: light guns A–C (2026-11-07)
**Scope and authorization:** owner ("build A, B, and C, then do D"). Reads only: lists HID mice to recognize guns (zero-access handles, as in §37), shows guides and finds light gun games in the catalog. The only write is one row in RetroDrive's own catalog (`gun_choices`) when the user confirms or chooses a gun; gun ids and USB IDs are validated. Nothing is written to devices or systems. Step D (copying the Sinden software into RetroBat) writes files and gets its own addendum.

## 40. Addendum: Sinden software into RetroBat, light guns D (2026-11-07)
**Scope and authorization:** owner ("build A, B, and C, then do D"). Copies the Sinden software from a folder the user picks into RetroBat's `system\tools\sinden`. Code: `sinden_step.rs`, `LightGuns.tsx`, `lib.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| A file in RetroBat replaced | New files only: temporary name (create-new), checked, renamed without replacing; different files already there are kept and listed | `the_sinden_software_is_copied_into_retrobat_without_replacing_anything` |
| Copying the wrong folder (e.g. a whole drive) | The folder must contain `Lightgun.exe`; at most 3,000 files and 2 GB; links aren't followed | same test, `files_below` |
| Copying a folder into itself | Refused when the source is inside the target or the reverse | same test |
| Wrong place | Only `system\tools\sinden` inside a connected RetroBat installation that has been started (its `system` folder exists) | `target_for` |
| Distributing third-party software | RetroDrive ships and downloads nothing; it copies the user's own download | ADR-067 |
| Crash mid-copy | At worst a temporary `…rdpart` file stays; never a half file under a real name | `copy_image` (§17) |

**Residual risk:** not tried with the real Sinden software (the owner has no Sinden gun); file names come from the user's download.

## 41. Addendum: Recalbox settings (2026-11-07)
**Scope and authorization:** owner ("let's go to the next step"; roadmap RDS-025: Recalbox settings). Edits the share's `system\recalbox.conf`. Code: `recalbox_settings.rs`, `RecalboxSettings.tsx`, `lib.rs`; shared editor in `batocera_settings.rs`.

| Hazard | Mitigation | Evidence |
|---|---|---|
| Other settings changed | Only the reviewed keys change; the editor checks that every other active value is identical before saving | `edits_change_only_the_wanted_keys_like_recalboxs_menu`, `edit_conf` check |
| Recalbox's menu changed the file meanwhile | Saved only if the file's hash equals the reviewed one | `settings_and_theme_are_reviewed_saved_and_undone` |
| Invalid values break Recalbox | Only Recalbox's own keys, scopes, values and installed themes are accepted; no line breaks in values | same test |
| Can't take it back | `customizer::write_file`: journal, read-back check, backup, no-replace renames; Undo in the settings history | same test (undo restores the file) |

**Residual risk:** not tried on a real Recalbox.

## 42. Addendum: vertical games, game groups and screen rotation (2026-11-07)
**Scope and authorization:** owner ("Yes, let's do it according to your plan": A vertical games, B groups on RetroBat/Batocera, C screen orientation). A reads only. B edits one setting (`CollectionSystemsAuto`) in es_settings.cfg; C adds `display.rotate` to Batocera's settings.

| Hazard | Mitigation | Evidence |
|---|---|---|
| The user's own collections lost | Only `lightgun` and `vertical` are added or removed; every other name in the list stays, in order | `groups_are_added_and_removed_without_touching_the_users_collections` |
| RetroBat saves over the change | Refused while RetroBat runs (it rewrites es_settings.cfg when it closes); Batocera over the network: restart before other menu changes (as for themes) | same test; message |
| A sideways screen (C) | Only 0/90/180/270°; the review warns; Undo or setting it back restores it | `settings_and_theme_are_reviewed_saved_and_undone` (extended) |
| Can't take it back | `customizer::write_file` (journal, backup, no-replace renames); Undo | same tests |

**Residual risk:** not tried on a real RetroBat with the groups on, or a real Batocera with rotation.

## 43. Addendum: light gun and vertical collections on RetroPie and ES-DE (2026-11-07)
**Scope and authorization:** owner ("let's continue" after agreeing the next step: fill RetroPie's and ES-DE's groups from the library). Writes `custom-Light Gun Games.cfg` / `custom-Vertical Games.cfg` and one setting (`CollectionSystemsCustom`).

| Hazard | Mitigation | Evidence |
|---|---|---|
| A user's collection changed | Lines are only added (existing lines kept byte for byte); turning a group off removes only its name from the setting, never a file | `es_de_groups_list_the_copied_games_and_keep_existing_lines` |
| Invalid entries | Only games RetroDrive copied and verified there, still present on the drive | `copied` |
| Wrong path on the Pi | Read from RetroPie's own files; if assumed, the review warns | `the_pis_rom_folder_is_read_from_its_own_files` |
| The frontend saves over it | ES-DE: refused while running; RetroPie: restart EmulationStation before changing its settings (message) | same test |
| Can't take it back | Each file via `customizer::write_file` (journal, backup); a failure undoes the earlier files; Undo in the settings history | same test |

**Residual risk:** not tried on a real RetroPie (Pi path and share) or a real ES-DE install.

## 44. Addendum: favorites on the systems (2026-11-07)
**Scope and authorization:** owner ("1. I love this idea": show RetroDrive's Favorites as favorites on the systems). Adds one tag to game list entries within the existing art and details step (§24 and later addenda).

| Hazard | Mitigation | Evidence |
|---|---|---|
| A user's own favorite choice overwritten | Only entries without a `<favorite>` tag get one; an existing `true` or `false` is kept; nothing is removed when a game is unstarred | `starred_games_become_favorites_unless_the_list_already_says` |
| Broken game list | Same checked edit as art: the list is re-parsed and every entry must still be there before saving; journal, backup, undo | existing art tests |

## 45. Addendum: light gun steps for RetroPie, ES-DE and Recalbox (2026-11-07)
**Scope and authorization:** owner ("Light gun steps is next"). The guide reads only. The one write is Recalbox's light gun group: `emulationstation.collection.lightgun=1` (or 0) in `system\recalbox.conf`, edited like Recalbox's own menu (the `;`-disabled line reactivated in place, every other line unchanged), reviewed, refused if the file changed since the review, journaled and undoable (`recalboxs_light_gun_group_is_switched_in_recalbox_conf`). RetroArch settings are not written.

## 46. Addendum: ROCKNIX step 1 and Knulli (2026-11-08)
**Scope and authorization:** owner ("let's keep working the list"; "more systems"). Read-only: the ROCKNIX adapter recognizes, checks and plans; `copy_games` is false, so the copier refuses ROCKNIX until a separate go-ahead and review (as for every new platform). Knulli only gets a label: it is copied to as Batocera, which is already reviewed (§21 and later).

## 47. Addendum: copying to ROCKNIX (2026-11-08)
**Scope and authorization:** owner ("switch on copying to Rocknix"). ROCKNIX now uses the same copier as Batocera, RetroPie and Recalbox: temporary names, read-back checks, no-replace renames, journal and crash recovery scoped to the installation folder (§1–§20); over the network the share gets its id on the first copy and a different machine answering to the same name is refused (§21, ADR-046). Nothing in the copier changed; only the adapter's capability flag and placement (§46). Evidence: `rocknix_roms_folders_are_copied_to`. **Residual risk:** not tried on a real ROCKNIX device or over its Samba share.

## 48. Addendum: game art and details on ROCKNIX (2026-11-08)
**Scope and authorization:** owner ("keep working the list"). ROCKNIX joins the existing art and details step (§24 and later): new image files only, game lists edited by adding missing fields, checked, journaled and undoable. Nothing in that step changed except the list of systems. Evidence: `rocknix_game_lists_get_the_same_art_and_details`.
## 49. Addendum: game details from game lists (2026-11-08)
**Scope and authorization:** owner ("keep working the list"). Scans read three more fields from gamelist.xml files they already read (no new files touched; only RetroDrive's catalog is written). The art step adds `<desc>`, `<players>` and `<rating>` only where an entry lacks them, through the same checked, journaled, undoable edit (§24 and later). Evidence: `library_descriptions_players_and_ratings_go_along`.

## 50. Addendum: ROCKNIX settings (2026-11-08)
**Scope and authorization:** owner ("yes", to building ROCKNIX settings). One file is changed: ROCKNIX's `system.cfg` on the device's `config` share, and only the lines of settings the user reviewed (known keys, ROCKNIX's values, systems with games). The write is the same checked, journaled, backed-up, undoable replacement as other settings (§24 and later), refused if the file changed since the review. The backup and Undo stay on the config share (`.retrodrive\backup`), so no file moves between shares. Nothing is deleted. Not offered for a local card (Windows can't read that part). Evidence: `rocknix_settings::tests`. Not tested on a real ROCKNIX device.

## 51. Addendum: settings for one game (2026-11-08)
**Scope and authorization:** owner ("let's keep going", next on the list: per-game emulator settings). The same files as §23 and §50 (batocera.conf, ROCKNIX's system.cfg) get lines for single games, named as EmulationStation names them; only games present in that system's folder are accepted, and the same values as for systems. Same checked, journaled, backed-up, undoable write, refused if the file changed since the review. Nothing is deleted. Evidence: `one_game_gets_its_own_settings_like_emulationstation_names_them`, the ROCKNIX test's game step.

## 52. Addendum: RetroBat settings for one game (2026-11-08)
**Scope and authorization:** owner ("yes", to RetroBat per-game settings). The same file as RetroBat's emulator settings (es_settings.cfg, §12) gets entries for single games, named as RetroBat's EmulationStation names them; only games in that system's folder, only RetroBat's own choices, emulator and core checked together. Same checked, journaled, backed-up, undoable write, refused while RetroBat runs or if the file changed since the review. Nothing is deleted. Evidence: `one_game_gets_its_own_emulator_and_display_settings`.

## 53. Addendum: Recalbox settings for one game (2026-11-08)
**Scope and authorization:** owner ("yes build", after being told this creates a new file next to each customized game). The only file touched is `roms\<system>\<game file>.recalbox.conf` for a game present in that system's folder (the name is checked against the folder's own listing, so no other path can be named). The first change creates it; later changes edit only their lines (`;`-disabled lines reactivated like Recalbox does). Same journaled, sha-checked write as other settings; Undo moves a created file into `.retrodrive\backup` (never deleted) or puts the previous file back. recalbox.conf is not touched. Evidence: `one_game_gets_its_own_override_file_next_to_it`.

## 54. Addendum: Lakka (2026-11-09)
**Scope and authorization:** owner ("Do it all, let's go", after the research and plan: recognize, copy games and BIOS, playlists, box art). Games are copied to Lakka's ROMs share by the same verified copier (new files only, checked, nothing overwritten or deleted). After a copy, and from Customize after a review, the copied games are **added** to their system's RetroArch playlist on the Playlists share: a playlist is created if missing, existing entries are never removed or changed, and a playlist that isn't RetroArch's JSON format is left alone; each playlist write is journaled, backed up on the Playlists share and undoable, and a reviewed apply is refused if a playlist changed since. Box art goes to the Thumbnails share as new PNG files only (a picture already there is kept). BIOS files go to the System share through the existing BIOS step (new files only). Evidence: `adapters::lakka::tests`, `lakka_playlists::tests`, `lakkas_bios_files_go_to_its_system_share`. Not tested on a real Lakka device.

## 55. Addendum: BIOS checker (2026-11-09)
**Scope and authorization:** owner ("work on everything you can"). Reads only: the catalog, file names in the library's `bios` folders, and file names in the system's BIOS folder (directory listings and existence checks). Nothing is written, copied, downloaded or shipped; copying stays with Plan's BIOS row and the BIOS step. Evidence: `bios_check::tests`; a read-only run on the owner's RetroBat left its file listing unchanged.

## 56. Addendum: System backup, step 1 (2026-11-09)
**Scope and authorization:** owner chose "Backup and restore (Recommended)" and "let's go ahead with the next part" (ADR-090). **On the system: reads only** (directory listings and file reads of saves, settings and game lists). **On the PC:** creates one new folder per backup inside the backup folder, and new files in it; nothing is replaced or deleted.

| Risk | Mitigation | Evidence |
|---|---|---|
| Changing the system | No write path to the system; files are only opened for reading | `a_backup_copies_verified_files_into_a_new_folder_and_never_touches_the_system` (byte-for-byte listing before and after); real run on the owner's RetroBat and ES-DE: `G:` listing unchanged (0 differences) |
| A half-written save copied while the frontend runs | Refused while RetroBat (retrobat.exe, emulationstation.exe, emulatorlauncher.exe) or ES-DE runs | same test (refused, nothing created); the preview shows why |
| Overwriting an earlier backup or another folder | A new folder with a name not used by a finished or incomplete backup (`(2)`, `(3)`…); each file via `copy_image` (temporary file, SHA-256 checked against the original, no-replace rename); the folder is renamed without replacing | `incomplete_backups_are_listed_and_names_are_never_reused`; a second backup leaves the first byte-identical |
| A backup that looks complete but isn't | `manifest.json` written last; the folder keeps `(incomplete)` until then, and when any file failed (listed under problems); **Check** re-reads every file against the manifest | `checking_finds_missing_and_changed_files`; real check: 1,876 of 1,876 files matched |
| Running out of space | Free space checked first (size + 5% + 16 MB) | code review |
| Commands used on arbitrary folders | **Check** and **Open** accept only a folder directly inside the backup folder; the backup folder must be an absolute path | code review |
| Copyrighted art | Downloaded art is opt-in and only goes to the user's own PC (a personal backup); RetroDrive ships or uploads nothing | ADR-090 |

**Restore is not part of this step.** It writes to the system and gets its own addendum and owner go-ahead.

## 57. Addendum: Restore, step 2 of system backups (2026-11-09)
**Scope and authorization:** owner approved the scoped plan on 2026-11-09 ("Yes, build it as planned (Recommended)": preview first; missing files added; identical skipped; different files keep the system's copy unless the user chooses the backup's, then the system's file is moved to `.retrodrive\backup`, never deleted; refused while the frontend runs; one-step Undo; tested on scratch copies, then one small real restore on the owner's RetroBat, undone). **Writes to the system:** new files (missing ones), and for "use the backup's copies" a no-replace move of the system's file into `.retrodrive/backup/restore-<id>/` followed by a new file in its place. Nothing is overwritten in place and nothing is deleted.

| Risk | Mitigation | Evidence |
|---|---|---|
| Losing a newer save | Different files keep the system's copy by default; choosing the backup's moves the system's copy aside first; Undo moves it back | `restoring_adds_missing_files_and_keeps_different_ones_by_default`, `using_the_backups_copy_moves_the_systems_aside_and_undo_puts_everything_back` |
| Writing outside the backed-up places (edited manifest) | Every path must be plain (no `..`, drive, share, control characters, temp marker, `.retrodrive`) and inside what that system's backup contains; one bad path refuses the whole backup | `a_damaged_or_edited_backup_restores_nothing` (`emulators/…exe`, `../`, `.retrodrive/…`, `C:/…`), `the_allowed_places_match_what_a_backup_reads` |
| Restoring a damaged backup | Every chosen backup file is checked against its size and SHA-256 before anything is written; any mismatch refuses the restore; incomplete or newer-format backups aren't offered | `a_damaged_or_edited_backup_restores_nothing` |
| Writing through links | Folders are created with `copier::ensure_folders` (refuses links and files in the way); a link or folder where a file would go is "unavailable" and left out | code review |
| Half-applied restore after a crash | Journal written before the first change; Undo works from what's on the disk (restored file identical → moved aside; original moved back) | `using_the_backups_copy…` (Undo restores the exact listing); `undo_keeps_files_changed_since_the_restore` |
| Undo destroying later play | A file changed since the restore is kept and reported ("partly undone") | `undo_keeps_files_changed_since_the_restore` |
| The frontend overwriting the restore | Refused while RetroBat (retrobat.exe, emulationstation.exe, emulatorlauncher.exe) or ES-DE runs, for restore and undo | tests (refused, nothing changed) |
| Acting on an old review | The preview's fingerprint (every file's place, decision and checksum) must still match | `restoring_adds_missing_files…` (stale review refused) |
| Commands used on other folders | Restore only accepts a folder directly inside the backup folder; undo ids are letters, digits and `-` and must belong to the system | code review; `using_the_backups_copy…` (`../../x` refused) |

**Real test (2026-11-09, owner-approved):** on `G:\RetroBat` with a test backup holding a new file and a different version of a real save state (`saves\nes\libretro.fceumm\1943 - The Battle of Midway (USA).state3`): preview add 1 / different 1; restore with "use the backup's copies": 1 added, 1 replaced, the system's state moved to `.retrodrive\backup\restore-…`; Undo: both put back. Listing of `G:\RetroBat` afterwards: identical outside `.retrodrive` (the save state byte-identical, same timestamp); new only: the restore record and the two test files moved to `.retrodrive\backup\restore-…-undo`. The real backup of the drive previewed as 1,876 same, 0 to do.

## 58. Addendum: Backup and restore for Batocera, Recalbox, RetroPie, ROCKNIX and Lakka (2026-11-09)
**Scope and authorization:** owner, after the research summary: "Build all 5 backup and restore for all 5" (ADR-092). Backup reads only. Restore writes to these systems under the rules approved for §57 (preview, keep the system's copy unless the backup's is chosen, system copies moved to `.retrodrive\backup` on the same share, journal first, Undo from the disk). New write places: Batocera's userdata, Recalbox's share, RetroPie's roms and configs shares (or USB folder), ROCKNIX's games-roms and config shares (or SD card folder), Lakka's Savefiles, Savestates, Configfiles, Remappings and Playlists shares.

| Risk | Mitigation | Evidence |
|---|---|---|
| Backing up or writing games where saves sit next to them (RetroPie, ROCKNIX) | Only save file types are taken from system folders (`is_save_file`); restore's `covered` accepts only those there | `retropie_and_rocknix_pick_saves_out_of_the_game_folders` (`snes/Mario.sfc` not backed up and not covered), `save_files_are_told_apart_from_games` |
| Restore writing outside what the backup holds | One rule list per system is used by both backup and restore (`covered`); everything else in §57 still applies | every new test asserts each backed-up file is covered; `a_damaged_or_edited_backup_restores_nothing` |
| Moving a file between shares (not possible) | Each area's replaced files go to `.retrodrive\backup` on that same share | code review (`moved_to` is below the item's own area) |
| The system overwriting a restore when it quits | These systems run elsewhere, so RetroDrive can't check; the review says to restore while it's off or restart it right after, and the result says to restart | code review |
| A share that can't be reached | Its files are left out with a note (preview and restore); nothing else is affected | `retropie_and_rocknix…` (USB RetroPie: settings note) |
| RetroBat/ES-DE behaving differently after the change | Same files as before | all earlier tests; read-only preview of the owner's RetroBat (saves 1,863, settings 8, game lists 5, media 11) and ES-DE (settings 1, game lists 1, art 1,204): unchanged |

**Testing limits:** no Batocera, Recalbox, RetroPie, ROCKNIX or Lakka hardware is available; each is tested on folders laid out like it (backup, lose a save, restore, compare). A first real use should be a backup, then a small restore with Undo.

## 59. Addendum: Maintenance checks (2026-11-09)
**Scope and authorization:** owner approved the outline ("great let's do it"; ADR-093). **Reads only**: the catalog, the Sync plan (stat and listing of the drive as Sync already does), the adapter's layout checks, the BIOS checker, the backup folder's manifests, and a walk of the installation folder for temporary-file names and the size of `.retrodrive\backup`. No new write path: every finding links to an existing, already reviewed feature (Sync, Plan, Backup, BIOS step/checker, Open backup folder).

| Risk | Mitigation | Evidence |
|---|---|---|
| Changing the drive | No writes; only existing read functions | `health::tests::a_new_system_gets_every_check_and_nothing_is_written` (listing before and after); real run on the owner's RetroBat: `G:` listing unchanged (0 differences) |
| Misleading counts | Uses the Sync plan's counts (its attention list is capped at 500; found on the owner's drive, where it showed "500 differ" instead of 57 differ and 588 can't be copied) | real run |
| Slow on a network share | Runs only when asked (Check health / Check again); same reads as Sync | code review |

## 60. Addendum: Backups as .zip files (2026-11-09)
**Scope and authorization:** owner chose "Export a backup as .zip" (ADR-096). Writes **only inside the backup folder on this PC**, and only new files: one `.zip` (save) or one new backup folder (add). No system is read or written; restoring from an added backup is the normal restore (§57) with its preview.

| Risk | Mitigation | Evidence |
|---|---|---|
| A zip that doesn't match the backup | Each file's size and SHA-256 checked against `manifest.json` while packing; the zip is read back and every file checked again before it gets its name | `backup_zip::tests::a_backup_saved_as_zip_and_added_back_is_identical_and_the_original_is_untouched`, `a_backup_that_changed_or_did_not_finish_is_not_zipped` |
| Replacing an earlier zip | Temporary name, then `rename_no_replace`; a taken name gets ` (2)` | same test (saving twice) |
| Leftover partial zip | Only RetroDrive's own temporary file (`.rdpart-` name, checked by `remove_own_temp`) is removed when saving fails | `a_backup_that_changed_or_did_not_finish_is_not_zipped` (no zip or temporary file left) |
| A zip that writes outside the backup ("zip slip") or adds programs | Only entries named on the zip's list are unpacked, each to `area/path` inside the new folder; the list must pass `system_backup::covered` (no `..`, drives, shares, `.retrodrive`, temporary names; only places that kind of backup holds); duplicates refused; checked before any folder is created | `only_listed_files_are_unpacked_and_only_to_places_a_backup_holds` (`../evil.exe` and `RetroBat.exe` entries ignored; bad lists refused with nothing created) |
| Damaged or hostile zip | Each file limited to its listed size plus one byte and checked by SHA-256; the list itself limited to 64 MB; format and "complete" required; a mismatch stops and leaves an "(incomplete)" folder, which isn't offered for restore | `a_damaged_zip_adds_nothing_complete` (changed contents, missing file, not a backup, not a zip) |
| Changing the original backup | The folder is only read | first test (listing before and after); real run on a copy of the owner's RetroBat backup: 1,876 files, 143 MB → 52 MB zip, added back and checked 1,876/1,876 |
| Running out of space | Free space checked first (backup size + 5% + 16 MB) | code review |

## 61. Addendum: Catalog tools (2026-11-09)
**Scope and authorization:** owner chose catalog tools as the next step ("Let's work on catalog tools") with removing entries and a library-only reset (ADR-097; the reset is a separate step with its own addendum). **Export** writes one new CSV file in Documents. **Removing** changes catalog records only; no drive is read or written.

| Risk | Mitigation | Evidence |
|---|---|---|
| Touching game files | No file operation in removal; only `DELETE`/`UPDATE` on catalog tables, in one transaction | `catalog_tools::tests::removing_a_folder_forgets_only_its_entries_and_never_touches_files`, `cleaning_up_missing_entries_forgets_only_files_a_scan_found_gone` (listings before and after) |
| Removing more than shown | Counts shown first; the removal recomputes them in its transaction and rolls back if they differ (e.g. a scan finished); refused while a scan, copy or import runs | `a_stale_preview_removes_nothing` |
| Entries another folder still holds (nested folders) | A location under another scanned folder on the same drive is kept and counted as "stays" | `a_folder_inside_another_keeps_entries_the_other_still_holds` |
| Broken references or leftovers | Parts that stay keep their parent; releases and games go only when nothing is left; foreign keys stay on (cascades for collections, art, details; copy history set to no file) | real run on a copy of the owner's catalog: 0 foreign key problems, 0 orphan releases/games/files; search index rows match games |
| Search index trigger set aside | Dropped and recreated from its own stored SQL inside the same transaction; any failure rolls back both | first test checks the trigger is back and search rows match |
| Overwriting a file on export | New name each time (`(2)` …), temporary file + `rename_no_replace`; only RetroDrive's own temporary file is removed on failure | `the_export_is_a_new_csv_with_one_row_per_game_file` |
| Spreadsheet formulas from file names | Cells starting with `= + - @`, tab or CR get a leading `'` | same test (`=cmd (USA).nes`) |
| Slow on a big library | Schema v28 indexes; search rows deleted in one pass | real copy: 42,335 files in 27 s, 118,443 in 37 s (before: not finished after 10 minutes) |

## 62. Addendum: Catalog reset and put back (2026-11-09)
**Scope and authorization:** owner chose the reset with "Library only" (ADR-098). Writes only in RetroDrive's own data folder: a new saved copy of the catalog, then changes to catalog rows. No drive is read or written.

| Risk | Mitigation | Evidence |
|---|---|---|
| Losing the library | A full copy is saved and checked (`quick_check`, schema version, counts) **before** anything is cleared; if saving fails nothing changes; saved copies are never deleted; Put back restores them | `catalog_reset::tests::reset_saves_a_copy_first_then_clears_only_the_library` (the copy holds the whole library), `putting_the_old_catalog_back_restores_the_library_and_saves_the_current_one` |
| Clearing more than the library | Fixed list of library tables; systems, copy history, designs, settings history, controller/gun choices, Theme Studio and reference catalog untouched; drives used by a system kept | first test (systems, designs, the system's drive and the copy-history record stay) |
| Put back losing what's there now | The current catalog is saved first (`before-put-back`) | second test (that copy lists the one-folder scan made after the reset) |
| A saved copy from an older RetroDrive, a damaged file, or another file | Only `retrodrive-before-*.sqlite` files next to the catalog; a working copy is migrated (the saved file unchanged); a file that can't be read changes nothing and saves nothing | `a_copy_from_an_older_version_is_brought_up_to_date_first`, `a_stale_preview_or_a_foreign_file_changes_nothing` |
| Resetting more than shown | Counts compared with the preview; refused while a scan, copy, import or drive change runs | `a_stale_preview_or_a_foreign_file_changes_nothing` |
| Broken references | One transaction; foreign keys checked before commit (deferred during put back); drives matched by id or fingerprint | real copy: 0 foreign key problems after reset and after put back |
| Triggers set aside | Dropped and recreated from their own stored SQL in the same transaction; titles and search index rebuilt; any failure rolls everything back | first test compares every trigger's SQL before and after; real copy: search index identical |
| Leftover working files | Only RetroDrive's own temporary files (marked names) are removed | tests check no temporary file is left |

## 63. Addendum: Faster restore preview (2026-11-09)
**Scope:** owner's list ("faster restore preview", ADR-099). No new write path. Backups record one more field per file; the restore preview reads file sizes and times instead of contents where they match.

| Risk | Mitigation | Evidence |
|---|---|---|
| A changed file counted as unchanged | Only when size **and** last-change time both equal what was backed up (a normal edit changes the time); the result is only that restore leaves the system's copy, its default; anything else is compared by SHA-256 | `system_restore::tests::unchanged_files_are_told_by_size_and_time_and_older_backups_by_contents` (edited contents with a new time: different) |
| Restore writing the wrong files | Unchanged: files restore adds (missing) or replaces (different, after the user chose the backup's copies) are decided as before; the fingerprint still covers every decision | existing restore tests |
| Older backups or older RetroDrive versions | The field is optional: backups without it are compared by contents; older versions ignore it (format stays 1) | same test (a list without times: compared by contents) |
| A damaged backup | The backup's own files are still checked by SHA-256 in every preview | `a_damaged_or_edited_backup_restores_nothing` |

## 64. Addendum: Setup wizard, finding systems (2026-11-09)
**Scope:** owner approved the wizard plan, including checking the systems' network names (ADR-100). **Reads only:** folder listings and the adapters' existing recognition checks. The only write is the existing **Add existing installation** catalog record, made when the user clicks Add and use.

| Risk | Mitigation | Evidence |
|---|---|---|
| Changing a drive while searching | Only `read_dir` and the adapters' `detect` (existence checks and small reads); no writes; links and junctions not followed | `find_systems::tests::systems_up_to_two_folders_deep_are_found_and_nothing_is_written` (listing before and after) |
| Slow on big drives | Top folder plus two levels, Windows folders skipped, at most 400 folders per level; a system found at a level isn't searched below | same tests; real run on this PC (C: and G:): 1.0 s, found `G:\RetroBat` 8.3.1.1 and `G:\ES-DE\ROMs` |
| A network name that never answers | Each checked on its own thread; whatever hasn't answered within 8 s is left out (Browse… still works) | `network_names_that_do_not_answer_are_left_out_by_the_deadline`; real run: 5.4 s with none on the network |
| Adding something the user didn't choose | Nothing is added automatically; Add and use calls the existing `add_installation` (which re-checks recognition) | in the app with a scratch catalog: RetroBat added and chosen, ES-DE left alone |
| Being pushed into the wizard | Offered once on first start with No thanks; Exit any time; never reopens by itself | in the app with a scratch catalog |


## 65. Addendum: Install a system for every supported system (2026-11-09)
**Scope:** owner asked for the install helper for Recalbox, ROCKNIX, Lakka, RetroBat and ES-DE (ADR-103). Same design as §24: RetroDrive reads the file the user chose and opens fixed pages and programs; it never downloads, installs or writes a system, and never writes to a card.

| Risk | Mitigation | Evidence |
|---|---|---|
| Opening an unexpected address | Only fixed official https addresses by id; anything else refused | `only_known_programs_and_pages_are_opened` |
| Writing the wrong drive | Unchanged: the user picks the drive in Raspberry Pi Imager or balenaEtcher, which warn before erasing | §24 |
| A wrong or damaged image | File type, cut-short and size checks as before; a different system's image is noted; checksum optional where the page shows one | `images_are_recognized_and_short_downloads_caught`, `image_names_say_what_they_are_for` |
| Installing a Windows app for the user | Not done: RetroDrive opens the official page and says how; the user installs it; Find it only searches (reading only, §64) | in the app: Find RetroBat listed G:\RetroBat and G:\ES-DE, nothing added |

## Test-only CI correction: confirmation timestamps

The existing same-second confirmation test used one newest timestamp across two source/destination pairs. If a slower runner created the pairs in different seconds, the older pair legitimately qualified for reuse and the assertion failed. The fixture now deliberately gives pairs different modified times and pins each deployment item's confirmation to its own pair's latest modified/created time. The staggered fixture reproduces the prior failure without sleeps. All same-second/later-confirmation/changed-content assertions remain in place.

Only the disposable test fixture in copier.rs changes. Production confirmation checks, copying, no-replace publication, rollback/deletion rules and hazards H1-H14 are unchanged. The complete Rust suite and strict Clippy are required before pushing this test correction; no user catalog or target files are used.

## 66. Addendum: "added from a .zip" note (2026-11-10)
**Scope:** ADR-106. **Add a backup from a .zip** (§60) also writes one small text file, `added-from-zip.txt`, holding the zip's file name, so the backup list can tell an added backup from the original.

| Risk | Mitigation | Evidence |
|---|---|---|
| Writing somewhere unexpected | Only inside the new backup folder RetroDrive just created (still named "(incomplete)"), before it is renamed; never in an existing folder, never on a system's drive | `a_backup_saved_as_zip_and_added_back_is_identical_and_the_original_is_untouched` |
| Changing what a backup holds | The note isn't on the backup's list; `manifest.json` is written exactly as it was in the zip; check, restore and save as .zip use only the list | same test: a re-saved zip has no note; `only_listed_files_are_unpacked_and_only_to_places_a_backup_holds` |
| A strange note shown in the app | Only the first line is read, at most 260 characters, shown as plain text | `system_backup::entry` |

## 67. Addendum: Save for RetroBat writes a new theme folder (2026-11-10)
**Scope:** ADR-113. Theme Studio writes a RetroBat theme it built (theme.xml, PNG pictures, credits.json, retrodrive-theme.json) to a new folder. It doesn't touch RetroBat, Customize or any existing theme; putting a theme on RetroBat is a later step with its own review.

| Risk | Mitigation | Evidence |
|---|---|---|
| Replacing or changing existing files | Always a new folder named after the theme (`free_folder` adds "(2)" etc.); files created with create-new; written as "<name> (incomplete)" and renamed without replacing; nothing deleted | `a_theme_goes_to_a_new_folder_and_never_replaces_one` |
| Writing outside the chosen folder | File names come only from the builder and must be plain relative names (lowercase letters, digits, `._-/`, no `..`, no leading slash); checked before anything is written | `only_plain_relative_file_names_are_written`, same test (`../escape.txt` refused) |
| Corrupt or partial output | Every file read back and compared; a failure leaves the "(incomplete)" folder and says so | `write_new_folder` |
| Packaging pictures that aren't the theme's, or damaged ones | Baked pictures re-checked by the PNG normalizer; other pictures must be declared by the stored theme and pass the store's integrity check; 32 MB budget; read-only transaction | `a_theme_is_built_from_baked_pictures_and_live_parts_without_storing_anything` |
| Opening arbitrary folders | Open folder only for absolute paths holding `retrodrive-theme.json` | `open_saved_theme` |

## 68. Addendum: Put on RetroBat (2026-11-10)
**Scope:** ADR-115. Theme Studio adds a saved theme to a RetroBat installation as a new theme folder and, if the user leaves the box ticked, makes it RetroBat's theme through Customize's existing theme switch.

| Risk | Mitigation | Evidence |
|---|---|---|
| Replacing or changing an existing theme | New folder only (`free_folder` adds "(2)"); create-new files; "(incomplete)" then rename without replacing; nothing deleted | `a_saved_theme_is_put_on_retrobat_only_as_listed_and_unchanged` |
| Copying unexpected or altered files | Only files listed in `retrodrive-theme.json`, each rechecked for size and SHA-256; plain relative names; 256 files and 32 MB at most | same test |
| Writing while another drive change runs | Holds the drive-change claim like other drive writes | `put_theme_on_retrobat` |
| Changing RetroBat's settings unsafely | The existing theme switch: file hash reviewed, refused while RetroBat runs, backed up, journaled, Undo in Customize (unchanged code) | Customize tests; ADR for the theme switch |

## 69. Addendum: theme clips (2026-11-10)
**Scope:** ADR-117. Theme Studio copies MP4 clips the user chooses into a `theme-videos` folder beside the catalog, serves them to the editor for preview, and copies them into saved themes.

| Risk | Mitigation | Evidence |
|---|---|---|
| Changing the user's file | The chosen file is only read; RetroDrive keeps its own copy | `clips_are_stored_once_by_content_and_checked_when_read` |
| Replacing or corrupting stored clips | Named by SHA-256; written as `.part` then renamed without replacing; an existing clip is reused only if it still matches; a damaged one is refused, never overwritten | same test |
| Non-video or oversized files | MP4 header check; 50 MB per clip; 1 GB in all | `only_mp4_clips_within_the_limit_are_accepted` |
| Serving other files through the media channel | `tv-` must be followed by a 64-character lowercase SHA-256; read only from the clip folder and re-checked | `video_store::read` (rejects `../secret`) |
| Packaging clips that aren't the theme's | A clip enters a saved theme only when the stored theme declares it and the stored copy matches its size | `a_clip_is_saved_only_when_the_theme_owns_it` |
