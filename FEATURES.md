# RetroDrive features

RetroDrive is a free Windows app for people who keep their own retro game collection. It
catalogs your games, copies them safely to the systems you play on (RetroBat, Batocera,
RetroPie, ES-DE, Recalbox, ROCKNIX, Knulli, Lakka), keeps them in sync, backs up your saves,
sets up controllers and light guns, and lets you design your own themes.

RetroDrive never ships, sells or downloads games, BIOS files, firmware or game art. You
bring your own. It never overwrites or deletes your files.

**Get it:** [download the latest version](https://github.com/gollum10/RetroDrive/releases) (0.1.0 beta, Windows 10 and 11).

**Status:** ✅ Ready · 🟡 Works, with limits noted · 📋 Planned · ❌ Not planned

What changed and when: [CHANGELOG.md](CHANGELOG.md). How it's built: [docs/](docs/00-START-HERE.md).

---

## Your game library

| Feature | Status | Notes |
|---|---|---|
| Add your games | ✅ | Point RetroDrive at a folder or drive and it catalogs every game. Your files are only read, never changed. Fast: tens of thousands of files in seconds. |
| Browse by system | ✅ | A card per system with its game count and size; click one to see its games. |
| Grid or list view | ✅ | Cover tiles or a detailed list, with an A–Z bar. Your choice is remembered. |
| Search, filter and sort | ✅ | Search titles anywhere in the name. Filter by system, drive, status, genre, years, players, light gun or vertical games. Stays fast on 100,000+ games. |
| 49+ systems recognized | ✅ | From NES and Genesis to Amiga, MSX, Neo Geo, PSP and PlayStation 2, including hack folders. |
| Multi-disc games counted once | ✅ | Cue sheets and playlists are grouped into one game, with a "2 discs" badge. Missing discs are flagged. |
| Drives that come and go | ✅ | Games on unplugged drives show as **Offline**, not lost. A drive is recognized even under a new letter. |
| Your own box art | ✅ | Found automatically from your existing game lists or art folders, or pick one per game. Shown from where it is; never copied into RetroDrive. |
| Game details | ✅ | Descriptions, players and ratings from the game lists already in your folders. |
| Proper titles from your own catalog | ✅ | Import your spreadsheet (.xlsx or .csv) to show real titles, years, genres and publishers. Art columns are never read. |
| Possible duplicates | ✅ | Spot games you have twice (same name and size). **Clean up duplicates** leaves extras out after a preview. Nothing is deleted. |
| Leave games out | ✅ | Hide a game from the library and from copying; bring it back any time. |
| Favorites and collections | ✅ | Star favorites, make your own lists, or **smart collections** that fill themselves from filters (for example "Couch co-op"). |
| Light gun and vertical games | ✅ | Nearly 800 light gun games and about 3,500 vertical arcade games are marked, so you can filter and collect them. |
| Export and manage the catalog | ✅ | Export your game list to Excel, remove a folder from the catalog, clean up missing entries, or start over (a full copy is saved first, and you can put it back). |
| Hash-checked duplicates | 📋 | Comparing file contents, not just names and sizes. |
| Online scrapers (art and details from the internet) | ❌ | RetroDrive is free and doesn't fetch online art or details. |

## Your systems

| Feature | Status | Notes |
|---|---|---|
| RetroBat | ✅ | Find it, plan, copy, sync, settings, themes, art and details, controllers, backups. |
| Batocera and Knulli | ✅ | On a game drive or over the network. Copy, sync, settings, theme, art and details, backups. Network copies not yet tried on a real Batocera. |
| RetroPie | ✅ | Over the network or on a USB stick. Copy, sync, default emulators, art and details, backups. Not yet tried on a real RetroPie. |
| ES-DE | ✅ | Copy, sync, art and details, backups. |
| Recalbox | ✅ | Copy (games and BIOS), sync, settings, theme, art and details, backups. Not yet tried on a real Recalbox. |
| ROCKNIX | ✅ | Copy, sync, settings (over the network), art, details and favorites, backups. |
| Lakka | ✅ | Copy games and add them to Lakka's playlists with your box art; BIOS files; backups. |
| Find your systems | ✅ | RetroDrive looks on your drives and your network, and explains what to pick if you choose the wrong folder. |
| Install a system (helper) | ✅ | Walks you through installing any supported system with its official download and the usual card-writing tools. RetroDrive itself never downloads, ships or writes a system. |
| Survives drive-letter changes | ✅ | Saved systems are matched by the drive's identity, not its letter. |
| OneSauce / RetroFE (AtGames Legends Ultimate) | ❌ | Not supported until a legal review allows it. |

## Copying and keeping in sync

| Feature | Status | Notes |
|---|---|---|
| Plan before copying | ✅ | Choose systems or a collection and see exactly where every game goes, what's already there, and whether it fits, before anything is copied. |
| Safe, checked copies | ✅ | Every file is written under a temporary name, read back and checked (SHA-256), then given its real name. Never overwrites or deletes. |
| Cancel and crash safe | ✅ | Cancel any time. After a crash or unplugged drive, unfinished files are tidied up next time. |
| Choices out of the way | ✅ | Games needing a decision are set aside; everything else goes ahead. Review them in one place, with "for all" choices that are remembered. |
| Keep, replace or keep both | ✅ | When a different file is already there: keep it (default), keep both, or replace it (the original is backed up and can be put back). |
| Same game under another name | ✅ | A game already on the drive under a different name isn't copied twice. |
| Arcade: MAME or FinalBurn Neo | ✅ | You choose per system; arcade sets keep their exact names. |
| BIOS files | ✅ | Copies the BIOS files you own into each system's BIOS folder. Files already there are never replaced. |
| Sync | ✅ | Keeps what you copied up to date (new games in those systems and collections), and asks before restoring anything. Never copies your whole library by default. |
| Art and details on your systems | ✅ | Adds your box art, logos, titles, years and genres to each system's game lists, keeping what the system already has. Favorites carry over. |
| Copy record | ✅ | Every copy is recorded, with verified checksums. |
| Faster many-small-file copies | ✅ | Several games copy at once; large files still go one at a time. |
| File types a system can't show | 🟡 | RetroBat warns about games it won't list (for example .7z NES). Other systems aren't checked yet. |

## Backups and health

| Feature | Status | Notes |
|---|---|---|
| Back up saves, settings and game lists | ✅ | For every supported system, to a folder on your PC. Each file is checked; nothing on the system changes. Games and BIOS files aren't included. |
| Restore with a preview | ✅ | See what a restore would do first, choose which groups, and undo it later. A damaged backup restores nothing. |
| Backups as .zip | ✅ | Pack a backup into one checked .zip to keep on a USB stick, and bring it back later. |
| Health check | ✅ | Space, games, folders, BIOS, backups and leftovers, each with a button for the fix. Changes nothing by itself. |
| BIOS checker | ✅ | Which systems need BIOS files and whether you have them. RetroDrive never provides BIOS files. |

## Controllers and light guns

| Feature | Status | Notes |
|---|---|---|
| Recognize your controller | ✅ | Plug it in, confirm the style (Xbox, PlayStation, arcade stick…), and look up any of 1,400+ controllers. |
| Set it up on your systems | 🟡 | One click where RetroDrive can (RetroBat, RetroPie, ES-DE), clear steps where the system does it in its own menu. Xbox-style pads already work everywhere. |
| Light guns | ✅ | Recognizes Sinden, GUN4IR, AimTrak, Blamcon, Retro Shooter and more; shows setup per system, how far to sit, and helps copy the Sinden software you downloaded. |
| Light gun games collection | ✅ | One click makes a collection that fills itself, ready to copy. |
| Advanced control mapping | 📋 | Later. |

## Customize your systems

| Feature | Status | Notes |
|---|---|---|
| Designs | ✅ | Plan a system's look and behavior as a design (theme, boot video, music, behavior, emulators) without touching it, then apply it in one reviewed step and undo it the same way. Reusable on other drives. |
| Themes and remixes | ✅ | Switch themes, or remix one (colors, background, fonts) into a new folder. The original is never changed; credits stay with it. |
| Boot video and music | ✅ | Pick or add your own boot video and menu music, music per system, and which songs play. |
| Behavior | ✅ | Kiosk or kid mode, fullscreen, hide Quit, frame rate, language and more. |
| Emulator settings | ✅ | Per system, and for single games: emulator and core, shader, bezel, aspect ratio, integer scaling, rewind and more (RetroBat, Batocera, Knulli, ROCKNIX, Recalbox). |
| Game groups | ✅ | Show light gun and vertical games as their own groups on your systems. |
| Every change can be undone | ✅ | Reviewed before applying, backed up, and listed under **Changes made**. |

## Theme Studio (design your own themes)

| Feature | Status | Notes |
|---|---|---|
| Visual editor | ✅ | Every screen (Startup, System Select, Game List, Game Detail and more) on a canvas: drag, resize, align, snap, layer order, lock and hide, undo and redo. |
| Tool levels | ✅ | **Simple**, **Standard** or **Pro** tools for your skill level; the theme stays the same in each. |
| Ready layout | ✅ | Start with the game's title, year and description already placed. |
| Your art everywhere | ✅ | PNG, JPG or WebP pictures on the screen, as a full-screen background, or as a frame over another layer, such as an arcade cabinet. |
| Your own looping clips | ✅ | Add MP4 clips anywhere, including as a moving background. |
| Game video, game list and game details | ✅ | Place the selected game's video (with a frame over it), the game list and its colors, and text that shows each game's title, description, year or players. |
| System Select carousel | ✅ | Style the band of systems and show the number of games. |
| Save for RetroBat | ✅ | Saves your design as a RetroBat theme in a new folder, drawn exactly as you see it, with credits for every picture. **Put on RetroBat** switches to it in one click (undoable). Checked on RetroBat itself. |
| Other systems | 📋 | Saving themes for Batocera, ES-DE and others. |
| Animation | 📋 | Moving and fading layers over time. |

## AI helpers (optional)

Off until you connect one. Your games, folders and settings are never sent.

| Feature | Status | Notes |
|---|---|---|
| Connect an AI | ✅ | Your own Claude account (Claude Opus 5.5 by default, cheaper models selectable), another service with your key (OpenAI, Google Gemini, OpenRouter, Mistral, Groq, DeepSeek, xAI, Together AI, Fireworks AI, or any address in Pro), or a free private AI on your PC or network (Ollama, LM Studio). Keys are kept in Windows Credential Manager, never in a file. |
| Ask AI for design ideas | ✅ | In Theme Studio, ask for "an arcade feel" or "easier-to-read text". The changes appear on the screen at once; **Keep** or **Discard**. Only the screen's layout and your words are sent; every change is checked first. |
| Make a picture with AI | ✅ | In **Add picture**, describe one and make it with OpenAI or a picture program on your PC (Automatic1111, Forge, ComfyUI), then use it as a background or anywhere. Credited as AI-made. Only your description and the size are sent. |
| Cost protection | ✅ | Up to 50 requests a day; each answer shows roughly what it cost (for Claude) and how many requests are left. |

## Looks and comfort

| Feature | Status | Notes |
|---|---|---|
| App looks | ✅ | **Classic**, **Galactic** (black vector display, red and amber), **Bridge** (starship console), **Cartoon** (light and sunny), **Versus** (arcade fighter) and **80s** (synthwave in RetroDrive's logo colors). Original designs. |
| Setup wizards | ✅ | **Set up everything** takes you from your games to a ready system step by step; each task also has its own short wizard. All optional. |
| Progress where you clicked | ✅ | Progress, results and choices appear next to the button you pressed. |

## Safety and privacy

| Feature | Status | Notes |
|---|---|---|
| Never overwrites or deletes your files | ✅ | Replacements are backed up first and can be undone. |
| No account, no telemetry | ✅ | RetroDrive has no account and sends nothing about you. The only internet requests are the AI helpers you connect yourself. |
| Locked-down app | ✅ | The app's pages can only load RetroDrive's own code and talk only to RetroDrive itself. |
| Open source | ✅ | Apache License 2.0. Third-party notices and data credits ship with the app (**Settings → About and legal**). |
| Download and check | ✅ | Installers are built by GitHub from the public code and published on the Releases page with SHA-256 checksums. Not code-signed yet, so Windows SmartScreen may warn. |
| Automatic updates | 📋 | For now, download new versions from the Releases page. |
| Privacy policy | 🟡 | [docs/legal/PRIVACY.md](docs/legal/PRIVACY.md), including exactly what the AI helpers send. Not yet reviewed by a lawyer. |
| Disk formatting, imaging and cloning | ❌ | Not until a dedicated safety review. |
