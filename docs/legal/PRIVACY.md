# RetroDrive Privacy Policy

> Checked against RetroDrive's code on 2026-11-10 (version 0.1.0). It hasn't been reviewed by a lawyer yet (LEGAL-009).

**Version:** 2 · 2026-11-10 (version 1, 2026-11-09, was written before the optional AI helpers existed)

## The short version

- **RetroDrive doesn't collect your data.** It has no account, no sign-in, no ads, no
  analytics and no telemetry. It doesn't send information about you, your computer or
  your games to us or to anyone else.
- Everything RetroDrive knows (your game library, your drives, your settings) is stored
  **on your own computer**, and on the drives and systems you choose to use with it.
- You can delete all of it yourself at any time (see "Deleting your data").
- **The optional AI helpers** in Theme Studio are the only feature that sends anything over
  the internet, and only after you connect one with your own account. They send only what
  you ask about, never your game library (see "Optional AI helpers").

## What RetroDrive stores, and where

RetroDrive needs to remember some things to work. It stores them locally:

| What | Why | Where |
|---|---|---|
| **Your game library catalog:** the folders you add, file names and paths, sizes, checksums, game titles and details (including from a catalog file you import), your collections and favorites | To show and organize your games and work out what to copy | `%APPDATA%\com.retrodrive.desktop\retrodrive.sqlite`, plus automatic copies made before an upgrade (`retrodrive.sqlite.v…-backup-…`) in the same folder |
| **Your drives and systems:** the drives, folders and network shares you add, their names, and identifiers Windows reports for them (such as the volume serial number and size), so RetroDrive recognizes the same drive again | To make sure it copies to the right drive | the same catalog file |
| **Your choices:** settings per system, designs, choices you made in Plan and Sync, controllers and light guns you confirmed (by their USB ID) | So you don't have to choose again | the same catalog file |
| **App preferences:** for example your backup folder and remembered tick boxes | Convenience | Windows' WebView2 storage for RetroDrive in `%LOCALAPPDATA%\com.retrodrive.desktop\` |
| **AI helper settings** (only if you use them): which AI services you connected, the models you chose, the address of a private AI or picture program, and how many AI requests you made today | To remember your choices and keep the daily limit | `%APPDATA%\com.retrodrive.desktop\ai-settings.json` |
| **AI keys** (only if you connect a service) | So you don't have to paste the key each time | **Windows Credential Manager** on your PC, under names starting `RetroDrive/ThemeStudio/`. Keys are never written to RetroDrive's own files. |
| **Backups you make:** copies of a system's saves, settings and game lists | So you can restore them | the backup folder you chose (by default `Documents\RetroDrive Backups`) |

### On your drives and systems

When you copy games or change settings, RetroDrive also writes a small folder named
`.retrodrive` on that drive or system. It holds records of what RetroDrive copied
(including file paths), copies of files it moved aside instead of replacing them, and
records that make undo possible. These stay on your drive; RetroDrive never sends
them anywhere.

## What RetroDrive does not do

- It doesn't collect or send usage statistics, crash reports or diagnostics.
- It doesn't connect to the internet by itself. The only feature that can is an AI
  helper you connect yourself (below). It doesn't check for updates automatically.
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

## Optional AI helpers

Theme Studio can suggest design changes (**Ask AI**) and make pictures (**Make one with
AI**). These are **off until you connect an AI** in Settings → AI helpers. Nothing is sent
before that. You can use:

- **An online AI service with your own account and key:** Anthropic (Claude), OpenAI,
  Google Gemini, OpenRouter, Mistral, Groq, DeepSeek, xAI, Together AI, Fireworks AI, or
  another service whose address you enter. RetroDrive sends your request straight from
  your PC to that company. It doesn't go through us; we have no servers.
- **A private AI or picture program** on your own PC or local network (for example
  Ollama, LM Studio, AUTOMATIC1111 or ComfyUI). RetroDrive only accepts addresses on this
  PC or your local network for these, so the request stays on your own devices.

**What is sent, only when you press the button:**

| Helper | Sent |
|---|---|
| Connecting a service | Your key, to that service only, to check that it works and list its models |
| Ask AI | What you typed, and a description of the one screen you're designing: its name and size, and for each layer its name, type, position, size, colours, font size and any text you typed on it. RetroDrive also adds its own fixed instructions for the AI. In the Standard and Pro skill levels, **Show exactly** lets you see the whole request before it's sent. |
| Make a picture | The description you typed and the picture's shape (for example wide or square) |

**Never sent:** your game library, game names and file paths, your drives, folders and
systems, your pictures, your settings, or anything about your PC. RetroDrive adds no
identifier of its own. As with any internet request, the company can see your internet
(IP) address.

**Your account, their terms.** Each AI company receives the request under your account
and its own terms and privacy policy, and bills you under its own prices. Read those
before you connect. What they keep, and for how long, is up to them, not RetroDrive.

**Limits and control.** RetroDrive allows at most 50 AI requests a day, to protect you
from surprise bills. Suggestions are only applied if you press **Keep**. Pictures made with
AI are credited as AI-made in your theme. **Disconnect** in Settings → AI helpers deletes
the key from Credential Manager.

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
4. **AI keys:** press **Disconnect** for each service in Settings → AI helpers, or delete
   the entries starting `RetroDrive/ThemeStudio/` in Windows Credential Manager. To remove
   anything an AI company kept, contact that company.

[Planned: buttons in Settings to export or delete the catalog (docs/10).]

## Children

RetroDrive doesn't collect personal information from anyone, including children. The
online AI services set their own age rules for accounts.

## Security

Your catalog and backups are ordinary files on your computer, protected by your Windows
account like your other files. AI keys are kept by Windows Credential Manager, and
requests to online AI services use encrypted HTTPS. The catalog contains file paths and the names of your
games, so treat it, and your backups, like other personal files.

## Changes to this policy

If a future version of RetroDrive changes what it stores or starts connecting to any
service, this policy will be updated before that version is released, and anything that
sends data would be off until you turn it on.

## Contact

RetroDrive is an open-source project. For questions about this policy, open an issue on
GitHub: https://github.com/gollum10/RetroDrive/issues. Please don't post personal information there; issues are public.

---

## Notes for the owner and counsel (not part of the policy)

- Facts checked in the code on 2026-11-10: no updater; the window's Content Security
  Policy still allows no outside connections (ADR-089), so only the Rust side can reach
  the network. The one HTTP client (`ureq`, HTTPS through Windows) is used only by the AI
  helpers (`src-tauri/src/ai.rs`, `ai_design.rs`, `ai_pictures.rs`; ADR-119 to ADR-122):
  online services from the fixed list in `ai::SERVICES` or a custom `https://` address,
  private AIs and picture programs only on local addresses (`ai::private_url`). What Ask
  AI sends is built by `describeScreen` in `src/theme-studio/ai-suggest.ts`. The other
  outbound actions open fixed download or key pages in the browser. Data folders come
  from the Tauri identifier `com.retrodrive.desktop`.
- Version 1 said the app contained no code that sends data over the internet; that
  stopped being true with the AI helpers (2026-11-10) and this version replaces it.
- If another feature that sends data is added (for example AI troubleshooting, FR-12, or
  update checks), update this policy first; docs/09 already requires such features to be
  opt-in and to redact paths, names and logs.
- Counsel to confirm which laws require which wording (for example GDPR or CCPA). With
  no data collected, most obligations may not apply, but the disclosures should still
  be accurate.
- Where it's shown: linked from the README, FEATURES.md and each release's notes.
  Suggested later: a link in Settings → About and legal.
- No name, email or postal address is published (owner decision 2026-11-09); contact is
  through GitHub Issues. Counsel to confirm that's enough where a privacy contact is
  required.
