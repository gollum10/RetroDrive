# RetroDrive Privacy Policy

> **DRAFT for counsel review (LEGAL-009). Not legal advice and not yet in force.**
> Items in `[brackets]` are for the owner to fill in.

**Version:** draft 1 · **Date:** [EFFECTIVE DATE]

## The short version

- **RetroDrive doesn't collect your data.** It has no account, no sign-in, no ads, no
  analytics and no telemetry. It doesn't send information about you, your computer or
  your games to us or to anyone else.
- Everything RetroDrive knows (your game library, your drives, your settings) is stored
  **on your own computer**, and on the drives and systems you choose to use with it.
- You can delete all of it yourself at any time (see "Deleting your data").

## What RetroDrive stores, and where

RetroDrive needs to remember some things to work. It stores them locally:

| What | Why | Where |
|---|---|---|
| **Your game library catalog:** the folders you add, file names and paths, sizes, checksums, game titles and details (including from a catalog file you import), your collections and favorites | To show and organize your games and work out what to copy | `%APPDATA%\com.retrodrive.desktop\retrodrive.sqlite`, plus automatic copies made before an upgrade (`retrodrive.sqlite.v…-backup-…`) in the same folder |
| **Your drives and systems:** the drives, folders and network shares you add, their names, and identifiers Windows reports for them (such as the volume serial number and size), so RetroDrive recognizes the same drive again | To make sure it copies to the right drive | the same catalog file |
| **Your choices:** settings per system, designs, choices you made in Plan and Sync, controllers and light guns you confirmed (by their USB ID) | So you don't have to choose again | the same catalog file |
| **App preferences:** for example your backup folder and remembered tick boxes | Convenience | Windows' WebView2 storage for RetroDrive in `%LOCALAPPDATA%\com.retrodrive.desktop\` |
| **Backups you make:** copies of a system's saves, settings and game lists | So you can restore them | the backup folder you chose (by default `Documents\RetroDrive Backups`) |

### On your drives and systems

When you copy games or change settings, RetroDrive also writes a small folder named
`.retrodrive` on that drive or system. It holds records of what RetroDrive copied
(including file paths), copies of files it moved aside instead of replacing them, and
records that make undo possible. These stay on your drive; RetroDrive never sends
them anywhere.

## What RetroDrive does not do

- It doesn't collect or send usage statistics, crash reports or diagnostics.
- It doesn't connect to the internet by itself. It contains no code that sends data
  over the internet, and it doesn't check for updates automatically.
- It doesn't read files outside the folders, drives and systems you add to it, apart
  from the standard places those systems keep their own files (for example ES-DE's data
  folder or RetroArch's folder), what Windows needs to run it, and checking whether
  RetroBat or ES-DE is running so it doesn't change their files while they're open.
- It doesn't sell or share any information, because it doesn't have any of yours.

## Network connections you start

- **Your own network shares.** If you add a system by its network share (for example
  `\\BATOCERA\share` or `\\RETROPIE\roms`), RetroDrive reads and writes files there over
  your local network, like File Explorer would. That traffic goes to your own device.
- **Official download pages.** In the install helper, RetroDrive can open an official
  download page (for example batocera.org or raspberrypi.com) **in your web browser**
  when you click it. From then on, that website and its own privacy policy apply.

## Other software involved

- **Microsoft Edge WebView2,** part of Windows, draws RetroDrive's windows. Microsoft
  may collect diagnostic data under your Windows privacy settings and Microsoft's
  privacy statement; RetroDrive doesn't control or receive it.
- **Emulation systems and tools you use with RetroDrive** (such as RetroBat, ES-DE or
  Batocera) have their own privacy practices.

## Deleting your data

You're in control, and nothing is kept anywhere else:

1. **The catalog and settings:** close RetroDrive and delete the folder
   `%APPDATA%\com.retrodrive.desktop` (and `%LOCALAPPDATA%\com.retrodrive.desktop`
   for app preferences). Uninstalling RetroDrive may leave these folders, so delete them
   if you want everything gone. This never touches your game files.
2. **Backups:** delete the backup folder (Settings → Backups → Open folder).
3. **Records on your drives:** delete the `.retrodrive` folder on each drive or system.
   You'll lose the ability to undo RetroDrive's changes there.

[Planned: buttons in Settings to export or delete the catalog (docs/10).]

## Children

RetroDrive doesn't collect personal information from anyone, including children.

## Security

Your catalog and backups are ordinary files on your computer, protected by your Windows
account like your other files. The catalog contains file paths and the names of your
games, so treat it, and your backups, like other personal files.

## Changes to this policy

If a future version of RetroDrive changes what it stores or starts connecting to any
service, this policy will be updated before that version is released, and anything that
sends data would be off until you turn it on.

## Contact

[CONTACT EMAIL OR ADDRESS]

---

## Notes for the owner and counsel (not part of the policy)

- Facts checked in the code on 2026-11-09: no HTTP client or updater in the app's
  dependencies; the window's Content Security Policy allows no outside connections
  (ADR-089); the only outbound action is opening one of four fixed download pages in the
  browser (`install_helper::open_download_page`); data folders from the Tauri identifier
  `com.retrodrive.desktop`.
- If a feature that sends data is ever added (for example optional AI troubleshooting,
  FR-12, or update checks), update this policy first; docs/09 already requires such
  features to be opt-in and to redact paths, names and logs.
- Counsel to confirm which laws require which wording (for example GDPR or CCPA). With
  no data collected, most obligations may not apply, but the disclosures should still
  be accurate.
- Where it's shown: suggested in Settings → About and legal and next to the installer,
  once approved.
