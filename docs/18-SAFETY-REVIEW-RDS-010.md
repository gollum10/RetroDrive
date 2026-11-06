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
- **Before turning it on (RDS-022):** review no-replace rename and unbuffered read-back over SMB, the share disappearing mid-copy (Batocera shut down), drive identity for `\\BATOCERA\share` (no volume serial guarantees), and exFAT game drives; test on a real Batocera.

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

