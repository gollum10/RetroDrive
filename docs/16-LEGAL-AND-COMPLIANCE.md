# 16 | Legal and Compliance Assessment

**Project:** RetroDrive  
**Document:** `docs/16-LEGAL-AND-COMPLIANCE.md` (received as `legal.md`)  
**Version:** 1.0  
**Date:** November 4, 2026  
**Status:** Preliminary legal-risk assessment; requires qualified attorney review before commercial launch

> **Editor's note (2026-11-04, added when filing this document):** The assessment was written when the product was "RetroDrive Studio" published by an earlier publisher name. The product was later renamed **RetroDrive** and an earlier publisher name was removed (ADR-017). Only names were changed: "RetroDrive Studio" became "RetroDrive", and the publisher line and an earlier publisher name references were removed or replaced with "RetroDrive" / "the project". Everything else is the original text. The branding requirement in §8 originally read "RetroDrive Studio by an earlier publisher name", and LEGAL-008 originally named "RetroDrive Studio"; both now name **RetroDrive**, so the trademark search applies to the new name. Status of each LEGAL item: see §18 (added) and `Features.md`.

> **Disclaimer:** This document is a product-planning risk assessment, not legal advice or a legal opinion. It focuses primarily on U.S. considerations. Laws, license versions, terms of service, and permissions can change. Verify the current terms for every specific software version, asset, service, jurisdiction, and distribution channel before launch. Do not treat a disclaimer, user acknowledgment, or technical separation as permission to do something otherwise prohibited.

## 1. Executive summary

RetroDrive is a proposed commercial desktop application that helps customers create and manage retro-gaming drives, install or configure compatible systems, catalog and deploy games they supply, manage metadata and artwork, verify BIOS requirements, configure controllers, back up collections, and troubleshoot issues.

**The core concept is not inherently unlawful.** Most risks come from what the application actually **bundles, downloads, redistributes, copies, modifies, or represents**. The project should develop an independent management product rather than packaging copyrighted games, proprietary BIOS/firmware, or restricted third-party distributions.

**Recommended launch position:** Sell RetroDrive's own drive-management, cataloging, backup, automation, deployment, and diagnostics functionality. Support third-party systems through separately reviewed adapters. Obtain needed permissions before distributing, bundling, or automating installation of software with restrictions.

## 2. Preliminary risk register

| Topic | Preliminary risk | Recommended control |
| --- | --- | --- |
| Unauthorized ROM distribution | High | Never bundle or link to unauthorized commercial game downloads |
| Proprietary BIOS/firmware | High | Validate files supplied by users; do not ship unlicensed proprietary firmware |
| Emulator and core licensing | High | Inventory and audit every exact binary, component, dependency, and version |
| RetroBat commercial restrictions | High | Review terms and obtain written permission before commercial bundling or restricted integration |
| Batocera redistribution/commercialization | High | Examine relevant licenses, notices, and project terms before bundling or modifying |
| Copyrighted artwork and metadata | Medium | Use licensed providers, comply with attribution and usage restrictions |
| Trademark and naming | Medium | Use descriptive compatibility names; avoid endorsement implications; perform clearance search |
| Destructive drive operations | Medium to High | Protect system disks, require explicit confirmation, test recovery and verification |
| Privacy and telemetry | Medium | Prefer local-first data processing; provide truthful privacy disclosures and controls |
| Third-party download integrity | Medium to High | Download from trusted sources, validate hashes/signatures, log provenance |

These ratings are prioritization aids, not findings that a proposed feature is illegal.

## 3. ROMs and copyrighted video games

### Issue

Games, ROM images, executable data, sound, graphics, and manuals may be protected by copyright. The fact that a game is old, unavailable at retail, or owned on a physical cartridge does **not** automatically authorize downloading an otherwise unauthorized copy from a third-party site.

U.S. software archival and maintenance exceptions have conditions; anti-circumvention restrictions may apply separately. The actual legal status of user-made copies depends on facts and jurisdiction.

### Product requirements

- Do **not** include unauthorized commercial ROMs with RetroDrive or preloaded images.
- Do **not** embed catalogs, search tools, mirrors, or integrations intended to obtain unauthorized game downloads.
- Support homebrew, public-domain, and properly licensed titles, plus files supplied by the user where the user has the rights necessary for the intended operation.
- Make game import, organization, duplication detection, indexing, and deployment user-controlled.
- Avoid workflows designed to defeat DRM or copy-protection measures.
- Use clear terms explaining that users are responsible for rights to content they import or deploy.
- Do not assume that a click-through warning makes an unauthorized copy or distribution legal.

**Design implication:** Keep the ROM Library Manager and Smart Deployment Engine, with user-directed file operations and safeguards.

## 4. BIOS and firmware

Proprietary console BIOS and firmware files can be protected by copyright and other restrictions. Hash checking and compatibility diagnostics are distinct from redistributing the underlying files.

### Product requirements

- Never bundle proprietary BIOS or firmware without an appropriate license.
- Allow users to select their own BIOS/firmware files.
- Check filenames, supported versions, checksums, hashes, and expected paths where permitted.
- Clearly show whether a required file is missing, incompatible, or unverified.
- Never direct users to infringing download sources.
- Distinguish legally redistributable firmware from proprietary components in the license inventory.

## 5. RetroBat licensing and commercial use

**Priority: High.** RetroBat's published project materials have described restrictions on commercial use and selling machines or drives with RetroBat preinstalled. Project materials have referenced LGPL-3.0 for certain team-authored code and CC BY-NC-SA 4.0 for other materials. The legal effect of any individual statement depends on the actual license for each component and distribution, so do not infer that one README statement overrides all component licenses.

### Recommended handling by use case

| Activity | Initial position |
| --- | --- |
| Manage an installation independently obtained by the user | Potentially viable; review the exact integration and terms |
| Link to RetroBat's official project/download page | Generally lower risk, but verify link and branding rules |
| Automatically download/install RetroBat within a paid application | Obtain legal review and ideally written permission |
| Bundle or redistribute RetroBat with RetroDrive | Do not do so without the necessary rights and permissions |
| Sell preloaded drives containing RetroBat | Avoid unless specifically authorized |

**Action:** Contact RetroBat maintainers and seek written clarification/permission for proposed commercial integration and distribution scenarios.

## 6. Batocera and other emulation projects

Batocera's own published license and project guidance should be reviewed before commercial redistribution, use in preloaded drives, or modifications. Supporting user-installed Batocera is not automatically the same thing as redistributing it.

Likewise, RetroArch, individual Libretro cores, emulators, launchers, and utilities may carry **different licenses**. Some licenses and special permissions may restrict commercial use; GPL/LGPL and other copyleft licenses can impose obligations when distributing covered software.

### Required approach

- Audit the **exact version and license** of each separately distributed component and core.
- Distinguish independent management/integration from bundling, linking, modifying, and redistribution.
- Comply with applicable license text, notices, source-offer or source-distribution requirements, and attribution obligations.
- Do not assume that an umbrella project's license covers every included emulator or core.
- Maintain adapter boundaries so third-party components can be reviewed, replaced, or excluded independently.

### ES-DE opportunity

The Windows/macOS/Linux versions of ES-DE have been identified by its project documentation as MIT-licensed, while its Android distribution has different commercial characteristics. ES-DE may be a useful frontend option, but assess the exact version, platform, bundled assets, and any additional emulators independently.

## 7. Artwork, screenshots, manuals, videos, and metadata

Game covers, promotional screenshots, publisher logos, manuals, and video previews can remain copyrighted regardless of game age or internet availability.

### Product requirements

- Integrate with metadata providers only after reviewing their licensing and commercial API terms.
- Record source/provider, attribution, content identity, permitted usage, and applicable terms for retrieved assets.
- Respect API usage limits, caching restrictions, rate limits, and required credit.
- Let users supply their own artwork.
- Do not create a redistribution database of third-party artwork without appropriate authorization.
- Use provider adapters so a licensing change does not require replacing the entire library.

**Provider to evaluate:** IGDB, which has published commercial integration/partnership information. Confirm current terms before integration.

## 8. Trademarks, branding, and product naming

References to Nintendo, Sega, PlayStation, Xbox, and other system names can be useful to describe compatibility, but avoid misleading use of logos or trade dress and anything suggesting sponsorship, licensing, or endorsement.

### Requirements

- Brand the product as **RetroDrive** (see editor's note).
- Use third-party brand names descriptively where necessary for compatibility.
- Avoid using protected logos as if they are our own branding.
- Add a compatibility/independence statement where appropriate.
- Perform a U.S. trademark search, plus relevant common-law, domain, and international searches, for **RetroDrive** before investing heavily in the name.

**Proposed disclaimer (requires review):**

> RetroDrive is an independent retro gaming management application. All third-party product names and trademarks belong to their respective owners. Compatibility references do not imply sponsorship, affiliation, or endorsement.

A disclaimer does not cure otherwise misleading branding or infringement.

## 9. Drive formatting, flashing, partitioning, and data-loss liability

RetroDrive may perform destructive writes to physical storage devices. Selecting the wrong disk could cause major data loss. This is a product-liability, contractual, quality-assurance, and customer-trust risk.

### Mandatory safety controls

1. Identify target drives by model, size, serial number where available, bus, physical disk ID, and partition layout.
2. Protect the Windows boot/system disk and other known critical disks by default.
3. Never rely on drive letters alone as proof of target identity.
4. Show exactly which partitions and files will be erased or overwritten.
5. Require explicit, affirmative confirmation immediately before destructive work.
6. Provide a backup recommendation and feasible backup/restore paths.
7. Make safe operations idempotent or recoverable where practicable; log actions and results.
8. Validate downloads and written images using supported checksums, signatures, or read-back checks.
9. Test device unplugging, power loss, cancellation, insufficient space, and permission failures.
10. Consider an independently reviewed privileged helper for low-level disk operations with narrowly scoped permissions.

Have counsel review user terms, warranty language, limitation of liability, and suitable commercial insurance. No checkbox or disclaimer substitutes for sound engineering safeguards.

## 10. Privacy and telemetry

A local-first application may still handle personal information, file paths, library contents, device identifiers, and usage telemetry. If such data is collected, transmitted, or retained, relevant privacy laws and contractual obligations may apply.

### Recommended controls

- Keep the core library and game index local by default.
- Make telemetry opt-in where appropriate and minimize the collected information.
- Explain what metadata/API queries disclose to external providers.
- Publish an accurate privacy policy covering data, retention, requests, and third-party services.
- Secure any future cloud accounts, synchronization, and diagnostics uploads.
- Do not transmit full local ROM paths, filenames, or device identifiers unnecessarily.

## 11. Open-source compliance and software supply chain

Before a public or paid release, maintain a **third-party component manifest** for software and media shipped or fetched by the product.

Recommended fields:

| Field | Description |
| --- | --- |
| Component | Project / package / emulator / core name |
| Version | Exact release or commit |
| Source | Official project and download URL |
| License | SPDX identifier or applicable full terms |
| Distribution mode | Linked, downloaded by user, bundled, modified, or not shipped |
| Commercial rights | Confirmed, restricted, requires permission, or unresolved |
| Obligations | Notices, attribution, corresponding source, license text, etc. |
| Integrity | Trusted checksum, signature, provenance |
| Approval | Reviewer, review date, documentation link |

**Release gate:** No component with unresolved redistribution, commercial-use, or source-code obligations may be bundled in a commercial installer.

## 12. Design adjustments approved in principle

| Planned module | Safer initial implementation |
| --- | --- |
| Drive Manager | Independent RetroDrive disk and storage functionality |
| Master Game Library | Index and organize user-supplied files; no ROM distribution |
| Game Deployment | User-selected copies with previews, verification, and safeguards |
| Batocera Adapter | Primarily manage existing installations; review installers/redistribution |
| RetroBat Adapter | Primarily manage existing installations; seek clarification for commercial integration |
| ES-DE Adapter | Evaluate licensing as a potential frontend integration |
| Emulator Downloads | Only approved components after license audit |
| Artwork & Metadata | Authorized providers with attribution and provenance |
| BIOS Manager | User-supplied firmware diagnostics only |
| Maintenance Center | Independent backups, diagnostics, and supported config changes |

### Business model implications

The prospective free/pro tiers should monetize **RetroDrive's own value-added features**, such as collection management, backups, batch operations, profiles, and diagnostics. Avoid selling access to free third-party distributions as though they belong to RetroDrive.

Separating components technically can reduce risk but does not by itself guarantee legal compliance; actual use and distribution behavior matters.

## 13. Compliance backlog for GitHub

- [ ] **LEGAL-001:** Inventory every dependency, emulator, core, firmware component, image, and asset.
- [ ] **LEGAL-002:** Review RetroBat license, project terms, and all commercially relevant integration workflows.
- [ ] **LEGAL-003:** Contact RetroBat maintainers for written commercial permission/clarification.
- [ ] **LEGAL-004:** Review Batocera licenses and redistribution/commercialization guidance.
- [ ] **LEGAL-005:** Audit every supported emulator/core and third-party installer separately.
- [ ] **LEGAL-006:** Establish prohibited ROM/BIOS distribution rules and UI requirements.
- [ ] **LEGAL-007:** Evaluate metadata providers, caching requirements, artwork licensing, and attribution.
- [ ] **LEGAL-008:** Conduct trademark clearance for RetroDrive and review brand usage.
- [ ] **LEGAL-009:** Draft and review EULA, third-party notices, privacy policy, and warranty language.
- [ ] **LEGAL-010:** Implement and test destructive disk-operation protection and recovery.
- [ ] **LEGAL-011:** Implement signed/hashed download verification and software provenance logging.
- [ ] **LEGAL-012:** Obtain qualified IP/software-license counsel review before commercial launch.
- [ ] **LEGAL-013:** Review insurance, customer support, refunds, consumer protection, and distribution-channel terms.
- [ ] **LEGAL-014:** Add a mandatory license/security approval gate to release CI and documentation.

## 14. Decisions requiring legal sign-off

1. **Automatic download of third-party installations:** Which named projects and versions may be downloaded, installed, cached, and redistributed by a commercial application?
2. **Commercially preloaded drives:** Will RetroDrive ever ship physical drives with gaming systems or content installed? If so, obtain specific distribution rights and reassess liability.
3. **Artwork delivery:** May third-party artwork be cached or redistributed, or only fetched by a user's own API credentials?
4. **ROM copying and migration:** Which workflows create legal risk based on copying, format conversion, or protection circumvention?
5. **Proprietary versus open-source licensing:** Which libraries, linking arrangements, or modifications impose source-disclosure and other obligations?
6. **Brand clearance:** Is the name RetroDrive available and defensible in target markets?
7. **Supported countries:** Which jurisdictions beyond the United States will be offered downloads or sales?

## 15. Recommended sequencing

### Immediately, during development

- Keep the desktop product, local catalog, read-only drive detection, and user-controlled file management independent of third-party distributions.
- Record each third-party component and license as soon as it is considered for integration.
- Implement disk safety protections before enabling destructive operations.
- Define metadata-provider interfaces without committing to an unapproved artwork source.

### Before public beta

- Test all operations on disposable storage, including failure/recovery scenarios.
- Complete initial license inventory and relevant trademark search.
- Establish privacy, telemetry, download verification, and third-party notices.
- Obtain review for any embedded or automatically installed third-party software.

### Before paid/commercial launch

- Secure needed written permissions.
- Complete counsel review of licensing, terms, EULA, privacy, branding, and any game/firmware workflows.
- Audit commercial release artifacts for prohibited bundled ROMs, BIOS files, artwork, and restricted software.
- Confirm appropriate insurance and customer-support practices.

## 16. Reference sources to verify

The earlier research discussed these primary sources. Use the official, current pages and terms for exact versions; links below are starting points for verification, not legal sign-off.

- **U.S. Copyright Office:** https://www.copyright.gov/ — copyright guidance and statutory resources.
- **U.S. Copyright Law, Title 17:** https://www.copyright.gov/title17/ — including software-related provisions and anti-circumvention law.
- **RetroBat official site:** https://www.retrobat.org/ — project and download information.
- **RetroBat GitHub:** https://github.com/RetroBat-Official/retrobat — repository documentation and license information.
- **Batocera official site:** https://batocera.org/ — project documentation and terms.
- **Batocera GitHub:** https://github.com/batocera-linux/batocera.linux — source and license information.
- **Libretro:** https://www.libretro.com/ — project documentation; inspect individual cores separately.
- **ES-DE:** https://es-de.org/ — licensing and platform information.
- **IGDB API:** https://api-docs.igdb.com/ — API and commercial integration information.
- **USPTO trademark search:** https://www.uspto.gov/trademarks/search — name clearance starting point.

**Note on source verification:** The underlying assessment was discussed on November 4, 2026. This document does not certify that every linked term was rechecked at the time of packaging. Validate all rights and restrictions directly before use.

---

## 17. Bottom line

**Proceed with RetroDrive as an independent management suite**, not as a seller or unauthorized redistributor of games, BIOS files, or third-party emulation distributions. Preserve the project's hybrid library, drive manager, deployment engine, controller/configuration features, artwork manager, backups, and diagnostics, while building compliance checks into the architecture and release process.

The product concept is viable enough to continue engineering. Commercial release must remain conditional on reviewing exact third-party licenses and obtaining any required permissions.

---

## 18. Engineering status against this assessment (added 2026-11-04; keep updated)

This is an engineering status record, not a legal conclusion.

| Item | Status | Where |
|---|---|---|
| No ROMs, BIOS, firmware or game artwork shipped | ✅ True today. The repository and installer contain none; test fixtures are generated empty or zero-filled files, created at test time and deleted. | docs/10, `.gitignore` |
| No download links or integrations for unauthorized games | ✅ None exist. Rule added to the AI coding brief (docs/14). | docs/14 |
| User-controlled import and indexing | ✅ The user picks every folder; scans are read-only. | docs/07 |
| RetroBat: manage a user-installed copy only | ✅ The adapter only reads an existing folder the user picks; nothing is bundled, downloaded or modified. | docs/06, ADR-018 |
| RetroBat download/install (RDS-012) | ⛔ **Blocked on LEGAL-002/003** (written permission). | docs/11 |
| Batocera images and installs (RDS-013/014) | ⛔ **Blocked on LEGAL-004** (plus the existing safety review). | docs/11 |
| Metadata and artwork providers | ⛔ **Blocked on LEGAL-007.** | docs/11 |
| Independence statement | 🟡 The proposed disclaimer is shown in the app (Settings) and in the README, marked as pending counsel review. | `src/App.tsx`, README |
| LEGAL-001 dependency inventory | 🟡 Software dependencies are inventoried (`docs/17-THIRD-PARTY-COMPONENTS.md`). Emulators and cores aren't shipped, so none are listed. | docs/17 |
| LEGAL-014 license gate in CI | 🟡 CI fails on licenses outside the allowlist (`scripts/check-licenses.py`). Human sign-off for releases is still to be defined. | `.github/workflows/ci.yml` |
| Destructive disk operations | ✅ None implemented; the existing gates in docs/06 and docs/15 apply (LEGAL-010). | docs/06 |
| Privacy | ✅ Local-only; no telemetry and no network calls in the app today. | docs/10 |
