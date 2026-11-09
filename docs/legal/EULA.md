# RetroDrive End User License Agreement

> **DRAFT for counsel review (LEGAL-009). Not legal advice and not yet in force.**
> Items in `[brackets]` are for the owner to fill in. The notes at the end list the
> decisions this draft depends on.

**Version:** draft 1 · **Date:** [EFFECTIVE DATE]

This agreement is between you and [LICENSOR LEGAL NAME] ("we", "us"), the maker of
RetroDrive. By installing or using RetroDrive you agree to it. If you don't agree,
don't install or use RetroDrive.

## 1. What RetroDrive is

RetroDrive is a free Windows app that helps you organize game files you already
have and copy them, with their settings, artwork and saves, to emulation systems you
installed yourself (such as RetroBat, ES-DE, Batocera, Recalbox, RetroPie, ROCKNIX
and Lakka). RetroDrive does not include, sell or download games, BIOS or firmware, or
those emulation systems.

## 2. Your license

We give you a free, personal, non-exclusive, non-transferable license to install and
use RetroDrive on computers you own or control, for your own personal, non-commercial
use.

RetroDrive is free. We don't charge for it and this agreement doesn't create any
obligation to pay.

## 3. What you may not do

You may not:

1. Sell, rent or lease RetroDrive, or charge others for access to it.
2. Use RetroDrive to prepare drives, cards, consoles or other machines with games,
   BIOS, firmware or third-party software **for other people or for sale**.
3. Use RetroDrive to copy, share or distribute files you don't have the right to use.
4. Remove or change the copyright, license or trademark notices in RetroDrive, including
   the third-party notices.
5. Present RetroDrive, or a modified copy of it, as made, endorsed or sponsored by
   the makers of any emulation system, console or game.

Open-source components inside RetroDrive are licensed to you under their own licenses
(see section 6). Nothing in this section limits rights those licenses give you, or
rights you have under the law where you live.

## 4. Your files are your responsibility

RetroDrive works only with files you choose. You are responsible for making sure you
have the right to use and copy every game, BIOS, firmware file, image and other file
you use with it. RetroDrive doesn't check whether you have those rights and doesn't
give you any.

## 5. Your drives and your data

RetroDrive is designed to be careful with your files: it shows you what it will do
before it does it, checks every file it copies, doesn't overwrite or delete files on
your drives, moves files aside instead of replacing them, and lets you undo its
changes. Even so:

1. Computers, drives, cards and networks can fail, and software can have mistakes.
   **Keep your own backups** of anything you can't afford to lose, including saves.
2. RetroDrive changes the settings, game lists and other files of the emulation systems
   you point it at. Those systems may behave differently afterwards, and their makers
   may not support systems changed by other software.
3. RetroDrive's own backups and undo are a convenience, not a guarantee.

## 6. Third-party software and services

1. **Open-source components.** RetroDrive is built with open-source components that
   are licensed to you under their own licenses. They're listed, with their license
   texts, in `THIRD_PARTY_NOTICES.txt` (installed with RetroDrive; also under
   Settings → About and legal → Third-party notices). Data files RetroDrive includes
   are credited in `THIRD_PARTY_CONTROLLERS.txt` (Settings → About and legal → Data
   credits).
2. **Emulation systems and other software you install** (for example RetroBat, ES-DE,
   RetroArch, Batocera, Raspberry Pi Imager or balenaEtcher) are made by others and
   are governed by their own terms. We don't make, provide or support them.
3. **Websites.** RetroDrive can open official download pages in your web browser when
   you ask it to. Those websites and their terms and privacy policies are not ours.
4. **Microsoft WebView2.** RetroDrive shows its windows with Microsoft Edge WebView2,
   which is part of Windows and governed by Microsoft's terms.

## 7. Trademarks and independence

RetroDrive is an independent retro gaming management application. All third-party
product names, logos and trademarks belong to their respective owners. Mentioning a
system or product to describe compatibility does not mean it sponsors, endorses or is
affiliated with RetroDrive. "RetroDrive" and the RetroDrive logo are [OUR TRADEMARKS /
NAMES WE USE]; this agreement doesn't give you any right to use them.

## 8. Your privacy

RetroDrive keeps its data on your computer and doesn't send it to us. The
[RetroDrive Privacy Policy](PRIVACY.md) explains what it stores and where.

## 9. Updates

RetroDrive doesn't update itself automatically. If we offer a new version, you choose
whether to install it. A new version may come with an updated version of this
agreement.

## 10. No warranty

RetroDrive is provided **"as is" and "as available", without warranties of any kind**,
whether express, implied or statutory, including warranties of merchantability,
fitness for a particular purpose, title, non-infringement, and that it will be
error-free or uninterrupted, to the fullest extent the law allows. Some places don't
allow some of these exclusions, so some may not apply to you.

## 11. Limit of liability

To the fullest extent the law allows, we are not liable for any indirect, incidental,
special, consequential or punitive damages, or for any loss of data, saves, files,
profits or use, arising from your use of or inability to use RetroDrive, even if we
were told such loss was possible. Because RetroDrive is free, our total liability for
any claim about it is limited to [USD 0 / USD 10 / THE AMOUNT YOU PAID US FOR IT].
Some places don't allow these limits, so some may not apply to you.

## 12. Ending this agreement

You can end this agreement at any time by uninstalling RetroDrive. It ends
automatically if you break it. Sections 4, 5, 6, 7, 10, 11, 12 and 13 continue after it
ends.

## 13. General

1. **Law.** This agreement is governed by the laws of [STATE / COUNTRY], except its
   conflict-of-law rules. [COURTS / VENUE].
2. **Changes.** We may change this agreement for future versions of RetroDrive. The
   version that came with the copy you installed applies to that copy.
3. **Whole agreement.** This agreement and the open-source licenses in
   `THIRD_PARTY_NOTICES.txt` are the whole agreement about RetroDrive. If a part of it
   can't be enforced, the rest still applies.
4. **Contact.** [CONTACT EMAIL OR ADDRESS]

---

## Notes for the owner and counsel (not part of the agreement)

- **Depends on the project license decision** (docs/13, open question #9). If RetroDrive
  is released as open source (for example under MIT), its own license replaces most of
  sections 2, 3 and 12, and section 3's restrictions on selling and redistributing would
  conflict with it. Decide the license first, then keep or cut this document.
- **Personal-use restriction (section 3.2)** carries over the "personal use" position in
  docs/16 and the wording already in Settings → About and legal. Counsel to confirm it's
  wanted and enforceable as a license term rather than guidance.
- **Liability cap (section 11)** is a placeholder; counsel to choose.
- **Trademarks (section 7):** depends on the RetroDrive trademark check (LEGAL-008).
- **Where it's shown:** suggested in the installer (Tauri's NSIS `licenseFile`, so it's
  accepted before installing) and in Settings → About and legal, once approved.
- Facts this draft relies on were checked in the code on 2026-11-09: no network calls of
  its own (only opening fixed official download pages in the browser when asked, and the
  user's own network shares), no automatic updates, no telemetry, never deletes, copies
  are verified, changes are undoable.
