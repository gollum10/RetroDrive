// Install helper (RDS-027, ADR-103): put Batocera, RetroPie, Recalbox, ROCKNIX or Lakka
// on a card or USB stick from an image the user downloaded, or show where to get RetroBat
// and ES-DE (installed by the user on Windows) and then find them. RetroDrive checks the file and opens Raspberry Pi Imager
// or balenaEtcher; the user picks the drive there. RetroDrive never writes to it.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, ExternalLink, FolderOpen, Search, X } from 'lucide-react';
import { fmtSize } from './format';

type ImageInfo = { path: string; file_name: string; bytes: number; format: string; system: string|null; version: string|null; target: string|null; problem: string|null; notes: string[] };
type Writer = { id: 'rpi-imager'|'etcher'; name: string; takes_image: boolean };
type Checksum = { algorithm: string; actual: string; matches: boolean };

type ImageSystem = { kind: 'image'; name: string; hardware: string; checksum: string|null; tip?: string; devices?: string; afterBoot: string[] };
type AppSystem = { kind: 'app'; name: string; hardware: string; install: string[] };
// Image systems go on a card or USB stick; app systems (Windows) the user installs from
// the official site. RetroDrive never downloads, ships or installs any of them (ADR-087, ADR-103).
const SYSTEMS: Record<string, ImageSystem|AppSystem> = {
  batocera: { kind: 'image', name: 'Batocera', hardware: 'PCs (x86_64) and many boards, including Raspberry Pi 3, 4 and 5', checksum: 'SHA-256', afterBoot: ['Put the card or USB stick in the machine and start it from it (on a PC, choose it in the boot menu).', 'The first start takes a few minutes while Batocera sets itself up.', 'Connect it to your network: games can then be copied to \\\\BATOCERA\\share from this PC.', 'Back here, choose Add existing installation and pick \\\\BATOCERA\\share (or the batocera folder on its game drive).'] },
  retropie: { kind: 'image', name: 'RetroPie', hardware: 'Raspberry Pi (pick the image for your model)', checksum: 'MD5', afterBoot: ['Put the card in the Raspberry Pi and start it.', 'The first start expands the card and asks you to set up a controller.', 'Turn on its network share (RetroPie setup) so this PC can reach \\\\RETROPIE\\roms.', 'Back here, choose Add existing installation and pick \\\\RETROPIE\\roms.'] },
  recalbox: { kind: 'image', name: 'Recalbox', hardware: 'Raspberry Pi, PCs, some handhelds and ODROID boards (choose yours on the download page)', checksum: null, tip: 'On a Raspberry Pi you can skip steps 2 and 3: in Raspberry Pi Imager choose Choose OS → Emulation and game OS → Recalbox, and Imager downloads it for you.', afterBoot: ['Put the card or USB stick in the machine and start it from it.', 'The first start takes a few minutes while Recalbox sets itself up.', 'Connect it to your network: games can then be copied to \\\\RECALBOX\\share from this PC.', 'Back here, choose Find or add a system (or Add existing installation) and pick \\\\RECALBOX\\share.'] },
  rocknix: { kind: 'image', name: 'ROCKNIX', hardware: 'handhelds from Anbernic, Powkiddy, Retroid, AYN and others', checksum: null, devices: "Not sure which image? ROCKNIX's site lists each device and the image it needs (named after the device's chip, e.g. RK3566).", afterBoot: ['With the handheld switched off, put the card in, then switch it on (some devices need the card chosen as the start device).', 'Wait while ROCKNIX installs itself: it restarts into its game menu.', 'Connect it to Wi-Fi: games can then be copied to \\\\ROCKNIX\\games-roms from this PC.', 'Back here, choose Find or add a system (or Add existing installation) and pick \\\\ROCKNIX\\games-roms.'] },
  lakka: { kind: 'image', name: 'Lakka', hardware: 'PCs, Raspberry Pi and many other boards (choose yours on the download page)', checksum: null, afterBoot: ['Put the card or USB stick in the machine and start it from it.', 'Connect it to your network: games can then be copied to \\\\LAKKA\\ROMs from this PC.', 'Back here, choose Find or add a system (or Add existing installation) and pick \\\\LAKKA\\ROMs.'] },
  retrobat: { kind: 'app', name: 'RetroBat', hardware: 'Windows PCs', install: ['Download RetroBat from its official page.', 'Unpack or install it into a folder of your choice, for example on your game drive (G:\\RetroBat).', 'Start RetroBat once so it creates its folders, then close it.'] },
  esde: { kind: 'app', name: 'ES-DE', hardware: 'Windows PCs (also Linux, macOS and Android)', install: ['Download ES-DE for Windows from its official page: the installer, or the portable zip.', 'Install it, or unpack the portable zip into a folder of your choice.', 'Start ES-DE once: it asks where your games should go (its ROMs folder). Then close it.'] },
};
type SystemId = string;

export default function InstallHelper({ notify, onClose, onFind }: { notify: (text: string, error?: boolean) => void; onClose: () => void; /** Open Find or add a system. */ onFind: () => void }) {
  const [system, setSystem] = useState<SystemId>('batocera');
  const [image, setImage] = useState<ImageInfo|null>(null); const [checking, setChecking] = useState(false);
  const [expected, setExpected] = useState(''); const [sum, setSum] = useState<Checksum|null>(null); const [hashing, setHashing] = useState<{ done: number; total: number }|null>(null);
  const [writers, setWriters] = useState<Writer[]|null>(null); const [opened, setOpened] = useState('');
  const s = SYSTEMS[system];
  const img = s.kind === 'image' ? s : null;
  useEffect(() => { void invoke<Writer[]>('install_writers').then(setWriters).catch(() => setWriters([])); }, []);
  useEffect(() => { const un = listen<{ done: number; total: number }>('install-hash', e => setHashing(e.payload)); return () => { void un.then(f => f()); }; }, []);
  useEffect(() => { setImage(null); setSum(null); setExpected(''); setOpened(''); }, [system]);

  const page = (id: string) => void invoke('open_download_page', { id }).catch(e => notify(String(e), true));
  const pick = async () => {
    const chosen = await open({ multiple: false, directory: false, title: `Choose the ${s.name} image you downloaded`, filters: [{ name: 'Disk images', extensions: ['gz', 'xz', 'zip', 'img'] }] });
    if (typeof chosen !== 'string') return;
    setChecking(true); setSum(null); setOpened('');
    try { setImage(await invoke<ImageInfo>('install_image_info', { path: chosen, system })); } catch (e) { notify(String(e), true); } finally { setChecking(false); }
  };
  const verify = async () => { if (!image) return; setHashing({ done: 0, total: image.bytes });
    try { setSum(await invoke<Checksum>('install_checksum', { path: image.path, expected })); } catch (e) { notify(String(e), true); } finally { setHashing(null); } };
  const openWriter = async (w: Writer) => { if (!image) return;
    try { if (!w.takes_image) { try { await navigator.clipboard.writeText(image.path); } catch { /* clipboard unavailable */ } }
      setOpened(await invoke<string>('open_install_writer', { id: w.id, image: image.path })); } catch (e) { notify(String(e), true); } };
  const usable = !!image && !image.problem && !(sum && !sum.matches);

  return <section className="panel install-helper">
    <div className="section-heading"><div><h2>Install a system</h2><p>RetroDrive shows you the way; it never downloads, ships or installs a system itself. For a system on a card or USB stick you download the official image; RetroDrive checks it and opens Raspberry Pi Imager or balenaEtcher to write it. <b>RetroDrive never writes to the card itself</b>: you choose the drive in that program, which warns before erasing it.</p></div><button className="ghost" onClick={onClose}><X size={16}/> Close</button></div>

    <ol className="install-steps">
      <li><b>Choose the system</b>
        <div className="actions">{(Object.keys(SYSTEMS) as SystemId[]).map(id => <button key={id} className={system === id ? 'primary' : 'ghost'} onClick={() => setSystem(id)}>{SYSTEMS[id].name}</button>)}</div>
        <small className="plan-notes">{s.name} runs on {s.hardware}.</small></li>

      {img ? <>
      <li><b>Download the image</b>Get it from the official site; RetroDrive doesn't download or ship it.
        <div className="actions"><button className="ghost" onClick={() => page(system)}><ExternalLink size={15}/> Open {s.name}'s download page</button></div>
        {img.devices && <p className="plan-notes">{img.devices} <button className="link" onClick={() => page('rocknix-devices')}>Open ROCKNIX's device list</button></p>}
        {img.checksum ? <small className="plan-notes">Keep the page open: it shows the image's {img.checksum} checksum for step 3.</small> : <small className="plan-notes">If the page shows a checksum (SHA-256 or MD5), keep it for step 3.</small>}
        {img.tip && <p className="plan-notes">{img.tip}</p>}</li>

      <li><b>Choose the file you downloaded</b>
        <div className="actions"><button className="ghost" disabled={checking || !!hashing} onClick={() => void pick()}><FolderOpen size={15}/> {checking ? 'Checking…' : image ? 'Choose another file' : 'Choose the image file'}</button></div>
        {image && <div className={image.problem ? 'warn-text' : 'plan-notes'}>
          <p><b>{image.file_name}</b> ({fmtSize(image.bytes)}){image.system ? ` · ${SYSTEMS[image.system]?.name ?? image.system}${image.version ? ` ${image.version}` : ''}${image.target ? ` for ${image.target}` : ''}` : ''}</p>
          {image.problem ? <p>{image.problem}</p> : <p><CheckCircle2 size={14}/> It's a {image.format === 'img' ? 'disk image' : `${image.format} disk image`}.</p>}
          {image.notes.map(n => <p key={n} className="warn-text">{n}</p>)}</div>}
        {image && !image.problem && <div className="checksum"><label>Checksum from the download page (optional, recommended) <input className="design-new" value={expected} placeholder={img.checksum ? `${img.checksum} checksum` : 'SHA-256 or MD5 checksum'} onChange={e => { setExpected(e.target.value); setSum(null); }}/></label>
          <button className="ghost" disabled={!expected.trim() || !!hashing} onClick={() => void verify()}>{hashing ? `Checking… ${hashing.total ? Math.floor(hashing.done * 100 / hashing.total) : 0}%` : 'Check it'}</button>
          {sum && (sum.matches ? <p className="plan-notes"><CheckCircle2 size={14}/> {sum.algorithm} matches: this is the exact file from the download page.</p> : <p className="warn-text">The {sum.algorithm} doesn't match ({sum.actual}). The file is damaged or a different version: download it again before writing it.</p>)}</div>}</li>

      <li><b>Write it to the card or USB stick</b>
        {writers === null ? <p className="plan-notes">Looking for Raspberry Pi Imager and balenaEtcher…</p>
        : writers.length === 0 ? <><p className="plan-notes">Neither Raspberry Pi Imager nor balenaEtcher is installed. Install one (free), then come back:</p>
          <div className="actions"><button className="ghost" onClick={() => page('rpi-imager')}><ExternalLink size={15}/> Raspberry Pi Imager</button><button className="ghost" onClick={() => page('etcher')}><ExternalLink size={15}/> balenaEtcher</button><button className="ghost" onClick={() => void invoke<Writer[]>('install_writers').then(setWriters)}>Look again</button></div></>
        : <div className="actions">{writers.map(w => <button key={w.id} className="primary" disabled={!usable} onClick={() => void openWriter(w)}>Open {w.name}</button>)}</div>}
        {!usable && writers && writers.length > 0 && <small className="plan-notes">Choose a good image file first.</small>}
        {opened && <p className="plan-notes"><CheckCircle2 size={14}/> {opened}</p>}</li>

      <li><b>After writing</b><ul>{img.afterBoot.map(x => <li key={x}>{x}</li>)}</ul></li>
      </> : s.kind === 'app' && <>
      <li><b>Download it</b>Get it from the official site; RetroDrive doesn't download or ship it.
        <div className="actions"><button className="ghost" onClick={() => page(system)}><ExternalLink size={15}/> Open {s.name}'s download page</button></div></li>
      <li><b>Install it</b><ul>{s.install.map(x => <li key={x}>{x}</li>)}</ul></li>
      <li><b>Find it</b>RetroDrive looks for {s.name} on your drives; you choose it.
        <div className="actions"><button className="primary" onClick={onFind}><Search size={15}/> Find {s.name}</button></div></li>
      </>}
    </ol>
  </section>;
}
