# 06 | Drive Manager and Platform Installation Design

## Storage concepts
A physical disk may contain multiple partitions, volumes and mount points. A mounted drive letter is not necessarily the underlying disk. The app must model these separately before any format, imaging or clone feature is implemented.

## Read-only discovery (phase 1)
Enumerate mounted storage; display OS-reported capacity/free space, filesystem type, mount point, and disk identity when available. Clearly flag system volumes and inaccessible/unknown devices. Do not probe unknown raw devices with writes.

## Installation types
- **Batocera:** bootable operating system with platform/hardware-specific image and userdata layout. On Windows, direct access to Linux data partitions may not be supported; consider the Batocera network share after first boot, or an explicitly designed trusted access mechanism. Do not assume raw FAT/NTFS game paths are universally writable.
- **RetroBat:** portable Windows application/frontend with its own directory layout. First support placement to a normal user-selected writable folder; preserve app layout and upstream distribution rules.
  - **Implemented read-only (RDS-007, facts verified 2026-11-04 against the RetroBat wiki):**
    - **Detection:** "Detected" needs `RetroBat.exe` plus at least one of `emulationstation\`, `retrobat.ini` or `roms\`. Without the exe it can only be "Possible", which is never saved.
    - **Layout checks:** `roms`, `bios`, `emulators` and `emulationstation` folders, plus the install path (the wiki advises against spaces, accents and drive roots).
    - **Version:** shown as unknown, because no version file is documented.
    - **Folder mapping:** NES→`nes` (`.fds`→`fds`), SNES→`snes`, Genesis→`megadrive`, Master System→`mastersystem`, Game Gear→`gamegear`, PlayStation→`psx`, PS2→`ps2`, Game Boy→`gb`, GBC→`gbc`, GBA→`gba`, N64→`n64`.
    - **Not guessed:** Arcade, because `mame` and `fbneo` take different ROM sets.
    - **Multi-file games** go to `roms\<system>\<Game title>\…`, keeping file paths relative to the playlist or cue sheet.
    - **Sources:** https://wiki.retrobat.org/get-started/retrobat-folder-structure, https://wiki.retrobat.org/get-started/installation-and-first-startup and https://wiki.retrobat.org/systems-and-emulators/supported-game-systems.
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
