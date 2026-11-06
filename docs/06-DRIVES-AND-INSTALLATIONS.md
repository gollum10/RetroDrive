# 06 | Drive Manager and Platform Installation Design

## Storage concepts
A physical disk may contain multiple partitions, volumes and mount points. A mounted drive letter is not necessarily the underlying disk. The app must model these separately before any format, imaging or clone feature is implemented.

## Read-only discovery (phase 1)
Enumerate mounted storage; display OS-reported capacity/free space, filesystem type, mount point, and disk identity when available. Clearly flag system volumes and inaccessible/unknown devices. Do not probe unknown raw devices with writes.

## Installation types
- **Batocera:** bootable operating system with platform/hardware-specific image and userdata layout. On Windows, direct access to Linux data partitions may not be supported; consider the Batocera network share after first boot, or an explicitly designed trusted access mechanism. Do not assume raw FAT/NTFS game paths are universally writable.
  - **Implemented read-only (RDS-008, 2026-11-06, `src-tauri/src/adapters/batocera.rs`; facts checked against the Batocera wiki and source):**
    - **Where games are:** the **BATOCERA** partition is the system (`batocera-boot.conf`, `boot\batocera`) and the only one Windows sees by default; games never go there. Games live on **SHARE** ("userdata"), ext4 by default, which Windows can't read. From Windows, the game folder is either the network share **`\\BATOCERA\share`** (while Batocera runs) or, when an external drive is used for storage, the **`batocera` folder at that drive's root** (exFAT/NTFS are readable).
    - **Detection:** "Detected" needs `roms\` plus `system\batocera.conf` (Batocera creates both on every boot if missing). A folder with `RetroBat.exe` is never claimed. Near misses are "Possible" (never saved) with a hint: the boot partition (choose the game folder instead), a drive root with a `batocera` folder (choose that folder), or userdata folders without `batocera.conf` (start Batocera once).
    - **Layout checks:** `roms`, `bios`, `system\batocera.conf`, access (network share or local drive), copying (not yet) and version.
    - **Version:** unknown. It isn't on SHARE and is inside the system image on the boot partition.
    - **Folder mapping:** the same as RetroBat (every folder RetroDrive uses exists in Batocera's `es_systems.yml`), except **CPS1/2/3**, which Batocera has no folders for: they follow the installation's arcade choice (`roms\mame` or `roms\fbneo`).
    - **Preview only:** plans show what would go where and the space needed, but **copying is off** (`copy_games: false`); the copier refuses before writing anything. Copying to Batocera needs its own safety review (docs/18 §9), mainly because network shares behave differently from local drives.
    - **Sources:** https://wiki.batocera.org/batocera.linux_architecture, https://wiki.batocera.org/add_games_bios, https://wiki.batocera.org/store_games_on_a_second_usb_sata_drive, and in https://github.com/batocera-linux/batocera.linux: `board/batocera/fsoverlay/etc/init.d/S11share` and `S12populateshare`, `board/batocera/scripts/post-image-script.sh`, `package/batocera/emulationstation/batocera-es-system/es_systems.yml` (checked 2026-11-06).
- **RetroBat:** portable Windows application/frontend with its own directory layout. First support placement to a normal user-selected writable folder; preserve app layout and upstream distribution rules.
  - **Implemented read-only (RDS-007, facts verified 2026-11-04 against the RetroBat wiki):**
    - **Detection:** "Detected" needs `RetroBat.exe` plus at least one of `emulationstation\`, `retrobat.ini` or `roms\`. Without the exe it can only be "Possible", which is never saved. A fresh install has no `retrobat.ini` until RetroBat first runs (observed on 8.3.1.1).
    - **Layout checks:** `roms`, `bios`, `emulators` and `emulationstation` folders, plus the install path (the wiki advises against spaces, accents and drive roots).
    - **Version:** read from `system\version.info` (fallback `emulationstation\version.info`), e.g. `8.3.1.1-stable-win64`. Not documented by RetroBat; observed on a real 8.3.1.1 install on 2026-11-05 (`RetroBat.exe` still reports 8.3.0.0, so it isn't used). Only a short version-like string is accepted.
    - **Folder mapping:** NES→`nes` (`.fds`→`fds`), SNES→`snes`, Genesis→`megadrive`, Master System→`mastersystem`, Game Gear→`gamegear`, PlayStation→`psx`, PS2→`ps2`, Game Boy→`gb`, GBC→`gbc`, GBA→`gba`, N64→`n64`.
    - **More systems (2026-11-05, checked against the RetroBat supported-systems page):** Famicom Disk System→`fds`, Amstrad CPC→`amstradcpc`, GX4000→`gx4000`, Atari 800→`atari800`, Atari ST→`atarist`, Atari 2600/5200/7800→`atari2600`/`atari5200`/`atari7800`, Lynx→`lynx`, Jaguar→`jaguar`, C64→`c64`, Amiga→`amiga500`, Amiga 1200→`amiga1200`, CD32→`amigacd32`, X68000→`x68000`, MSX→`msx1`, MSX2→`msx2`, ZX81→`zx81`, ZX Spectrum→`zxspectrum`, PC Engine→`pcengine`, PC Engine CD→`pcenginecd`, SuperGrafx→`supergrafx`, DS→`nds`, Neo Geo→`neogeo`, Neo Geo CD→`neogeocd`, NGP/NGPC→`ngp`/`ngpc`, Intellivision→`intellivision`, SG-1000→`sg1000`, Odyssey 2→`odyssey2`, WonderSwan/Color→`wswan`/`wswanc`, ColecoVision→`colecovision`, Vectrex→`vectrex`, Game & Watch→`gameandwatch`, 32X→`sega32x`, Sega CD→`megacd`, Pokemon Mini→`pokemini`, Virtual Boy→`virtualboy`, Supervision→`supervision`, CPS1/2/3→`cps1`/`cps2`/`cps3`, Dreamcast→`dreamcast`, Naomi→`naomi`, Atomiswave→`atomiswave`, Saturn→`saturn`, 3DO→`3do`. **BIOS files are never placed.**
    - **Arcade:** never guessed. Each installation has a setting, *Arcade games go to*: not chosen (default; arcade games aren't placed), **MAME** (`roms\mame`) or **FinalBurn Neo** (`roms\fbneo`). Only `.zip`/`.7z` archives are placed, with their names unchanged (they are romset names). CHD disks aren't placed yet: they must sit in a folder named after their game's zip. Per the RetroBat Arcade Guide, romset versions must match the emulator, non-merged sets are recommended and merged sets aren't supported.
    - **Multi-file games** go to `roms\<system>\<Game title>\…`, keeping file paths relative to the playlist or cue sheet.
    - **Sources:** https://wiki.retrobat.org/get-started/retrobat-folder-structure, https://wiki.retrobat.org/get-started/installation-and-first-startup and https://wiki.retrobat.org/systems-and-emulators/supported-game-systems and https://wiki.retrobat.org/systems-and-emulators/arcade-guide (checked 2026-11-05).
- **ES-DE/EmulationStation:** frontend, not interchangeable with a bootable OS image.
- **Recalbox/Lakka/RetroPie and others:** later adapters; platform-specific images and requirements.

## Installation wizard (later, gated release)
1. Choose target device and scenario (bootable OS, portable Windows, cabinet, Pi/handheld when supported).
2. Identify exact hardware architecture and compatibility.
3. Select appropriate upstream distribution/version.
4. Download from verified official source; verify checksum and/or signature against authenticated upstream publication.
5. Select physical target; show model, serial when safe, capacity, partitions and wipe warning.
6. Protect boot/system and active-data disks; refuse ambiguous or unsafe targets.
7. Require explicit disk-specific confirmation for destructive operations; provide backup guidance.
8. Apply platform-specific imaging or install method; verify resulting image/layout.
9. Produce first-boot steps and validation summary.

## Imaging safety invariants
- Cannot write to running OS disk or selected source disk.
- Refuse unexpected capacity, identity or mount changes between selection and execution.
- No silent partition deletion; no default target selected for destructive action.
- Validate download trust and full image before use.
- No arbitrary shell invoked via untrusted image metadata.
- Interrupted imaging yields 'potentially unusable destination' with recovery instructions, not 'success'.
- Separate elevated helper restricted to allowlisted imaging operations; build/test only after an explicit architectural review.

## Preliminary platform research URLs (verify before implementation)
- https://batocera.org/  and https://wiki.batocera.org/
- https://www.retrobat.org/
- https://www.es-de.org/
- https://www.recalbox.com/
- https://www.lakka.tv/
These are reference starting points only. Licensing, current download endpoints, checksums and supported devices require fresh verification during implementation.
