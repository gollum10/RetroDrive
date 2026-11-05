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

## Automated coverage (2026-11-04)
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
| RDS-004 | `catalog::tests::text_search_matches_inside_titles_and_platforms` (including literal special characters), `filters_combine`, `duplicate_candidates_need_same_platform_name_and_size`, `pages_and_sorting`, `filtered_counts_are_capped_but_unfiltered_are_exact`, `stats_summarize_the_library`; manual in the app with 39,300 files: search, paging, the duplicates filter and the Review shortcut | Automated + manual |
| QA-014 | Links are not followed and are reported (as warnings); `scanner::tests::references_outside_the_scanned_folder_are_refused` covers `..` escapes in playlists | Partial: creating a symlink in a test needs Windows Developer Mode |
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

After RDS-005 (listing primary files only), rerun the same day: first page 5.4 ms, exact search 4.3 ms, size sort 3.0 ms, platform 36 ms, duplicates 77 ms, stats 40 ms, offset 50,000 159 ms. An intermediate version took 70 ms for the first page because SQLite chose the role index; a unary `+` on that term fixed it.

The query plans (EXPLAIN QUERY PLAN) confirm index-ordered walks (`SCAN g USING INDEX idx_games_title`). **Scan cost:** a first-time scan of 39,300 files took 12 s in the dev app, against about 3 s before the search index; rescans of known files don't touch the index. Not yet measured: 1M rows, memory use.

## Security review checklist
Check path traversal, symlink/junction escape, signed/trusted downloads, secret handling, rate limits, frontend IPC validation, backup recovery, unsafe shell usage and dependency vulnerabilities.

## Release signoff record (template)
Version, commit SHA, target Windows environment, automated suite result, manual matrix result, known limitations, code reviewers, security approver for elevated operations, packaging/signing status, and release notes.
