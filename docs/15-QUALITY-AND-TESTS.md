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

## Security review checklist
Check path traversal, symlink/junction escape, signed/trusted downloads, secret handling, rate limits, frontend IPC validation, backup recovery, unsafe shell usage and dependency vulnerabilities.

## Release signoff record (template)
Version, commit SHA, target Windows environment, automated suite result, manual matrix result, known limitations, code reviewers, security approver for elevated operations, packaging/signing status, and release notes.
