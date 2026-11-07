// Install helper (RDS-027): put Batocera or RetroPie on a card or USB stick from an
// image the user downloaded. RetroDrive checks the file and opens Raspberry Pi Imager
// or balenaEtcher; the user picks the drive there. RetroDrive never writes to it.
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, ExternalLink, FolderOpen, X } from 'lucide-react';
import { fmtSize } from './format';

type ImageInfo = { path: string; file_name: string; bytes: number; format: string; system: string|null; version: string|null; target: string|null; problem: string|null; notes: string[] };
type Writer = { id: 'rpi-imager'|'etcher'; name: string; takes_image: boolean };
type Checksum = { algorithm: string; actual: string; matches: boolean };

const SYSTEMS = {
  batocera: { name: 'Batocera', hardware: 'PCs (x86_64) and many boards, including Raspberry Pi 3, 4 and 5', checksum: 'SHA-256', afterBoot: ['Put the card or USB stick in the machine and start it from it (on a PC, choose it in the boot menu).', 'The first start takes a few minutes while Batocera sets itself up.', 'Connect it to your network: games can then be copied to \\\\BATOCERA\\share from this PC.', 'Back here, choose Add existing installation and pick \\\\BATOCERA\\share (or the batocera folder on its game drive).'] },
  retropie: { name: 'RetroPie', hardware: 'Raspberry Pi (pick the image for your model)', checksum: 'MD5', afterBoot: ['Put the card in the Raspberry Pi and start it.', 'The first start expands the card and asks you to set up a controller.', 'Turn on its network share (RetroPie setup) so this PC can reach \\\\RETROPIE\\roms.', 'Copying to RetroPie comes with its connector (next on the roadmap).'] },
} as const;
type SystemId = keyof typeof SYSTEMS;

export default function InstallHelper({ notify, onClose }: { notify: (text: string, error?: boolean) => void; onClose: () => void }) {
  const [system, setSystem] = useState<SystemId>('batocera');
  const [image, setImage] = useState<ImageInfo|null>(null); const [checking, setChecking] = useState(false);
  const [expected, setExpected] = useState(''); const [sum, setSum] = useState<Checksum|null>(null); const [hashing, setHashing] = useState<{ done: number; total: number }|null>(null);
  const [writers, setWriters] = useState<Writer[]|null>(null); const [opened, setOpened] = useState('');
  const s = SYSTEMS[system];
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
    <div className="section-heading"><div><h2>Install a system</h2><p>Put Batocera or RetroPie on a card or USB stick. You download the official image; RetroDrive checks it and opens Raspberry Pi Imager or balenaEtcher to write it. <b>RetroDrive never writes to the card itself</b>: you choose the drive in that program, which warns before erasing it.</p></div><button className="ghost" onClick={onClose}><X size={16}/> Close</button></div>

    <ol className="install-steps">
      <li><b>Choose the system</b>
        <div className="actions">{(Object.keys(SYSTEMS) as SystemId[]).map(id => <button key={id} className={system === id ? 'primary' : 'ghost'} onClick={() => setSystem(id)}>{SYSTEMS[id].name}</button>)}</div>
        <small className="plan-notes">{s.name} runs on {s.hardware}.</small></li>

      <li><b>Download the image</b>Get it from the official site; RetroDrive doesn't download or ship it.
        <div className="actions"><button className="ghost" onClick={() => page(system)}><ExternalLink size={15}/> Open {s.name}'s download page</button></div>
        <small className="plan-notes">Keep the page open: it shows the image's {s.checksum} checksum for step 3.</small></li>

      <li><b>Choose the file you downloaded</b>
        <div className="actions"><button className="ghost" disabled={checking || !!hashing} onClick={() => void pick()}><FolderOpen size={15}/> {checking ? 'Checking…' : image ? 'Choose another file' : 'Choose the image file'}</button></div>
        {image && <div className={image.problem ? 'warn-text' : 'plan-notes'}>
          <p><b>{image.file_name}</b> ({fmtSize(image.bytes)}){image.system ? ` · ${image.system === 'batocera' ? 'Batocera' : 'RetroPie'}${image.version ? ` ${image.version}` : ''}${image.target ? ` for ${image.target}` : ''}` : ''}</p>
          {image.problem ? <p>{image.problem}</p> : <p><CheckCircle2 size={14}/> It's a {image.format === 'img' ? 'disk image' : `${image.format} disk image`}.</p>}
          {image.notes.map(n => <p key={n} className="warn-text">{n}</p>)}</div>}
        {image && !image.problem && <div className="checksum"><label>Checksum from the download page (optional, recommended) <input className="design-new" value={expected} placeholder={`${s.checksum} checksum`} onChange={e => { setExpected(e.target.value); setSum(null); }}/></label>
          <button className="ghost" disabled={!expected.trim() || !!hashing} onClick={() => void verify()}>{hashing ? `Checking… ${hashing.total ? Math.floor(hashing.done * 100 / hashing.total) : 0}%` : 'Check it'}</button>
          {sum && (sum.matches ? <p className="plan-notes"><CheckCircle2 size={14}/> {sum.algorithm} matches: this is the exact file from the download page.</p> : <p className="warn-text">The {sum.algorithm} doesn't match ({sum.actual}). The file is damaged or a different version: download it again before writing it.</p>)}</div>}</li>

      <li><b>Write it to the card or USB stick</b>
        {writers === null ? <p className="plan-notes">Looking for Raspberry Pi Imager and balenaEtcher…</p>
        : writers.length === 0 ? <><p className="plan-notes">Neither Raspberry Pi Imager nor balenaEtcher is installed. Install one (free), then come back:</p>
          <div className="actions"><button className="ghost" onClick={() => page('rpi-imager')}><ExternalLink size={15}/> Raspberry Pi Imager</button><button className="ghost" onClick={() => page('etcher')}><ExternalLink size={15}/> balenaEtcher</button><button className="ghost" onClick={() => void invoke<Writer[]>('install_writers').then(setWriters)}>Look again</button></div></>
        : <div className="actions">{writers.map(w => <button key={w.id} className="primary" disabled={!usable} onClick={() => void openWriter(w)}>Open {w.name}</button>)}</div>}
        {!usable && writers && writers.length > 0 && <small className="plan-notes">Choose a good image file first.</small>}
        {opened && <p className="plan-notes"><CheckCircle2 size={14}/> {opened}</p>}</li>

      <li><b>After writing</b><ul>{s.afterBoot.map(x => <li key={x}>{x}</li>)}</ul></li>
    </ol>
  </section>;
}
