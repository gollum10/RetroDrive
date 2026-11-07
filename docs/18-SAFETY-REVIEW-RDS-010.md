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

