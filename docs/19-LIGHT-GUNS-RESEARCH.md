# Light guns and light bars: research and proposal (RDS-028 step 3)

**Status:** A, B and C built 2026-11-07 (ADR-066; owner "build A, B, and C, then do D"). D built the same day (ADR-067). Untested with a real gun (the owner doesn't have one). Owner request: "helping them config for complex setups like those with a Lightbar and Lightgun, so I would like to explore that."

## 1. What the systems already do

Both main platforms handle the hard parts themselves. RetroDrive should not repeat that work. Its job is to guide the user and prepare things.

| | RetroBat | Batocera |
|---|---|---|
| Gun detection | Automatic for Sinden, GUN4IR, Blamcon, Retro Shooter, AimTrak, AE Lightgun, XGunner, Wiimote with a DolphinBar (one DolphinBar per Wiimote), Wiimote4Guns (up to 4). A mouse acts as a gun when no gun is connected. | Automatic since v35, by **device name** (udev rules per gun). Each gun is calibrated once and the calibration is stored in the gun. |
| Sinden | Needs the Sinden software (`Lightgun.exe`) running. Put it in `RetroBat\system\tools\sinden`, or set *SINDEN SOFTWARE PATH* under GAME SETTINGS > GUNS > SINDEN. The white border is added automatically; some standalone emulators need the *sindenborder* ReShade shader. Calibrate once outside RetroBat and enable "Start automatically". | Built in. CONTROLLER & BLUETOOTH SETTINGS > SINDEN GUN SETTINGS: border size/mode/colour, camera, recoil. Turn off overscan or zoom, or the border is hidden. Recoil models need their own power supply. Firmware 1.09+ on v40+. |
| GUN4IR | Detected automatically. Keep the gun in mouse mode. | Full support from v36. Custom settings are burned into the gun with the Windows GUN4IR tool. |
| Light gun games | Automatic collection: MAIN MENU > GAME COLLECTION SETTINGS > AUTOMATIC GAME COLLECTIONS. Per game/system: ENABLE GUN, USE ONE GUN ONLY, RELOAD WITH BUTTON, INVERT TRIGGER/RELOAD. | A light gun genre, shown as a system if the user enables it. |
| Player order | Dedicated guns first, then Wiimotes, then mice; override with P1–P4 MOUSE/GUN INDEX. | Per gun. |

Sources: [RetroBat wiki: Lightguns](https://wiki.retrobat.org/controllers/supported-controllers/lightguns), [Batocera wiki: Light gun guide](https://wiki.batocera.org/emulators:lightgun_games), [Sinden wiki: Getting Started](https://sindenwiki.org/wiki/Getting_Started), [Sinden FAQ](https://www.sindenwiki.org/wiki/FAQ).

**Batocera's guide to how far to sit**, as a multiple of the screen size: GUN4IR 1.2–2×, Sinden 1.5×, Blamcon 1–2×, Wiimote with an IR bar 2.5×, Retro Shooter 2–3×, AimTrak 2–2.5×, AE Lightgun 3–3.5×. GunCon 2 needs a CRT.

**"Light bar"** covers two different things:
- The **IR LED bar** that IR guns aim at: GUN4IR uses 4 LEDs around the screen, Retro Shooter uses 2 or 4, and a Wiimote uses a sensor bar (a powered bar is recommended).
- The **white border** that Sinden guns look for, drawn on screen.

Both have to be set up physically, and both affect accuracy. RetroDrive can explain that setup step by step.

## 2. Facts RetroDrive can build on

- **Recognizing a gun on this PC:** guns appear to Windows as a mouse (plus keyboard), not as a gamepad, so the current controller detection skips them. Batocera recognizes them by name. RetroDrive can read each mouse's product name the same way (`HidD_GetProductString`) and match these words: *Sinden*, *GUN4IR*, *ATRAK*/*Aimtrak*, *Blamcon*, *Retro Shooter*, *Fusion*, *OpenFIRE*/*FIRECon*, *XGunner*, *OneHit*, *GunCon*, *AE lightgun*, *Mayflash Wiimote PC Adapter*. The names come from Batocera's per-gun udev rules (`batocera.linux/package/batocera/controllers/guns/*/99-*.rules`).
  - Generic Arduino names (SAMCO, some AE guns) can't be told apart, so for those the user picks the gun from the list, just like the controller style dropdown.
  - The GunCon 2 and 3 also have USB IDs (0b9a:016a and 0b9a:0800).
- **Finding light gun games in the library:** Batocera's EmulationStation ships `resources/gamesdb.xml` (MIT licence). It marks **791 gun games**: arcade 528, Wii 40, NES 29, PSX 25, Amstrad CPC 22, C64 20, PS3 19, PS2 14, Master System 13, Saturn 13, SNES 13, 3DO 11, and more.
  - Arcade entries use MAME short names, and console entries use simplified names (e.g. `gumshoe`, `area51`).
  - RetroDrive can match these against the file names in the library, the same way it already matches arcade sets.
  - The owner's `game_genres_supplement.csv` only tags about 32 games as light gun, so it's a supplement, not the main source.
- **No downloads:** the Sinden and GUN4IR software is the user's own download, and RetroDrive never fetches it. That's the same rule as for emulators (we don't ship emulators unless permitted).

## 3. Proposal (for the owner's approval)

The steps are in order of value and safety. The first three only read; only step D writes.

**A. Recognize a light gun on this PC (reads only).** The Controllers page gets a gun card with the light gun outline:
- "We found a **GUN4IR** light gun. Is that right?"
- Yes, or choose another gun from a list.
- RetroDrive remembers the answer, the same way as the controller style.

**B. Light gun guide, made for the user's own gun and systems (reads only).** One short checklist per gun and system, for example "GUN4IR on RetroBat":
1. Place the 4 IR LEDs at the corners of the screen.
2. Calibrate once with the GUN4IR tool on Windows.
3. Keep the gun in mouse mode.
4. In RetroBat, turn on the light gun collection: MAIN MENU > GAME COLLECTION SETTINGS > AUTOMATIC GAME COLLECTIONS.

It also includes a **"How far to sit" helper**: type your screen size (e.g. 55") and see the distance for your gun (from Batocera's table). For the Sinden it also covers the border, the "Start automatically" option, and turning off overscan.

**C. Light gun games (reads the library; copying uses Plan as usual).**
- A *Light gun games* filter in the Game Library, from `gamesdb.xml` (bundled and credited like the controller data).
- A ready-made **Light gun games** collection the user can pick in Plan. Plan still chooses and Sync still maintains, so nothing is added automatically.

**D. Sinden on RetroBat (writes; needs the owner's go-ahead and a safety review).**
- RetroDrive checks whether the Sinden software is in `RetroBat\system\tools\sinden`.
- If it isn't, the user points RetroDrive at the Sinden folder they downloaded, and RetroDrive copies it there. It copies new files only and replaces nothing, the same way as the BIOS step.
- Batocera's Sinden options stay in its own menu (as in step 2).

**Not proposed:**
- Writing emulator gun settings or calibration. RetroBat and Batocera generate these themselves, and calibration lives in the gun.
- Downloading gun software.
- RetroPie, ES-DE and Recalbox gun setup. Their support is per emulator and less consistent; researching them can come later. *(Done 2026-11-07, ADR-075: guidance for RetroPie and ES-DE, Recalbox's supported guns and its light gun group.)*

## 4. Open points to check before building

- On a real Windows PC, which product name does each gun report, and does the name keep the "P1/P2" player suffix?
- How exactly does EmulationStation match `gamesdb.xml` console names to ROM file names (`GunManager.cpp` / `gamesdb` loader)?
- Can RetroBat's automatic light gun collection be switched on through an `es_settings.cfg` key? (Find the exact key in its source before offering to switch it on for the user.)
