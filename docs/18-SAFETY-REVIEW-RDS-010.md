# 18 | Safety Review: First Disk Writes (RDS-010)

This is the dedicated safety review that AGENTS.md requires before RetroDrive writes to a user's drive. It covers the staged copier in `src-tauri/src/copier.rs` and its wiring in `lib.rs`. Raw disk imaging, formatting and partitioning are **out of scope** and still forbidden (docs/10).

_Review date: 2026-11-05. Status: **implemented, tested and used for real** (3,935 games copied and launched); owner sign-off complete (§6)._

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
- [x] Real RetroBat: copied games appear and launch (owner, 2026-11-05: NES and Game Gear copied with RetroDrive, launched in RetroBat 8.3.1.1).
- [x] Residual risks in §4 accepted by the owner for now (2026-11-05).
