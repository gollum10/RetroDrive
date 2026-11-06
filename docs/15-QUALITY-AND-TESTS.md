# 15 | Quality Assurance, Threat Scenarios and Test Matrix

## Phase 1 test matrix
| Test ID | Scenario | Expected outcome |
|---|---|---|
| QA-001 | Index 1,000 files in user-approved test folder | Correct count; source unchanged; clear progress |
| QA-002 | Cancel scan midway | No false deletes; recoverable job status |
| QA-003 | Unplug already indexed USB drive | Location offline; all catalog entries retained |
| QA-004 | Reconnect with different Windows drive letter | Matched by reliable identity or confirmation; no duplication |
| QA-005 | Same title on SNES and Genesis | Separate game/release identities |
| QA-006 | Hash-identical files in two roots | One file identity where appropriate, two physical locations |
| QA-007 | `.cue/.bin`, `.m3u`, `.zip`, `.chd` fixtures | Proper associations or 'unknown', no unsafe assumptions |
| QA-008 | Long Unicode paths and spaces | Correct display and safe access |
| QA-009 | Locked/inaccessible file | Specific error, continue other files |
| QA-010 | Empty drive/folder | Accurate empty state, no fabricated health values |
| QA-011 | Deploy with existing different file | Preserve by default, explicit conflict choice |
| QA-012 | Pull destination midway through copy | No success manifest; resumable/cleanable partial state |
| QA-013 | Insufficient capacity | Preflight blocks execution |
| QA-014 | Source symlink escapes approved root | Block or request scoped explicit authorization |
| QA-015 | Disk letter changes after plan | Revalidate stable target identity before writing |
| QA-016 | Malicious archive traversal path | Prevent writes outside approved destination |
| QA-017 | Catalog DB migration from starter | Original rows preserved or migration safely aborted |
| QA-018 | Network share disconnect mid-scan | Offline/unknown state, no mass catalog removal |

## Automated coverage (2026-11-05)
Run with `cd src-tauri && cargo test`.

| Test ID | Covered by | Status |
|---|---|---|
| QA-001 | `scanner::tests::indexes_only_game_files_and_never_modifies_source` | Automated, small fixture; the 1,000-file run and progress display are still manual |
| QA-002 | `scanner::tests::cancelled_scan_never_marks_files_missing`, `cancelled_scan_keeps_files_already_recorded`; manual 2026-11-04: 40,000-file fixture cancelled after 5,109 files, all 40,000 stayed present | Automated + manual |
| QA-003 | `catalog::tests::availability_states` (unreachable volume → offline, rows kept) | Automated with a simulated volume; a real USB test is still manual |
| QA-004 | `scanner::tests::drive_letter_change_is_recognized_without_duplicates`, `db::tests::same_volume_under_a_new_letter_is_recognized`; manual 2026-11-04 in the app with the real C: fingerprint (catalog root set to Q:\, rescan recognized it, 0 new) | Automated + simulated manual; **a real USB letter change still needs testing** |
| QA-007 | `scanner::tests::cue_sheet_tracks_join_one_game`, `playlist_discs_join_one_game_in_order`, `rescans_keep_grouping_without_duplicates_or_false_missing`, `missing_track_is_a_warning_and_marks_the_game_incomplete`, `removing_a_playlist_returns_its_discs_to_separate_games`, `cancelled_scan_does_not_split_grouped_games`, `catalog::tests::multi_file_games_are_listed_once_with_their_total_size`; manual 2026-11-04 in the app (5 games from 12 files, 2-disc badge, Incomplete flag, warning text) | Automated + manual |
| QA-008 | `paths::tests::unicode_round_trip`, scanner Unicode fixture, `scanner::tests::paths_longer_than_260_characters_are_indexed` (this test found a duplicate-record bug, now fixed) | Automated |
| QA-009 | `scanner::tests::locked_files_are_reported_and_the_scan_continues`: an exclusively locked ROM is still indexed, and a locked cue sheet produces one error, leaving the scan incomplete | Automated |
| QA-010 | Empty catalog shows zeros (manual screenshot 2026-11-04) | Manual |
| RDS-006/007 | `adapters::tests::*` (contract through a mock adapter, read-only capabilities, Windows-safe folder names), `adapters::retrobat::tests::*` (detection including case-insensitivity and partial folders, layout checks, wiki path advice, folder mapping, `.fds`, arcade and unknown not guessed, multi-disc subfolders, `..` refused), `installations::tests::*` (inspect, add/list/forget, refusal, preview grouping, preview never writes, incomplete games left out); manual 2026-11-04 in the app with a mock RetroBat folder: refused a non-RetroBat folder, recognized RetroBat, saved it, and the preview put `.fds`→`roms\fds` and the 2-disc game→`roms\psx\Star Saga (USA)\`. The installation folder was verified unchanged afterwards | Automated + manual |
| RDS-004 | `catalog::tests::text_search_matches_inside_titles_and_platforms` (including literal special characters), `filters_combine`, `duplicate_candidates_need_same_platform_name_and_size`, `pages_and_sorting`, `filtered_counts_are_capped_but_unfiltered_are_exact`, `stats_summarize_the_library`; manual in the app with 39,300 files: search, paging, the duplicates filter and the Review shortcut | Automated + manual |
| QA-014 | Links are not followed and are reported (as warnings); `scanner::tests::references_outside_the_scanned_folder_are_refused` covers `..` escapes in playlists | Partial: creating a symlink in a test needs Windows Developer Mode |
| QA-011 (plan side) | `deployment::tests::existing_files_are_kept_or_skipped`: a different existing file is a conflict, kept, and the game isn't copied; same size is skipped; the installation is byte-for-byte unchanged | Automated + manual 2026-11-05 in the app (conflict shown with both sizes); the copy-time choice comes with RDS-010 |
| QA-013 (plan side) | `deployment::tests::fat32_rejects_files_over_4_gib_and_small_drives_dont_fit`: reserve applied, "doesn't fit", unknown space never "fits"; `volumes::tests::system_volume_space_is_measured` | Automated; blocking execution comes with RDS-010 |
| QA-014 (destination) | `deployment::tests::junctions_are_never_written_through` (a real junction), `links_and_folders_in_the_way_are_blocked`, `unsafe_relative_paths_are_rejected` | Automated |
| RDS-009 plan | `deployment::tests::plans_copies_by_system_and_never_writes` (counts, folders, cluster-rounded capacity, stable fingerprint, no changes), `a_subset_of_systems_can_be_chosen`, `a_scanned_installation_is_already_in_place`, `duplicates_are_copied_once_and_clashes_are_blocked`, `missing_files_and_unplugged_drives_are_left_out`; manual 2026-11-05 in the app: 3 to copy (7 files), 1 already there, 1 duplicate, 1 conflict, 3 new folders, NTFS capacity measured, unticking NES re-planned to 2 games, RetroBat folder unchanged | Automated + manual |
| QA-011 (copy side) | `copier::tests::already_there_files_are_compared_by_content`, `a_file_that_appears_before_the_rename_is_never_overwritten`, `rename_never_replaces`; manual 2026-11-05 (conflicting file untouched after a real copy) | Automated + manual |
| QA-012 | `copier::tests::cancelling_mid_file_leaves_no_partial_files_and_no_false_completion`, `interrupted_copies_are_cleaned_up_safely_on_the_next_start`, `a_temp_file_that_cannot_be_removed_stays_tracked`, `a_partly_copied_game_is_finished_by_running_again`; manual 2026-11-05: cancelled a 1 GB copy, no temp left | Automated + manual; **a real USB unplug still needs testing (docs/18 §6)** |
| QA-013 (copy side) / QA-015 (write side) | `copier::tests::nothing_is_written_when_the_plan_changed_or_space_is_short` (stale fingerprint, no space, unmeasured space, a different volume at the letter: all refused, nothing written); manual 2026-11-05: stale plan refused in the app | Automated + manual |
| QA-016 / QA-014 (copy side) | `copier::tests::copies_never_go_through_a_junction`, `inside_checks_work_for_drive_roots_and_folders` | Automated |
| RDS-010 copy | `copier::tests::copies_verify_and_write_a_manifest`, `a_copy_that_does_not_verify_is_discarded`, `a_source_that_changes_during_the_copy_fails_that_file`, `a_hundred_unicode_names_with_spaces_copy_and_verify` (docs/08 acceptance), `paths_longer_than_260_characters_are_copied`, `reading_back_past_the_cache_matches_a_normal_read`, `content_comparison_spots_a_single_changed_byte`; manual 2026-11-05: 3 games / 7 files copied, all hash-identical to sources, manifest written | Automated + manual |
| Arcade choice (RDS-016) | `adapters::retrobat::tests::arcade_games_follow_the_chosen_emulator_and_keep_their_names`, `deployment::tests::arcade_games_are_planned_once_an_emulator_is_chosen`, `installations::tests::add_list_and_forget_installations` (set/clear/reject) | Automated |
| RetroBat version | `adapters::retrobat::tests::a_fresh_install_reports_its_version` (fresh 8.3.1.1 layout without `retrobat.ini`; fallback file; junk rejected) | Automated; layout confirmed on a real install 2026-11-05 |
| Real-world copy (owner, 2026-11-05) | 96,750 games scanned from a NAS; NES plan (4,333 games: 3,436 to copy, 847 duplicates, 50 blocked as 25 same-name pairs); 3,436 files copied to RetroBat 8.3.1.1 on a 2 TB exFAT drive, all verified, no temp files left; 5 random copies re-hashed against recorded checksums | Manual |
| Library views | `catalog::tests::multi_file_games_are_listed_once_with_their_total_size` (system summary counts a multi-disc game once, sizes include discs); manual 2026-11-05 with a separate test catalog (`RETRODRIVE_DATA_DIR`): Systems cards, NES grid, list toggle | Automated + manual |
| Reference catalog | `reference::tests::imports_a_catalog_and_matches_by_name_variant_and_title` (art columns ignored, no art column in schema, library titles unchanged, ambiguous titles not guessed, genre supplement keeps existing genres, new games matched after a scan, removal), `unhelpful_files_get_clear_errors`, `reads_xlsx_workbooks` (fixture `testdata/reference_small.xlsx`), `title_keys_ignore_tags_articles_and_punctuation` | Automated |
| Reference catalog (real data, 2026-11-05) | Copy of the owner's catalog (116,405 games) in a separate data folder: 63,793 entries, 47,840 games matched, 17,232 genres added; Systems page, Arcade grid with catalog titles, list Year/Genre columns | Manual |
| Box art linking (ADR-025) | `scanner::tests::box_art_is_linked_from_gamelists_and_folder_conventions_without_copying` (gamelist with `&amp;` names, folder convention, outside-folder and marquee refused, no files created, user pick survives rescans and wins, removal keeps the image file, nothing served from a disconnected drive, non-images refused), `media::tests::reads_games_and_images_from_a_gamelist`, `relative_paths_stay_on_their_volume` | Automated |
| Box art (real data, 2026-11-05) | Owner's NAS `roms II\nes` scanned into a separate test catalog: 1,520 of 1,520 images linked; grid shows the Skraper screenshots; Change art and Remove my art checked in the app | Manual |
| More systems (ADR-026) | `classify::tests::more_systems_and_the_owners_folder_names_are_recognized`, `scanner::tests::rescans_move_games_to_newly_recognized_systems_and_mark_bios`, `adapters::retrobat::tests::new_systems_map_to_retrobat_folders_and_bios_is_never_placed`; owner's catalog snapshot: 60,554 of 60,558 Unclassified recognized, none rejected | Automated + data check |
| Leave out / duplicate cleanup (ADR-027) | `duplicates::tests::duplicates_are_left_out_without_touching_files_and_can_come_back` (preview, keep-shorter-path, counts/listing/systems exclude left-out, files untouched, rescans keep marks, bring back, clash resolution), `a_missing_copy_is_never_the_one_kept`; manual 2026-11-05 in a test catalog: banner, preview (3 groups), cleanup, 8 files still on disk | Automated + manual |
| A–Z and leading-slash playlists | `catalog::tests::letters_filter_titles_and_hash_means_digits_or_symbols` (found a case bug in the # filter), `scanner::tests::playlists_with_a_leading_slash_mean_the_playlists_own_folder`; `references_outside_the_scanned_folder_are_refused` still passes; manual 2026-11-05: Systems A–Z with letter jumps, games letter filter | Automated + manual |
| Copy choices (step 5b, docs/18 §7) | `copier::tests::replace_moves_the_original_to_the_backup_folder_then_copies`, `a_failed_replace_puts_the_original_back`, `cancelling_during_a_replace_puts_the_original_back`, `keep_both_copies_under_the_next_free_number`, `an_interrupted_replace_is_put_right_on_the_next_start`; `deployment::tests::clash_choices_pick_a_version_or_keep_both`, `conflict_choices_and_multi_file_games`; manual 2026-11-05: Replace + Keep both in the app, verified on disk | Automated + manual |
| Parallel copies (docs/18 §11) | `copier::tests::parallel_and_one_at_a_time_copies_end_the_same` (66 games, 78 files: same files and contents with 1 and 4 workers, all verified, manifest complete), `cancelling_a_parallel_copy_leaves_only_verified_files` (no temp files; every file on the drive is a verified copy; the rest cancelled or not attempted), `a_parallel_copy_never_overwrites_a_file_that_appears`, `big_files_are_copied_one_at_a_time`; every other copier test runs with 4 workers when it has several games; repeated 15× with no failures | Automated |
| Undo a replace (docs/18 §10) | `restore::tests::undo_puts_the_original_back_and_keeps_retrodrives_copy`, `undo_refuses_when_anything_changed` (drive away, same-size different contents caught by SHA-256, backup missing, path outside the installation; nothing touched), `an_interrupted_undo_is_finished_or_rolled_back_at_startup` (crash before any move, after the first move, after both; nothing while the drive is away; the copier's recovery leaves undo items alone); in the app 2026-11-06: conflict → Replace → copy → Backups → Undo replace, drive checked file by file | Automated + manual |
| Shown titles (schema v14) | `reference::tests::the_library_searches_and_sorts_by_the_title_it_shows` (catalog title shown and sorted, searchable with 2+ letters, A–Z follows it, kept after a file-title change, reverted when the catalog is removed); `catalog::tests::benchmark_real_catalog` (ignored; timings above) | Automated |
| Batocera, preview only (RDS-008) | `adapters::batocera::tests` (game folder detected case-insensitively; boot partition, drive root and unfinished folders give hints and aren't saved; RetroBat folders aren't claimed; layout checks; Batocera folders and CPS via the arcade choice); `copier::tests::preview_only_platforms_are_never_copied_to` (plan shown, copier refuses, folder unchanged, nothing journaled); `adapters::tests::shipped_adapters_are_read_only_and_unique`; in the app 2026-11-06 with a mock share: boot partition hint, recognized game folder, plan with `roms\nes`/`roms\snes`, CPS2 held back, preview note instead of Copy, share unchanged | Automated + manual (no real Batocera yet) |
| Collections (step 7) | `collections::tests::collections_hold_games_without_touching_them` (case-insensitive unique names, adding twice keeps one entry, filter by collection, rename, delete leaves games and files); `deployment::tests::a_plan_can_be_limited_to_a_collection` (plan holds only the collection's games and its fingerprint differs); in the app 2026-11-05: "Add all shown" with the SNES filter made a 2-game collection and the plan dropped from 5 games to those 2 | Automated + manual |
| Faster small-file copies (step 6) | `copier::tests::leftovers_are_found_by_name_even_without_journal_entries` (temp files and replace backups found by their marked names when the journal lost its last entries); `copy_benchmark_small_files` (ignored benchmark) | Automated |
| QA-017 | `db::tests::v0_rows_are_preserved_and_backed_up`, `v1_catalog_upgrades_to_v2_keeping_data`, `v2_catalog_upgrades_to_v3_keeping_references` | Automated |
| QA-015 (read side) | `scanner::tests::a_different_drive_at_the_same_letter_does_not_inherit_files`, `volumes::tests::system_volume_has_a_fingerprint`; Known drives showed "now a different drive" | Partial: identity is checked; re-checking before *writes* comes with deployment |

RDS-002 tests: progress reporting and final counts, batched commits giving the same catalog as a single pass, a failed batch leaving no open transaction, job lifecycle and failure recording, and recovery of interrupted jobs (`jobs::tests`). Manual crash test 2026-11-04: force-killed the app mid-scan; on relaunch the job and folder showed `interrupted` and no catalog rows were lost.

Other regression tests: rescans don't create duplicates, rescanning a subfolder only affects that subfolder, manual titles survive a rescan, and stored paths never have the `\\?\` prefix.

## Later raw disk operations (not in starter)
- Protect Windows system/boot and source disks.
- Verify exact physical disk identity before raw write.
- Simulate device swap after user selects it.
- Corrupt download checksum: fail closed.
- Power loss/incomplete write: no false success, recovery guidance.
- Partition/resizing failure: operator confirmation and backup approach documented.
- Run only against approved disposable test devices, never real user data.

## Performance tests
Baseline 10k, 100k and 1M file metadata rows using synthetic fixtures; record scan rates, search latency, memory use, query plans and cancellation responsiveness. Numbers should be measured, not assumed.

### Measured 2026-11-04 (RDS-004)
Command: `cd src-tauri && cargo test --release benchmark_100k -- --ignored --nocapture`. Setup: 100,000 synthetic files across 6 platforms, 2,000 in duplicate pairs, in-memory SQLite, Windows 11 dev PC.

| Query | Before tuning | After |
|---|---|---|
| First page, no filter | 523 ms | **1.7 ms** |
| Page 500 (offset 50,000) | 602 ms | 124 ms |
| Search "puzzle" (14,286 matches) | 185 ms | 140 ms |
| Search "adventure 0999" (100 matches) | 43 ms | **2.6 ms** |
| Search "ad" (2 characters, matches everything) | 539 ms | 37 ms (count capped) |
| Platform filter (16,333 matches) | 156 ms | 40 ms |
| Duplicates only (2,000) | 460 ms | 74 ms |
| Sort by size | 302 ms | **1.6 ms** |
| Library stats | 247 ms | 24 ms |
| System summary (Systems page, 2026-11-05) | — | 262 ms (only when the page opens) |

After RDS-005 (listing primary files only), rerun the same day: first page 5.4 ms, exact search 4.3 ms, size sort 3.0 ms, platform 36 ms, duplicates 77 ms, stats 40 ms, offset 50,000 159 ms. An intermediate version took 70 ms for the first page because SQLite chose the role index; a unary `+` on that term fixed it.

The query plans (EXPLAIN QUERY PLAN) confirm index-ordered walks (`SCAN g USING INDEX idx_games_title`). **Scan cost:** a first-time scan of 39,300 files took 12 s in the dev app, against about 3 s before the search index; rescans of known files don't touch the index. Not yet measured: 1M rows, memory use.

### Copy speed, many small files (step 6, 2026-11-05)
Command: `cd src-tauri && cargo test --release copy_benchmark_small_files -- --ignored --nocapture` (2,000 × 40 KB files, on-disk catalog, internal NVMe, Windows Defender on).

| Configuration | Files/s |
|---|---|
| Before (journal with SQLite `synchronous = FULL`) | 57–62 |
| Measurement only: journal `NORMAL` | 101 |
| Measurement only: no `sync_all` on the game file | 59 |
| Measurement only: no read-back check | 98 |
| Journal `NORMAL` during copies (step 6) | 101 |
| **Shipped (2026-11-06): 4 games at once** (`RETRODRIVE_BENCH_WORKERS=4`) | **359–382** |
| Measurement only: 2 at once | 205 |
| Measurement only: 8 at once | 485 |

### Library pages on the owner's catalog (2026-11-06)
Command: copy the catalog, then `RETRODRIVE_BENCH_DB=<copy> cargo test --release benchmark_real_catalog -- --ignored --nocapture` (145,181 games, schema v14, warm runs). The before column is main at schema v13 on the same data.

| Page | Before | After |
|---|---|---|
| First page, title sort | 390 ms | 15 ms |
| Platform sort | 383 ms | 11 ms |
| Letter M | 2.9 s | 224 ms |
| Letter # | 3.2 s | 70 ms |
| SNES | 240 ms | 189 ms |
| Search 'street fighter' | 34 ms (267 found) | 36 ms (497 found: catalog titles too) |
| Search 'sf' (2 letters) | 3.3 s | 165 ms |
| Page 500 (offset 50,000) | 1.6 s | 1.2 s |

Causes: the total count scanned every file to skip left-out ones (fixed by a partial index), and without table statistics SQLite counted letter and short-text filters by walking every file (fixed by `ANALYZE` and counting from the games table when only game filters are used). Deep offsets still walk the skipped rows.

The remaining cost is mostly opening each new file again for the read-back check (Windows Defender scans new files when they're opened); it stays, because the read-back is a core safety check. Since 2026-11-06 four games are copied at once, which overlaps that wait (docs/18 §11). Four, not eight: USB drives gain less than this internal NVMe drive, and fewer parallel writes are kinder to them.

## Security review checklist
For any change to file writes, re-run the docs/18 hazard table. Check the dependency license gate (`scripts/check-licenses.py`, LEGAL-014), path traversal, symlink/junction escape, signed/trusted downloads, secret handling, rate limits, frontend IPC validation, backup recovery, unsafe shell usage and dependency vulnerabilities.

## Release signoff record (template)
Version, commit SHA, target Windows environment, automated suite result, manual matrix result, known limitations, code reviewers, security approver for elevated operations, packaging/signing status, and release notes.
