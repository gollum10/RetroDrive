# 13 | Product Decisions and Open Questions

## Accepted decisions (from product discussions)
| ID | Decision | Rationale |
|---|---|---|
| ADR-001 | Name: RetroDrive Studio, under an earlier publisher name | Product identity for continued development |
| ADR-002 | Full retro gaming **management suite**, not merely drive creator | Persistent use beyond initial install |
| ADR-003 | **Hybrid** master library | One view, no forced relocation of existing collections |
| ADR-004 | Windows-first desktop | Local drive operations and initial audience |
| ADR-005 | Tauri/React/TypeScript/Rust/SQLite recommended technical baseline | Desktop UI plus native filesystem services |
| ADR-006 | Batocera and RetroBat first | Covers bootable OS and portable Windows scenarios |
| ADR-007 | Read-only, safety-first incremental development | Avoid unexpected user data loss |
| ADR-008 | Game identity, file location and deployment separate | Enables cross-drive library and offline indexing |
| ADR-009 | Use GitHub and Visual Studio Code | User already has GitHub; source-centric workflow |

## Recommended, awaiting implementation validation
- Separate privileged helper for imaging, with hardened permissions.
- No sign-in requirement or default network telemetry.
- RetroBat ordinary-folder deployment before raw disk imaging.
- Metadata provider integration with licensing review.

## Open questions to resolve in development
1. What minimum supported Windows build and hardware architectures?
2. Should network share indexing ship in M1 or later?
3. How should the user manage title/region/revision resolution when no external metadata exists?
4. Which metadata providers have acceptable commercial licensing?
5. Which Batocera device/image families are supported by the first installer?
6. What portable RetroBat versions/installer formats are supported?
7. Which filesystem types will future deployment target directly?
8. What should be offered free versus in a Pro tier after product validation?
9. What license will apply to RetroDrive Studio's own code?
10. Should the app provide optional encrypted backups and how should keys be managed?

## Change control
Record each major design change with date, alternatives, security/data migration impact and affected docs. Mark recommendations as proposals until confirmed; don't silently turn roadmap concepts into shipped features.
